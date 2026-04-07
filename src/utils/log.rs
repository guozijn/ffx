use owo_colors::OwoColorize;

#[derive(Debug, Clone)]
pub struct Logger {
    verbose: bool,
}

impl Logger {
    pub fn new(verbose: bool) -> Self {
        Self { verbose }
    }

    pub fn info(&self, message: impl AsRef<str>) {
        println!("{} {}", "info".blue().bold(), message.as_ref());
    }

    pub fn success(&self, message: impl AsRef<str>) {
        println!("{} {}", "ok".green().bold(), message.as_ref());
    }

    pub fn warn(&self, message: impl AsRef<str>) {
        eprintln!("{} {}", "warn".yellow().bold(), message.as_ref());
    }

    pub fn command(&self, message: impl AsRef<str>) {
        println!("{} {}", "cmd".magenta().bold(), message.as_ref());
    }

    pub fn debug(&self, message: impl AsRef<str>) {
        if self.verbose {
            println!("{} {}", "debug".cyan().bold(), message.as_ref());
        }
    }
}
