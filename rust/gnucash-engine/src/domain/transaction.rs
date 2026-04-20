use super::types::{AccountId, CommodityId, EngineError, Money, SplitId, TransactionId};
use chrono::{DateTime, Utc};
use num_traits::Zero;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Split {
    pub id: SplitId,
    pub account_id: AccountId,
    pub amount: Money,
    pub memo: String,
}

impl Split {
    pub fn new(account_id: AccountId, amount: Money) -> Self {
        Self {
            id: SplitId::new(),
            account_id,
            amount,
            memo: String::new(),
        }
    }

    pub fn with_memo(mut self, memo: impl Into<String>) -> Self {
        self.memo = memo.into();
        self
    }
}

/// A DraftTransaction is unvalidated and might not balance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftTransaction {
    pub id: TransactionId,
    pub date: DateTime<Utc>,
    pub description: String,
    pub splits: Vec<Split>,
    pub commodity_id: CommodityId,
}

impl DraftTransaction {
    pub fn new(commodity_id: CommodityId) -> Self {
        Self {
            id: TransactionId::new(),
            date: Utc::now(),
            description: String::new(),
            splits: Vec::new(),
            commodity_id,
        }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }

    pub fn with_date(mut self, date: DateTime<Utc>) -> Self {
        self.date = date;
        self
    }

    pub fn add_split(mut self, split: Split) -> Self {
        self.splits.push(split);
        self
    }

    /// Validates the transaction. If it balances to zero, returns a `Transaction`.
    /// Otherwise, returns an `EngineError::ImbalancedTransaction`.
    pub fn validate(self) -> Result<Transaction, EngineError> {
        if self.splits.is_empty() {
            return Ok(Transaction { inner: self }); // Allow empty transactions? GnuCash usually does, or maybe not. Let's say empty is 0.
        }

        let mut sum = Money::zero();
        for split in &self.splits {
            sum += split.amount;
        }

        if sum.is_zero() {
            Ok(Transaction { inner: self })
        } else {
            Err(EngineError::ImbalancedTransaction(sum))
        }
    }
}

/// A validated, guaranteed zero-sum transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transaction {
    inner: DraftTransaction,
}

impl Transaction {
    pub fn id(&self) -> TransactionId {
        self.inner.id
    }

    pub fn date(&self) -> DateTime<Utc> {
        self.inner.date
    }

    pub fn description(&self) -> &str {
        &self.inner.description
    }

    pub fn splits(&self) -> &[Split] {
        &self.inner.splits
    }

    pub fn commodity_id(&self) -> &CommodityId {
        &self.inner.commodity_id
    }

    /// Converts back to a draft for editing.
    pub fn into_draft(self) -> DraftTransaction {
        self.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use num_rational::Rational64;

    #[test]
    fn test_valid_transaction() {
        let commodity = CommodityId::new("USD");
        let mut draft = DraftTransaction::new(commodity);

        let account1 = AccountId::new();
        let account2 = AccountId::new();

        draft = draft.add_split(Split::new(account1, Rational64::new(100, 1)));
        draft = draft.add_split(Split::new(account2, Rational64::new(-100, 1)));

        let tx = draft.validate();
        assert!(tx.is_ok());
    }

    #[test]
    fn test_invalid_transaction() {
        let commodity = CommodityId::new("USD");
        let mut draft = DraftTransaction::new(commodity);

        let account1 = AccountId::new();
        let account2 = AccountId::new();

        draft = draft.add_split(Split::new(account1, Rational64::new(100, 1)));
        draft = draft.add_split(Split::new(account2, Rational64::new(-50, 1)));

        let tx = draft.validate();
        assert!(tx.is_err());
        match tx.unwrap_err() {
            EngineError::ImbalancedTransaction(amount) => {
                assert_eq!(amount, Rational64::new(50, 1));
            }
            _ => panic!("Wrong error type"),
        }
    }
}
