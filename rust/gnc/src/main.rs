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
        /// Show balance for each account
        #[arg(short, long)]
        balance: bool,
    },
    /// Show balance for all accounts
    Balance {
        /// The path to the .gnucash file
        path: PathBuf,
        /// Include sub-account balances
        #[arg(short, long)]
        recursive: bool,
    }
}

#[derive(Subcommand)]
enum TransactionActions {
    /// List all transactions in a GnuCash file
    Ls {
        /// The path to the .gnucash file
        path: PathBuf,
        /// Filter by account name or ID
        #[arg(short, long)]
        account: Option<String>,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Accounts { action } => match action {
            AccountActions::Ls { path, balance } => {
                let book = load_gnucash_file(path)?;
                let mut table = Table::new();
                if *balance {
                    table.set_header(vec!["Name", "Type", "Balance"]);
                } else {
                    table.set_header(vec!["Name", "Type", "ID"]);
                }

                let mut accounts: Vec<_> = book.list_accounts();
                accounts.sort_by(|a, b| a.name.cmp(&b.name));

                for account in accounts {
                    if *balance {
                        let bal = book.calculate_balance(account.id, false);
                        table.add_row(vec![
                            &account.name,
                            &format!("{:?}", account.account_type),
                            &format!("{:.2}", bal.to_f64()),
                        ]);
                    } else {
                        table.add_row(vec![
                            &account.name,
                            &format!("{:?}", account.account_type),
                            &account.id.to_string(),
                        ]);
                    }
                }

                println!("{}", table);
            }
            AccountActions::Balance { path, recursive } => {
                let book = load_gnucash_file(path)?;
                let mut table = Table::new();
                table.set_header(vec!["Account", "Type", "Balance"]);

                let mut accounts: Vec<_> = book.list_accounts();
                accounts.sort_by(|a, b| a.name.cmp(&b.name));

                for account in accounts {
                    let bal = book.calculate_balance(account.id, *recursive);
                    // Only show non-zero balances for a cleaner view
                    if bal.num != 0 {
                        table.add_row(vec![
                            &account.name,
                            &format!("{:?}", account.account_type),
                            &format!("{:.2}", bal.to_f64()),
                        ]);
                    }
                }

                println!("{}", table);
            }
        },
        Commands::Transactions { action } => match action {
            TransactionActions::Ls { path, account } => {
                let book = load_gnucash_file(path)?;
                let mut table = Table::new();
                
                let filter_account_ids: Vec<_> = if let Some(q) = account {
                    let accs = book.find_accounts(q);
                    if accs.is_empty() {
                        eprintln!("No accounts found matching '{}'", q);
                        return Ok(());
                    }
                    accs.iter().map(|a| a.id).collect()
                } else {
                    Vec::new()
                };

                if filter_account_ids.is_empty() {
                    table.set_header(vec!["Date", "Description", "Value", "Splits"]);
                } else {
                    table.set_header(vec!["Date", "Description", "Transfer", "Amount", "Balance", "R"]);
                }

                let mut txns: Vec<_> = book.list_transactions();
                if !filter_account_ids.is_empty() {
                    txns.retain(|txn| {
                        txn.splits.iter().any(|s| filter_account_ids.contains(&s.account_id))
                    });
                }

                txns.sort_by(|a, b| a.date_posted.cmp(&b.date_posted));

                let mut running_balance = 0.0;

                for txn in txns {
                    if filter_account_ids.is_empty() {
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
                    } else {
                        let split = txn.splits.iter().find(|s| filter_account_ids.contains(&s.account_id)).unwrap();
                        let amount = split.value.to_f64();
                        running_balance += amount;
                        
                        let transfer = if txn.splits.len() == 2 {
                            let other = txn.splits.iter().find(|s| !filter_account_ids.contains(&s.account_id));
                            match other {
                                Some(s) => book.accounts.get(&s.account_id).map(|a| a.name.as_str()).unwrap_or("--Unknown--"),
                                None => "--Split--",
                            }
                        } else {
                            "--Split--"
                        };

                        table.add_row(vec![
                            &txn.date_posted.format("%Y-%m-%d").to_string(),
                            &txn.description,
                            transfer,
                            &format!("{:.2}", amount),
                            &format!("{:.2}", running_balance),
                            &split.reconciled.to_string(),
                        ]);
                    }
                }

                println!("{}", table);
            }
        },
    }

    Ok(())
}
