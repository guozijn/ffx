use clap::Parser;

fn main() {
    let cli = ffx::cli::Cli::parse();

    if let Err(error) = ffx::run(cli) {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
