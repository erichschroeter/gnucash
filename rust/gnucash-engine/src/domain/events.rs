use super::account::Account;
use super::transaction::Transaction;
use super::types::{AccountId, TransactionId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    CreateAccount(Account),
    UpdateAccount(Account),
    DeleteAccount(AccountId),
    
    RecordTransaction(Transaction),
    UpdateTransaction(Transaction),
    DeleteTransaction(TransactionId),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    AccountCreated(Account),
    AccountUpdated(Account),
    AccountDeleted(AccountId),

    TransactionRecorded(Transaction),
    TransactionUpdated(Transaction),
    TransactionDeleted(TransactionId),
}
