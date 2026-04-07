use anyhow::{Context, Result, bail};

use crate::cli::{CompressArgs, Preset};
use crate::utils::ffmpeg::{ProcessSpec, render_filter_chain};
use crate::utils::file::{build_output_path, ensure_parent_dir, validate_output_options};
use crate::utils::runner::{AppContext, run_for_inputs};

pub fn run(context: &AppContext, args: &CompressArgs) -> Result<()> {
    validate_output_options(&args.inputs, &args.output)?;

    run_for_inputs(context, &args.inputs, |input| {
        let plan = build_plan(context, args, input)?;
        context.execute_plan(&plan)
    })
}

pub fn build_plan(
    context: &AppContext,
    args: &CompressArgs,
    input: &std::path::Path,
) -> Result<Vec<ProcessSpec>> {
    let output = build_output_path(input, &args.output, "_compressed", "mp4")?;
    ensure_parent_dir(&output)?;

    let input_probe = context.probe_media(input).ok();
    let profile = CompressionProfile::from_args(args);
    let mut ffmpeg_args = vec![
        "-hide_banner".to_string(),
        "-y".to_string(),
        "-i".to_string(),
        input.display().to_string(),
    ];

    if let Some(target_video_bitrate) = profile.target_video_bitrate(context, input)? {
        ffmpeg_args.extend([
            "-c:v".into(),
            "libx264".into(),
            "-b:v".into(),
            target_video_bitrate.clone(),
            "-maxrate".into(),
            target_video_bitrate.clone(),
            "-bufsize".into(),
            format!("{}k", parse_kbps(&target_video_bitrate)? * 2),
        ]);
    } else {
        ffmpeg_args.extend([
            "-c:v".into(),
            "libx264".into(),
            "-preset".into(),
            profile.speed.clone(),
            "-crf".into(),
            profile.crf.to_string(),
        ]);
    }

    if let Some(filter) = profile.video_filter(input_probe.as_ref()) {
        ffmpeg_args.extend(["-vf".into(), filter]);
    }

    ffmpeg_args.extend([
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        "0:a:0?".into(),
        "-sn".into(),
        "-dn".into(),
        "-c:a".into(),
        "aac".into(),
        "-b:a".into(),
        profile.audio_bitrate,
        "-movflags".into(),
        "+faststart".into(),
        output.display().to_string(),
    ]);

    Ok(vec![context.ffmpeg(ffmpeg_args)])
}

#[derive(Debug, Clone)]
struct CompressionProfile {
    crf: u8,
    speed: String,
    max_height: Option<u32>,
    audio_bitrate: String,
    target_size_mb: Option<u64>,
}

impl CompressionProfile {
    fn from_args(args: &CompressArgs) -> Self {
        let mut profile = match args.preset.unwrap_or(Preset::Web) {
            Preset::Web => Self {
                crf: 23,
                speed: "medium".into(),
                max_height: Some(1080),
                audio_bitrate: "128k".into(),
                target_size_mb: args.target_size_mb,
            },
            Preset::Discord => Self {
                crf: 28,
                speed: "faster".into(),
                max_height: Some(1080),
                audio_bitrate: "128k".into(),
                target_size_mb: args.target_size_mb,
            },
            Preset::HighQuality => Self {
                crf: 18,
                speed: "slow".into(),
                max_height: Some(1440),
                audio_bitrate: "192k".into(),
                target_size_mb: args.target_size_mb,
            },
        };

        profile.crf = args.crf;
        profile.speed = args.speed.clone();
        profile.audio_bitrate = args.audio_bitrate.clone();

        if args.no_resize {
            profile.max_height = None;
        }

        if let Some(max_height) = args.max_height {
            profile.max_height = Some(max_height);
        }

        profile
    }

    fn video_filter(&self, probe: Option<&crate::utils::ffmpeg::MediaProbe>) -> Option<String> {
        self.max_height.map(|height| {
            let mut filters = Vec::new();
            let _rotation = probe.map(|value| value.rotation_degrees).unwrap_or(0);
            filters.push(format!(
                "scale='if(gt(iw,ih),if(gt(iw,{height}),{height},iw),if(gt(ih,{height}),trunc(iw*{height}/ih/2)*2,iw))':'if(gt(iw,ih),if(gt(iw,{height}),trunc(ih*{height}/iw/2)*2,ih),if(gt(ih,{height}),{height},ih))'"
            ));
            render_filter_chain(&filters)
        })
    }

    fn target_video_bitrate(
        &self,
        context: &AppContext,
        input: &std::path::Path,
    ) -> Result<Option<String>> {
        let Some(target_size_mb) = self.target_size_mb else {
            return Ok(None);
        };

        let duration = context.probe_media(input)?.duration_seconds.unwrap_or(0.0);
        if duration <= 0.0 {
            bail!("could not determine duration for target file size mode");
        }

        let audio_kbps = parse_kbps(&self.audio_bitrate)
            .with_context(|| format!("invalid audio bitrate '{}'", self.audio_bitrate))?;
        let total_kbps = ((target_size_mb as f64 * 8192.0) / duration).floor() as i64;
        let video_kbps = total_kbps - i64::from(audio_kbps);
        if video_kbps <= 0 {
            bail!("target file size is too small for the selected audio bitrate");
        }

        Ok(Some(format!("{video_kbps}k")))
    }
}

fn parse_kbps(value: &str) -> Result<u32> {
    let trimmed = value.trim_end_matches('k');
    Ok(trimmed.parse()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::OutputOptions;
    use crate::utils::log::Logger;

    #[test]
    fn compress_plan_contains_h264_and_faststart() {
        let context = AppContext::new(
            false,
            1,
            "ffmpeg".into(),
            "ffprobe".into(),
            Logger::new(false),
        )
        .expect("context");
        let args = CompressArgs {
            inputs: vec!["input.mov".into()],
            output: OutputOptions {
                output: None,
                output_dir: None,
            },
            crf: 23,
            speed: "medium".into(),
            no_resize: false,
            max_height: None,
            preset: Some(Preset::Web),
            target_size_mb: None,
            audio_bitrate: "128k".into(),
        };

        let plan = build_plan(&context, &args, std::path::Path::new("input.mov")).expect("plan");
        let rendered = plan[0].render();
        assert!(rendered.contains("libx264"));
        assert!(rendered.contains("+faststart"));
        assert!(rendered.contains("input_compressed.mp4"));
    }
}
