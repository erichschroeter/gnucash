use clap::{Parser, ValueEnum};

/// Command-line arguments for the gcash application.
#[derive(Parser, Debug)]
#[command(version, about = "Gcash CLI Application", long_about = None)]
pub struct Cli {
    /// Optional path to a YAML configuration file.
    #[arg(short, long)]
    pub config: Option<String>,

    /// Verbosity level for logging.
    #[arg(short, long, value_enum, default_value_t = Verbosity::Info)]
    pub verbosity: Verbosity,

    /// Enter the interactive terminal UI.
    #[arg(short, long)]
    pub interactive: bool,
}

/// Allowed verbosity levels for the logger.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum Verbosity {
    Debug,
    Info,
    Warn,
    Error,
}
