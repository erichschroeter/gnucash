use thiserror::Error;

/// Core application errors specific to internal modules.
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Configuration error: {0}")]
    Config(#[from] config::ConfigError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TUI rendering error: {0}")]
    Tui(String),
}
