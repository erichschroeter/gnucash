use super::{Account, Transaction};

#[derive(Debug, Default, Clone)]
pub struct Ledger {
    pub accounts: Vec<Account>,
    pub transactions: Vec<Transaction>,
}

impl Ledger {
    pub fn new(accounts: Vec<Account>, transactions: Vec<Transaction>) -> Self {
        Self {
            accounts,
            transactions,
        }
    }
}
