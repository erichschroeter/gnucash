use anyhow::{Context, Result};
use clap::{CommandFactory, Parser};
use gcash::cli;
use gcash::config;
use gcash::tui;
use gnucash_engine::domain::{Ledger, Money};
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Parse CLI arguments
    let cli = cli::Cli::parse();

    // 1.5 Handle --default-config
    if cli.default_config {
        println!("{}", config::AppSettings::default_config_yaml());
        return Ok(());
    }

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
            cli::Commands::Accounts { command } => {
                let ledger = get_ledger(cli.file.as_ref(), &settings)?;
                let command = command.unwrap_or(cli::AccountsCommands::Ls);
                match command {
                    cli::AccountsCommands::Ls => {
                        println!("{:<40} {:<15} {:<10}", "Account Name", "Type", "ID");
                        println!("{}", "-".repeat(65));
                        for account in ledger.accounts {
                            println!(
                                "{:<40} {:<15} {:<10?}",
                                account.name,
                                format!("{:?}", account.account_type),
                                account.id
                            );
                        }
                    }
                }
            }
            cli::Commands::Transactions { command } => {
                let ledger = get_ledger(cli.file.as_ref(), &settings)?;
                let command = command.unwrap_or(cli::TransactionsCommands::Ls);
                match command {
                    cli::TransactionsCommands::Ls => {
                        println!("{:<12} {:<40} {:<10}", "Date", "Description", "ID");
                        println!("{}", "-".repeat(65));
                        for tx in ledger.transactions {
                            println!(
                                "{:<12} {:<40} {:<10?}",
                                tx.date().format("%Y-%m-%d"),
                                tx.description(),
                                tx.id()
                            );
                        }
                    }
                }
            }
            cli::Commands::Completion { shell } => {
                let mut cmd = cli::Cli::command();
                let bin_name = cmd.get_name().to_string();
                clap_complete::generate(shell, &mut cmd, bin_name, &mut std::io::stdout());
            }
        }
        return Ok(());
    }

    if cli.interactive {
        log::info!("Entering interactive mode");

        let ledger = get_ledger(cli.file.as_ref(), &settings)?;

        tui::run(ledger, settings)
            .await
            .context("Fatal error in interactive TUI loop")?;
    } else {
        log::info!("Running in foreground mode");
        println!("Gcash foreground executing...");
        println!(
            "Configuration loaded. Database URL: {:?}",
            settings.database_url
        );

        let test_money = Money::new(100, 1);
        println!("Testing gnucash-engine Money type: {:?}", test_money);

        println!("Run with --interactive to enter the UI.");
    }

    log::info!("Shutdown complete.");
    Ok(())
}

fn get_ledger(file_path: Option<&String>, settings: &config::AppSettings) -> Result<Ledger> {
    if let Some(path) = file_path {
        log::info!("Loading ledger from: {}", path);
        gnucash_engine::persistence::xml::load_from_path(path)
            .context(format!("Failed to load GnuCash file: {}", path))
    } else if let Some(url) = settings.database_url.as_ref() {
        log::info!("Loading ledger from config database_url: {}", url);
        gnucash_engine::persistence::xml::load_from_path(url)
            .context(format!("Failed to load GnuCash file from config: {}", url))
    } else {
        log::info!("No file or database URL provided, using empty default ledger.");
        Ok(gnucash_engine::domain::Ledger::default())
    }
}
