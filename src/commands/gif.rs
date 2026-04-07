use anyhow::Result;

use crate::cli::GifArgs;
use crate::utils::ffmpeg::{ProcessSpec, render_filter_chain};
use crate::utils::file::{build_output_path, ensure_parent_dir, validate_output_options};
use crate::utils::runner::{AppContext, run_for_inputs};

pub fn run(context: &AppContext, args: &GifArgs) -> Result<()> {
    validate_output_options(&args.inputs, &args.output)?;

    run_for_inputs(context, &args.inputs, |input| {
        let plan = build_plan(context, args, input)?;
        context.execute_plan(&plan)
    })
}

pub fn build_plan(
    context: &AppContext,
    args: &GifArgs,
    input: &std::path::Path,
) -> Result<Vec<ProcessSpec>> {
    let output = build_output_path(input, &args.output, "", "gif")?;
    ensure_parent_dir(&output)?;

    let filter = render_filter_chain(&[
        format!("fps={}", args.fps),
        format!("scale={}:-1:flags=lanczos", args.width),
        "split[s0][s1]".into(),
        "[s0]palettegen=stats_mode=full[p]".into(),
        "[s1][p]paletteuse=dither=sierra2_4a".into(),
    ]);

    let mut ffmpeg_args = vec!["-hide_banner".into(), "-y".into()];

    if let Some(from) = args.from.as_deref() {
        ffmpeg_args.extend(["-ss".into(), from.into()]);
    }

    ffmpeg_args.extend(["-i".into(), input.display().to_string()]);

    if let Some(duration) = args.duration.as_deref() {
        ffmpeg_args.extend(["-t".into(), duration.into()]);
    }

    ffmpeg_args.extend(["-vf".into(), filter, output.display().to_string()]);

    Ok(vec![context.ffmpeg(ffmpeg_args)])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::OutputOptions;
    use crate::utils::log::Logger;

    #[test]
    fn gif_plan_uses_palette_pipeline() {
        let context = AppContext::new(
            false,
            1,
            "ffmpeg".into(),
            "ffprobe".into(),
            Logger::new(false),
        )
        .expect("context");
        let args = GifArgs {
            inputs: vec!["clip.mp4".into()],
            output: OutputOptions {
                output: None,
                output_dir: None,
            },
            fps: 12,
            width: 480,
            from: Some("00:00:03".into()),
            duration: Some("2.5".into()),
        };

        let plan = build_plan(&context, &args, std::path::Path::new("clip.mp4")).expect("plan");
        let rendered = plan[0].render();
        assert!(rendered.contains("palettegen"));
        assert!(rendered.contains("paletteuse"));
        assert!(rendered.contains("clip.gif"));
    }
}
