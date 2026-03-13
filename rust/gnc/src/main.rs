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
    /// Transaction-related commands
    Transactions {
        #[command(subcommand)]
        action: TransactionActions,
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

#[derive(Subcommand)]
enum TransactionActions {
    /// List all transactions in a GnuCash file
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
        Commands::Transactions { action } => match action {
            TransactionActions::Ls { path } => {
                let book = load_gnucash_file(path)?;
                let mut table = Table::new();
                table.set_header(vec!["Date", "Description", "Value", "Splits"]);

                let mut txns: Vec<_> = book.list_transactions();
                // Sort by date
                txns.sort_by(|a, b| a.date_posted.cmp(&b.date_posted));

                for txn in txns {
                    // Calculate total value (absolute sum of positive splits)
                    let total_value: f64 = txn.splits.iter()
                        .filter(|s| s.value.num > 0)
                        .map(|s| s.value.to_f64())
                        .sum();

                    table.add_row(vec![
                        &txn.date_posted.format("%Y-%m-%d").to_string(),
                        &txn.description,
                        &format!("{:.2}", total_value),
                        &txn.splits.len().to_string(),
                    ]);
                }

                println!("{}", table);
            }
        },
    }

    Ok(())
}
