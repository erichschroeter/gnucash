use clap::{Parser, Subcommand, ValueEnum};

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

    /// Optional path to a .gnucash data file to load.
    #[arg(name = "FILE")]
    pub file: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Import a GnuCash data file.
    Import {
        /// Path to the .gnucash file.
        path: String,
    },
}

/// Allowed verbosity levels for the logger.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum Verbosity {
    Debug,
    Info,
    Warn,
    Error,
}
