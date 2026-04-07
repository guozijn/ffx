use std::cmp::Ordering;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use tempfile::TempDir;

use crate::cli::CutArgs;
use crate::utils::ffmpeg::{MediaProbe, ProcessSpec};
use crate::utils::file::{
    build_output_path, build_segment_output_path, ensure_parent_dir, validate_output_options,
};
use crate::utils::runner::{AppContext, run_for_inputs};

pub fn run(context: &AppContext, args: &CutArgs) -> Result<()> {
    validate_output_options(&args.inputs, &args.output)?;

    run_for_inputs(context, &args.inputs, |input| {
        run_single_input(context, args, input)
    })
}

fn run_single_input(context: &AppContext, args: &CutArgs, input: &Path) -> Result<()> {
    let input_probe = context.probe_media(input)?;
    let segments = parse_segments(args)?;
    let precise_segments = segments.len() > 1;

    if args.split {
        for (index, segment) in segments.iter().enumerate() {
            let output = build_segment_output_path(input, &args.output, index, "mp4")?;
            ensure_parent_dir(&output)?;
            if precise_segments {
                run_trim_command(context, input, segment, &output, true)
                    .and_then(|_| validate_output(context, &input_probe, &output, segment.duration_seconds()))?;
            } else {
                run_trim_with_fallback(context, input, &input_probe, segment, &output, args)?;
            }
        }
        return Ok(());
    }

    if segments.len() == 1 {
        let output = build_output_path(input, &args.output, "_cut", "mp4")?;
        ensure_parent_dir(&output)?;
        return run_trim_with_fallback(context, input, &input_probe, &segments[0], &output, args);
    }

    let output = build_output_path(input, &args.output, "_cut", "mp4")?;
    ensure_parent_dir(&output)?;

    let temp_dir = TempDir::new().context("failed to create temporary directory for cut")?;
    let (part_files, extract_specs) =
        build_extract_specs(context, input, &segments, temp_dir.path(), true);

    context.execute_plan(&extract_specs)?;

    let concat_list = write_concat_list(temp_dir.path(), &part_files)?;
    let merge_copy = context.ffmpeg(vec![
        "-hide_banner".into(),
        "-y".into(),
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-i".into(),
        concat_list.display().to_string(),
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        "0:a:0?".into(),
        "-sn".into(),
        "-dn".into(),
        "-c".into(),
        "copy".into(),
        "-movflags".into(),
        "+faststart".into(),
        output.display().to_string(),
    ]);

    match context.execute_step(&merge_copy).and_then(|_| {
        validate_output(
            context,
            &input_probe,
            &output,
            segments.iter().map(Segment::duration_seconds).sum(),
        )
    }) {
        Ok(()) => Ok(()),
        Err(_error) if args.fallback_reencode => {
            context.logger.warn(format!(
                "concat copy verification failed for {}; retrying with re-encode",
                input.display()
            ));
            let merge_reencode = context.ffmpeg(vec![
                "-hide_banner".into(),
                "-y".into(),
                "-f".into(),
                "concat".into(),
                "-safe".into(),
                "0".into(),
                "-i".into(),
                concat_list.display().to_string(),
                "-map".into(),
                "0:v:0".into(),
                "-map".into(),
                "0:a:0?".into(),
                "-sn".into(),
                "-dn".into(),
                "-c:v".into(),
                "libx264".into(),
                "-crf".into(),
                "20".into(),
                "-preset".into(),
                "medium".into(),
                "-c:a".into(),
                "aac".into(),
                "-b:a".into(),
                "192k".into(),
                "-movflags".into(),
                "+faststart".into(),
                output.display().to_string(),
            ]);
            context.execute_step(&merge_reencode).and_then(|_| {
                validate_output(
                    context,
                    &input_probe,
                    &output,
                    segments.iter().map(Segment::duration_seconds).sum(),
                )
            })
        }
        Err(error) => Err(error),
    }
}

