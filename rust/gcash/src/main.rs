mod cli;
mod config;
mod error;
mod tui;

use anyhow::{Context, Result};
use clap::Parser;
use std::env;
use gnucash_engine::domain::Money;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Parse CLI arguments
    let cli = cli::Cli::parse();

    // 2. Setup Logging: CLI --verbosity flag strictly overrides RUST_LOG
    let log_level = match cli.verbosity {
        cli::Verbosity::Debug => "debug",
        cli::Verbosity::Info => "info",
        cli::Verbosity::Warn => "warn",
        cli::Verbosity::Error => "error",
    };
    env::set_var("RUST_LOG", log_level);
    env_logger::init();

    log::info!("Starting gcash CLI...");

    // 3. Load Configuration using Anyhow to provide high-level error context
    let settings = config::load_config(cli.config.as_ref())
        .context("Failed to load application configuration")?;
    log::debug!("Loaded settings: {:?}", settings);

    // 4. Execute Mode
    if let Some(command) = cli.command {
        match command {
            cli::Commands::Import { path } => {
                log::info!("Importing data from: {}", path);
                match gnucash_engine::persistence::xml::load_from_path(&path) {
                    Ok(ledger) => {
                        println!("Successfully parsed GnuCash XML file.");
                        println!("Found {} accounts.", ledger.accounts.len());
                        println!("Found {} transactions.", ledger.transactions.len());
                    }
                    Err(e) => {
                        eprintln!("Error importing file: {}", e);
                        std::process::exit(1);
                    }
                }
            }
        }
        return Ok(());
    }

    if cli.interactive {
        log::info!("Entering interactive mode");
        
        let ledger = if let Some(path) = cli.file.as_ref() {
            log::info!("Loading ledger from: {}", path);
            gnucash_engine::persistence::xml::load_from_path(path)
                .context(format!("Failed to load GnuCash file: {}", path))?
        } else if let Some(url) = settings.database_url.as_ref() {
            log::info!("Loading ledger from config database_url: {}", url);
            gnucash_engine::persistence::xml::load_from_path(url)
                .context(format!("Failed to load GnuCash file from config: {}", url))?
        } else {
            gnucash_engine::domain::Ledger::default()
        };

        tui::run(ledger).await.context("Fatal error in interactive TUI loop")?;
    } else {
        log::info!("Running in foreground mode");
        println!("Gcash foreground executing...");
        println!("Configuration loaded. Database URL: {:?}", settings.database_url);
        
        let test_money = Money::new(100, 1);
        println!("Testing gnucash-engine Money type: {:?}", test_money);
        
        println!("Run with --interactive to enter the UI.");
    }

    log::info!("Shutdown complete.");
    Ok(())
}
