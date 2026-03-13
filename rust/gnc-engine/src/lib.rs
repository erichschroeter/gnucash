use gnc_guid::GncGUID;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Book {
    pub id: GncGUID,
    pub accounts: HashMap<GncGUID, Account>,
}

impl Book {
    pub fn new() -> Self {
        Self {
            id: GncGUID::new(),
            accounts: HashMap::new(),
        }
    }

    pub fn add_account(&mut self, account: Account) {
        self.accounts.insert(account.id, account);
    }

    /// Returns a flat list of accounts.
    pub fn list_accounts(&self) -> Vec<&Account> {
        self.accounts.values().collect()
    }

    /// Get the parent of an account.
    pub fn get_parent(&self, account: &Account) -> Option<&Account> {
        account.parent_id.and_then(|id| self.accounts.get(&id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
