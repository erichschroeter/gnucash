use chrono::{DateTime, Utc};
use gnc_guid::GncGUID;
use gnc_numeric::{GncNumeric, GncNumericDenom, GncNumericRounding};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountType {
    ROOT,
    ASSET,
    BANK,
    CASH,
    CREDIT,
    LIABILITY,
    STOCK,
    MUTUAL,
    CURRENCY,
    INCOME,
    EXPENSE,
    EQUITY,
    RECEIVABLE,
    PAYABLE,
    TRADING,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub name: String,
    pub id: GncGUID,
    pub account_type: AccountType,
    pub parent_id: Option<GncGUID>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Split {
    pub id: GncGUID,
    pub account_id: GncGUID,
    pub value: GncNumeric,
    pub quantity: GncNumeric,
    pub reconciled: char,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub id: GncGUID,
    pub date_posted: DateTime<Utc>,
    pub description: String,
    pub splits: Vec<Split>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Book {
    pub id: GncGUID,
    pub accounts: HashMap<GncGUID, Account>,
    pub transactions: HashMap<GncGUID, Transaction>,
}

impl Book {
    pub fn new() -> Self {
        Self {
            id: GncGUID::new(),
            accounts: HashMap::new(),
            transactions: HashMap::new(),
        }
    }

    pub fn add_account(&mut self, account: Account) {
        self.accounts.insert(account.id, account);
    }

    pub fn add_transaction(&mut self, transaction: Transaction) {
        self.transactions.insert(transaction.id, transaction);
    }

    /// Returns a flat list of accounts.
    pub fn list_accounts(&self) -> Vec<&Account> {
        self.accounts.values().collect()
    }

    /// Get the parent of an account.
    pub fn get_parent(&self, account: &Account) -> Option<&Account> {
        account.parent_id.and_then(|id| self.accounts.get(&id))
    }

    /// List all transactions.
    pub fn list_transactions(&self) -> Vec<&Transaction> {
        self.transactions.values().collect()
    }

    /// Find accounts by name (partial match) or exact ID string.
    pub fn find_accounts(&self, query: &str) -> Vec<&Account> {
        let query_lower = query.to_lowercase();
        self.accounts
            .values()
            .filter(|a| a.name.to_lowercase().contains(&query_lower) || a.id.to_string() == query)
            .collect()
    }

    /// Calculate the balance of an account.
    pub fn calculate_balance(&self, account_id: GncGUID, recursive: bool) -> GncNumeric {
        let mut visited = HashSet::new();
        self.calculate_balance_internal(account_id, recursive, &mut visited)
    }

    fn calculate_balance_internal(
        &self,
        account_id: GncGUID,
        recursive: bool,
        visited: &mut HashSet<GncGUID>,
    ) -> GncNumeric {
        if !visited.insert(account_id) {
            // Cycle detected! Stop recursion here to avoid stack overflow.
            return GncNumeric::zero();
        }

        let mut balance = GncNumeric::zero();

        // Sum splits for this account
        for txn in self.transactions.values() {
            for split in &txn.splits {
                if split.account_id == account_id {
                    balance = GncNumeric::add(
                        balance,
                        split.value,
                        0,
                        GncNumericRounding::Never,
                        GncNumericDenom::Reduce,
                    );
                }
            }
        }

        if recursive {
            // Find children
            let children: Vec<GncGUID> = self
                .accounts
                .values()
                .filter(|a| a.parent_id == Some(account_id))
                .map(|a| a.id)
                .collect();

            for child_id in children {
                let child_bal = self.calculate_balance_internal(child_id, true, visited);
                balance = GncNumeric::add(
                    balance,
                    child_bal,
                    0,
                    GncNumericRounding::Never,
                    GncNumericDenom::Reduce,
                );
            }
        }

        balance
    }

    /// Check if an account is part of a circular hierarchy.
    pub fn is_circular(&self, account_id: GncGUID) -> bool {
        let mut current_id = account_id;
        let mut visited = HashSet::new();

        while let Some(account) = self.accounts.get(&current_id) {
            if !visited.insert(current_id) {
                return true;
            }
            if let Some(parent_id) = account.parent_id {
                current_id = parent_id;
            } else {
                break;
            }
        }
        false
    }

    /// Verify if a transaction is balanced (sum of splits == 0)
    pub fn is_transaction_balanced(&self, txn_id: GncGUID) -> bool {
        if let Some(txn) = self.transactions.get(&txn_id) {
            let mut sum = GncNumeric::zero();
            for split in &txn.splits {
                sum = GncNumeric::add(
                    sum,
                    split.value,
                    0,
                    GncNumericRounding::Never,
                    GncNumericDenom::Reduce,
                );
            }
            sum.num == 0
        } else {
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_create_account() {
        let mut book = Book::new();
        let root_id = GncGUID::new();
        let root = Account {
            name: "Root Account".to_string(),
            id: root_id,
            account_type: AccountType::ROOT,
            parent_id: None,
        };
        book.add_account(root);

        let bank_id = GncGUID::new();
        let bank = Account {
            name: "Checking".to_string(),
            id: bank_id,
            account_type: AccountType::BANK,
            parent_id: Some(root_id),
        };
        book.add_account(bank);

        assert_eq!(book.accounts.len(), 2);
        let bank_acc = book.accounts.get(&bank_id).unwrap();
        let parent = book.get_parent(bank_acc).unwrap();
        assert_eq!(parent.name, "Root Account");
    }

    #[test]
    fn test_balance_calculation_recursive() {
        let mut book = Book::new();
        let parent_id = GncGUID::new();
        let child_id = GncGUID::new();

        book.add_account(Account {
            name: "Assets".to_string(),
            id: parent_id,
            account_type: AccountType::ASSET,
            parent_id: None,
        });
        book.add_account(Account {
            name: "Bank".to_string(),
            id: child_id,
            account_type: AccountType::BANK,
            parent_id: Some(parent_id),
        });

        // Add 100 to child account
        book.add_transaction(Transaction {
            id: GncGUID::new(),
            date_posted: Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap(),
            description: "Deposit".to_string(),
            splits: vec![Split {
                id: GncGUID::new(),
                account_id: child_id,
                value: GncNumeric::new(100, 1),
                quantity: GncNumeric::new(100, 1),
                reconciled: 'n',
            }],
        });

        // Child balance should be 100
        assert_eq!(book.calculate_balance(child_id, false).num, 100);
        // Parent non-recursive balance should be 0
        assert_eq!(book.calculate_balance(parent_id, false).num, 0);
        // Parent recursive balance should be 100
        assert_eq!(book.calculate_balance(parent_id, true).num, 100);
    }

    #[test]
    fn test_circular_detection() {
        let mut book = Book::new();
        let id1 = GncGUID::new();
        let id2 = GncGUID::new();

        book.add_account(Account {
            name: "Account 1".to_string(),
            id: id1,
            account_type: AccountType::ASSET,
            parent_id: Some(id2),
        });
        book.add_account(Account {
            name: "Account 2".to_string(),
            id: id2,
            account_type: AccountType::ASSET,
            parent_id: Some(id1), // Cycle!
        });

        assert!(book.is_circular(id1));
        assert!(book.is_circular(id2));

        // calculate_balance should not overflow
        let bal = book.calculate_balance(id1, true);
        assert_eq!(bal.num, 0);
    }

    #[test]
    fn test_transaction_balancing() {
        let mut book = Book::new();
        let acc1 = GncGUID::new();
        let acc2 = GncGUID::new();
        let txn_id = GncGUID::new();

        let txn = Transaction {
            id: txn_id,
            date_posted: Utc::now(),
            description: "Double entry".to_string(),
            splits: vec![
                Split {
                    id: GncGUID::new(),
                    account_id: acc1,
                    value: GncNumeric::new(100, 1),
                    quantity: GncNumeric::new(100, 1),
                    reconciled: 'n',
                },
                Split {
                    id: GncGUID::new(),
                    account_id: acc2,
                    value: GncNumeric::new(-100, 1),
                    quantity: GncNumeric::new(-100, 1),
                    reconciled: 'n',
                },
            ],
        };
        book.add_transaction(txn);
        assert!(book.is_transaction_balanced(txn_id));

        // Unbalanced txn
        let bad_id = GncGUID::new();
        book.add_transaction(Transaction {
            id: bad_id,
            date_posted: Utc::now(),
            description: "Unbalanced".to_string(),
            splits: vec![Split {
                id: GncGUID::new(),
                account_id: acc1,
                value: GncNumeric::new(100, 1),
                quantity: GncNumeric::new(100, 1),
                reconciled: 'n',
            }],
        });
        assert!(!book.is_transaction_balanced(bad_id));
    }

    #[test]
    fn test_find_accounts() {
        let mut book = Book::new();
        let id = GncGUID::new();
        book.add_account(Account {
            name: "Savings Account".to_string(),
            id,
            account_type: AccountType::BANK,
            parent_id: None,
        });

        assert_eq!(book.find_accounts("SAVINGS").len(), 1);
        assert_eq!(book.find_accounts("account").len(), 1);
        assert_eq!(book.find_accounts(&id.to_string()).len(), 1);
        assert_eq!(book.find_accounts("Checking").len(), 0);
    }
}
