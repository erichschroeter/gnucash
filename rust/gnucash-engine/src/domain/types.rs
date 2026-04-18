use serde::{Deserialize, Serialize};
use uuid::Uuid;
use num_rational::Rational64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(Uuid);

impl AccountId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransactionId(Uuid);

impl TransactionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SplitId(Uuid);

impl SplitId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommodityId(String);

impl CommodityId {
    pub fn new(id: &str) -> Self {
        Self(id.to_string())
    }
}

pub type Money = Rational64;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("Transaction is not balanced. Imbalance: {0}")]
    ImbalancedTransaction(Money),
    #[error("Account not found: {0:?}")]
    AccountNotFound(AccountId),
    #[error("Invalid split amount")]
    InvalidSplitAmount,
}
