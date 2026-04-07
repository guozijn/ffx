use anyhow::Result;

use crate::cli::ThumbArgs;
use crate::utils::file::{build_output_path, ensure_parent_dir, validate_output_options};
use crate::utils::runner::{AppContext, run_for_inputs};

pub fn run(context: &AppContext, args: &ThumbArgs) -> Result<()> {
    validate_output_options(&args.inputs, &args.output)?;

    run_for_inputs(context, &args.inputs, |input| {
        let output = build_output_path(input, &args.output, "_thumb", "jpg")?;
        ensure_parent_dir(&output)?;

        let mut ffmpeg_args = vec!["-hide_banner".into(), "-y".into()];
        if let Some(at) = args.at.as_deref() {
            ffmpeg_args.extend(["-ss".into(), at.into()]);
        }

        ffmpeg_args.extend([
            "-i".into(),
            input.display().to_string(),
            "-vf".into(),
            format!("thumbnail=120,scale='min(iw,{})':-1", args.width),
            "-update".into(),
            "1".into(),
            "-frames:v".into(),
            "1".into(),
            output.display().to_string(),
        ]);

        context.execute_plan(&[context.ffmpeg(ffmpeg_args)])
    })
}
