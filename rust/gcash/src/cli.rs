use clap::{Parser, Subcommand, ValueEnum};

/// Command-line arguments for the gcash application.
#[derive(Parser, Debug)]
#[command(version, about = "Gcash CLI Application", long_about = None)]
pub struct Cli {
    /// Optional path to a YAML configuration file.
    #[arg(short, long, global = true)]
    pub config: Option<String>,

    /// Verbosity level for logging.
    #[arg(short, long, value_enum, default_value_t = Verbosity::Info, global = true)]
    pub verbosity: Verbosity,

    /// Enter the interactive terminal UI.
    #[arg(short, long)]
    pub interactive: bool,

    /// Output the default configuration file and exit.
    #[arg(long)]
    pub default_config: bool,

    /// Optional path to a .gnucash data file to load.
    #[arg(short, long, global = true)]
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
    /// Manage accounts
    Accounts {
        #[command(subcommand)]
        command: Option<AccountsCommands>,
    },
    /// Manage transactions
    Transactions {
        #[command(subcommand)]
        command: Option<TransactionsCommands>,
    },
    /// Generate shell completions
    Completion {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand, Debug)]
pub enum AccountsCommands {
    /// List all accounts
    Ls,
}

#[derive(Subcommand, Debug)]
pub enum TransactionsCommands {
    /// List all transactions
    Ls,
}

/// Allowed verbosity levels for the logger.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug)]
pub enum Verbosity {
    Debug,
    Info,
    Warn,
    Error,
}
