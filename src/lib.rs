pub mod cli;
pub mod commands;
pub mod utils;

use anyhow::Result;
use cli::{Cli, Commands};
use utils::log::Logger;
use utils::runner::AppContext;

pub fn run(cli: Cli) -> Result<()> {
    let logger = Logger::new(cli.verbose);
    let context = AppContext::new(
        cli.dry_run,
        cli.jobs,
        cli.ffmpeg_bin,
        cli.ffprobe_bin,
        logger,
    )?;

    match cli.command {
        Commands::Compress(args) => commands::compress::run(&context, &args),
        Commands::ToMp4(args) => commands::to_mp4::run(&context, &args),
        Commands::Gif(args) => commands::gif::run(&context, &args),
        Commands::Audio(args) => commands::audio::run(&context, &args),
        Commands::Thumb(args) => commands::thumb::run(&context, &args),
        Commands::Cut(args) => commands::cut::run(&context, &args),
    }
}
