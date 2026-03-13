use gnc_guid::GncGUID;
use gnc_numeric::GncNumeric;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

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
        self.accounts.values()
            .filter(|a| {
                a.name.to_lowercase().contains(&query_lower) || 
                a.id.to_string() == query
            })
            .collect()
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
    fn test_find_accounts() {
        let mut book = Book::new();
        let id = GncGUID::new();
        book.add_account(Account {
            name: "Test Account".to_string(),
            id,
            account_type: AccountType::BANK,
            parent_id: None,
        });

        assert_eq!(book.find_accounts("Test").len(), 1);
        assert_eq!(book.find_accounts(&id.to_string()).len(), 1);
        assert_eq!(book.find_accounts("Nonexistent").len(), 0);
    }

    #[test]
    fn test_create_transaction() {
        let mut book = Book::new();
        let acc_id = GncGUID::new();
        
        let split = Split {
            id: GncGUID::new(),
            account_id: acc_id,
            value: GncNumeric::new(100, 1),
            quantity: GncNumeric::new(100, 1),
            reconciled: 'n',
        };

        let txn = Transaction {
            id: GncGUID::new(),
            date_posted: Utc.with_ymd_and_hms(2024, 10, 7, 10, 59, 0).unwrap(),
            description: "Test Txn".to_string(),
            splits: vec![split],
        };

        book.add_transaction(txn);
        assert_eq!(book.transactions.len(), 1);
    }
}