fn run_trim_with_fallback(
    context: &AppContext,
    input: &Path,
    input_probe: &MediaProbe,
    segment: &Segment,
    output: &Path,
    args: &CutArgs,
) -> Result<()> {
    match run_trim_command(context, input, segment, output, args.reencode)
        .and_then(|_| validate_output(context, input_probe, output, segment.duration_seconds()))
    {
        Ok(()) => Ok(()),
        Err(_error) if args.fallback_reencode && !args.reencode => {
            context.logger.warn(format!(
                "copy trim verification failed for {}; retrying with re-encode",
                input.display()
            ));
            run_trim_command(context, input, segment, output, true).and_then(|_| {
                validate_output(context, input_probe, output, segment.duration_seconds())
            })
        }
        Err(error) => Err(error),
    }
}

fn run_trim_command(
    context: &AppContext,
    input: &Path,
    segment: &Segment,
    output: &Path,
    reencode: bool,
) -> Result<()> {
    let duration = segment.duration_render();
    let mut args = vec![
        "-hide_banner".into(),
        "-y".into(),
        "-ss".into(),
        segment.start.render(),
        "-i".into(),
        input.display().to_string(),
        "-t".into(),
        duration,
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        "0:a:0?".into(),
        "-sn".into(),
        "-dn".into(),
    ];

    if reencode {
        args.extend([
            "-c:v".into(),
            "libx264".into(),
            "-crf".into(),
            "20".into(),
            "-preset".into(),
            "medium".into(),
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            "192k".into(),
            "-movflags".into(),
            "+faststart".into(),
        ]);
    } else {
        args.extend([
            "-c:v".into(),
            "copy".into(),
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            "192k".into(),
            "-af".into(),
            "aresample=async=1:first_pts=0".into(),
            "-reset_timestamps".into(),
            "1".into(),
            "-movflags".into(),
            "+faststart".into(),
        ]);
    }

    args.push(output.display().to_string());
    context.execute_plan(&[context.ffmpeg(args)])
}

fn build_extract_specs(
    context: &AppContext,
    input: &Path,
    segments: &[Segment],
    temp_dir: &Path,
    reencode: bool,
) -> (Vec<PathBuf>, Vec<ProcessSpec>) {
    let mut files = Vec::with_capacity(segments.len());
    let mut specs = Vec::with_capacity(segments.len());

    for (index, segment) in segments.iter().enumerate() {
        let part_file = temp_dir.join(format!("part_{index:03}.mp4"));
        files.push(part_file.clone());

        let mut args = vec![
            "-hide_banner".into(),
            "-y".into(),
            "-ss".into(),
            segment.start.render(),
            "-i".into(),
            input.display().to_string(),
            "-t".into(),
            segment.duration_render(),
            "-map".into(),
            "0:v:0".into(),
            "-map".into(),
            "0:a:0?".into(),
            "-sn".into(),
            "-dn".into(),
        ];

        if reencode {
            args.extend([
                "-c:v".into(),
                "libx264".into(),
                "-crf".into(),
                "20".into(),
                "-preset".into(),
                "medium".into(),
                "-c:a".into(),
                "aac".into(),
                "-b:a".into(),
                "192k".into(),
                "-movflags".into(),
                "+faststart".into(),
            ]);
        } else {
            args.extend([
                "-c:v".into(),
                "copy".into(),
                "-c:a".into(),
                "aac".into(),
                "-b:a".into(),
                "192k".into(),
                "-af".into(),
                "aresample=async=1:first_pts=0".into(),
                "-reset_timestamps".into(),
                "1".into(),
                "-movflags".into(),
                "+faststart".into(),
            ]);
        }

        args.push(part_file.display().to_string());
        specs.push(context.ffmpeg(args));
    }

    (files, specs)
}

fn write_concat_list(temp_dir: &Path, files: &[PathBuf]) -> Result<PathBuf> {
    let concat_list = temp_dir.join("concat.txt");
    let mut content = String::new();

    for file in files {
        content.push_str("file '");
        content.push_str(&file.to_string_lossy().replace('\'', "'\\''"));
        content.push_str("'\n");
    }

    fs::write(&concat_list, content).context("failed to write concat list")?;
    Ok(concat_list)
}

fn parse_segments(args: &CutArgs) -> Result<Vec<Segment>> {
    let mut segments = if !args.segments.is_empty() {
        args.segments
            .iter()
            .map(|value| Segment::parse(value))
            .collect::<Result<Vec<_>>>()?
    } else {
        let from = args
            .from
            .as_deref()
            .ok_or_else(|| anyhow!("either --segment or --from/--to is required"))?;
        let to = args
            .to
            .as_deref()
            .ok_or_else(|| anyhow!("either --segment or --from/--to is required"))?;
        vec![Segment::new(
            TimePoint::parse(from)?,
            TimePoint::parse(to)?,
        )?]
    };

    if args.sort_segments {
        segments.sort_by(|left, right| {
            left.start
                .partial_cmp(&right.start)
                .unwrap_or(Ordering::Equal)
        });
    }

    validate_segments(&segments)?;
    Ok(segments)
}

