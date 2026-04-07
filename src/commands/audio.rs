use anyhow::Result;

use crate::cli::{AudioArgs, AudioFormat};
use crate::utils::file::{build_output_path, ensure_parent_dir, validate_output_options};
use crate::utils::runner::{AppContext, run_for_inputs};

pub fn run(context: &AppContext, args: &AudioArgs) -> Result<()> {
    validate_output_options(&args.inputs, &args.output)?;

    run_for_inputs(context, &args.inputs, |input| {
        let output = build_output_path(input, &args.output, "", output_extension(args.format))?;
        ensure_parent_dir(&output)?;

        let codec = match args.format {
            AudioFormat::Mp3 => "libmp3lame",
            AudioFormat::M4a => "aac",
        };

        let plan = vec![context.ffmpeg(vec![
            "-hide_banner".into(),
            "-y".into(),
            "-i".into(),
            input.display().to_string(),
            "-vn".into(),
            "-c:a".into(),
            codec.into(),
            "-b:a".into(),
            args.bitrate.clone(),
            output.display().to_string(),
        ])];

        context.execute_plan(&plan)
    })
}

fn output_extension(format: AudioFormat) -> &'static str {
    match format {
        AudioFormat::Mp3 => "mp3",
        AudioFormat::M4a => "m4a",
    }
}
