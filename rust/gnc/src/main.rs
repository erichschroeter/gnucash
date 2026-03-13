use clap::{Parser, Subcommand};
use comfy_table::Table;
use gnc_xml::load_gnucash_file;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "gnc")]
#[command(about = "A GnuCash CLI tool in Rust", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Account-related commands
    Accounts {
        #[command(subcommand)]
        action: AccountActions,
    },
}

#[derive(Subcommand)]
enum AccountActions {
    /// List all accounts in a GnuCash file
    Ls {
        /// The path to the .gnucash file
        path: PathBuf,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Accounts { action } => match action {
            AccountActions::Ls { path } => {
                let book = load_gnucash_file(path)?;
                let mut table = Table::new();
                table.set_header(vec!["Name", "Type", "ID"]);

                let mut accounts: Vec<_> = book.list_accounts();
                // Sort by name for better readability
                accounts.sort_by(|a, b| a.name.cmp(&b.name));

                for account in accounts {
                    table.add_row(vec![
                        &account.name,
                        &format!("{:?}", account.account_type),
                        &account.id.to_string(),
                    ]);
                }

                println!("{}", table);
            }
        },
    }

    Ok(())
}