fn validate_output(
    context: &AppContext,
    input_probe: &MediaProbe,
    output: &Path,
    expected_duration_seconds: f64,
) -> Result<()> {
    if context.dry_run {
        return Ok(());
    }

    let probe = context.probe_media(output)?;
    if input_probe.audio_streams > 0 && probe.audio_streams == 0 {
        bail!("output is missing audio");
    }

    let actual_duration = probe.duration_seconds.unwrap_or(0.0);
    let drift = (actual_duration - expected_duration_seconds).abs();
    if drift > 0.05 {
        bail!(
            "output duration drifted too much; expected about {:.3}s, got {:.3}s",
            expected_duration_seconds,
            actual_duration
        );
    }

    Ok(())
}

fn validate_segments(segments: &[Segment]) -> Result<()> {
    if segments.is_empty() {
        bail!("at least one segment is required");
    }

    for window in segments.windows(2) {
        let left = &window[0];
        let right = &window[1];
        if left.end > right.start {
            bail!(
                "overlapping segments are not allowed: {}-{} overlaps with {}-{}",
                left.start.render(),
                left.end.render(),
                right.start.render(),
                right.end.render()
            );
        }
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
struct Segment {
    start: TimePoint,
    end: TimePoint,
}

impl Segment {
    fn parse(value: &str) -> Result<Self> {
        let (start, end) = value
            .split_once('-')
            .ok_or_else(|| anyhow!("invalid segment '{value}'; expected START-END"))?;
        Self::new(TimePoint::parse(start)?, TimePoint::parse(end)?)
    }

    fn new(start: TimePoint, end: TimePoint) -> Result<Self> {
        if end <= start {
            bail!("segment end must be after start");
        }

        Ok(Self { start, end })
    }

    fn duration_seconds(&self) -> f64 {
        self.end.0 - self.start.0
    }

    fn duration_render(&self) -> String {
        TimePoint(self.duration_seconds()).render()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
struct TimePoint(f64);

impl TimePoint {
    fn parse(value: &str) -> Result<Self> {
        if value.contains(':') {
            let parts = value.split(':').map(str::trim).collect::<Vec<_>>();
            if parts.is_empty() || parts.len() > 3 {
                bail!("invalid timestamp '{value}'");
            }

            let mut seconds = 0.0;
            for part in parts {
                seconds = (seconds * 60.0)
                    + part
                        .parse::<f64>()
                        .with_context(|| format!("invalid timestamp '{value}'"))?;
            }
            Ok(Self(seconds))
        } else {
            Ok(Self(
                value
                    .trim()
                    .parse::<f64>()
                    .with_context(|| format!("invalid timestamp '{value}'"))?,
            ))
        }
    }

    fn render(self) -> String {
        let whole = self.0.trunc() as u64;
        let fraction = self.0.fract();
        let hours = whole / 3600;
        let minutes = (whole % 3600) / 60;
        let seconds = whole % 60;

        if fraction.abs() > f64::EPSILON {
            format!("{hours:02}:{minutes:02}:{:06.3}", seconds as f64 + fraction)
        } else {
            format!("{hours:02}:{minutes:02}:{seconds:02}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_seconds_and_hms_segments() {
        let segment = Segment::parse("10-25.5").expect("seconds");
        assert_eq!(segment.start.render(), "00:00:10");
        assert_eq!(segment.end.render(), "00:00:25.500");

        let hms = Segment::parse("00:01:00-00:01:30").expect("hms");
        assert_eq!(hms.start.render(), "00:01:00");
        assert_eq!(hms.end.render(), "00:01:30");
    }

    #[test]
    fn rejects_overlapping_segments() {
        let segments = vec![
            Segment::parse("00:00:01-00:00:05").expect("segment"),
            Segment::parse("00:00:04-00:00:06").expect("segment"),
        ];
        assert!(validate_segments(&segments).is_err());
    }
}
