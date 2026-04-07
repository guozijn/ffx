use anyhow::Result;

use crate::cli::{Preset, ToMp4Args};
use crate::utils::ffmpeg::{MediaProbe, is_mp4_audio_compatible, is_mp4_video_compatible};
use crate::utils::file::{build_output_path, ensure_parent_dir, validate_output_options};
use crate::utils::runner::{AppContext, run_for_inputs};

pub fn run(context: &AppContext, args: &ToMp4Args) -> Result<()> {
    validate_output_options(&args.inputs, &args.output)?;

    run_for_inputs(context, &args.inputs, |input| {
        let output = build_output_path(input, &args.output, "", "mp4")?;
        ensure_parent_dir(&output)?;

        let probe = context.probe_media(input)?;
        let should_copy = !args.reencode && can_stream_copy_to_mp4(&probe);
        let ffmpeg_args = if should_copy {
            vec![
                "-hide_banner".into(),
                "-y".into(),
                "-i".into(),
                input.display().to_string(),
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
            ]
        } else {
            let profile = args.preset.unwrap_or(Preset::Web);
            let (crf, speed) = match profile {
                Preset::Web => (23, "medium"),
                Preset::Discord => (28, "faster"),
                Preset::HighQuality => (18, "slow"),
            };

            vec![
                "-hide_banner".into(),
                "-y".into(),
                "-i".into(),
                input.display().to_string(),
                "-map".into(),
                "0:v:0".into(),
                "-map".into(),
                "0:a:0?".into(),
                "-sn".into(),
                "-dn".into(),
                "-c:v".into(),
                "libx264".into(),
                "-crf".into(),
                crf.to_string(),
                "-preset".into(),
                speed.into(),
                "-c:a".into(),
                "aac".into(),
                "-b:a".into(),
                "192k".into(),
                "-movflags".into(),
                "+faststart".into(),
                output.display().to_string(),
            ]
        };

        context.execute_plan(&[context.ffmpeg(ffmpeg_args)])
    })
}

fn can_stream_copy_to_mp4(probe: &MediaProbe) -> bool {
    probe
        .video_codec
        .as_deref()
        .is_some_and(is_mp4_video_compatible)
        && probe
            .audio_codec
            .as_deref()
            .map(is_mp4_audio_compatible)
            .unwrap_or(true)
}
