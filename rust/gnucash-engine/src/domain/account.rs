use super::types::{AccountId, CommodityId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountType {
    Bank,
    Cash,
    Credit,
    Asset,
    Liability,
    Income,
    Expense,
    Equity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub parent_id: Option<AccountId>,
    pub account_type: AccountType,
    pub commodity_id: CommodityId,
    pub name: String,
    pub description: String,
}

impl Account {
    pub fn new(
        name: impl Into<String>,
        account_type: AccountType,
        commodity_id: CommodityId,
    ) -> Self {
        Self {
            id: AccountId::new(),
            parent_id: None,
            account_type,
            commodity_id,
            name: name.into(),
            description: String::new(),
        }
    }

    pub fn with_parent(mut self, parent_id: AccountId) -> Self {
        self.parent_id = Some(parent_id);
        self
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = description.into();
        self
    }
}
