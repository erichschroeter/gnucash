use std::io::{Read, Seek, SeekFrom};
use flate2::read::GzDecoder;
use quick_xml::de::from_str;
use serde::Deserialize;
use super::PersistenceError;
use crate::domain::{Account, AccountId, AccountType, CommodityId, Ledger, DraftTransaction, Split, Money};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
struct GncV2 {
    #[serde(rename = "book")]
    book: Book,
}

#[derive(Debug, Deserialize)]
struct Book {
    #[serde(rename = "account", default)]
    accounts: Vec<RawAccount>,
    #[serde(rename = "transaction", default)]
    transactions: Vec<RawTransaction>,
}

#[derive(Debug, Deserialize)]
struct RawAccount {
    #[serde(rename = "name")]
    name: String,
    #[serde(rename = "type")]
    account_type: String,
    #[serde(rename = "id")]
    id: RawId,
}

#[derive(Debug, Deserialize)]
struct RawTransaction {
    #[serde(rename = "description")]
    description: String,
    #[serde(rename = "id")]
    id: RawId,
    #[serde(rename = "date-posted")]
    date_posted: RawDate,
    #[serde(rename = "splits")]
    splits: RawSplits,
}

#[derive(Debug, Deserialize)]
struct RawSplits {
    #[serde(rename = "split", default)]
    splits: Vec<RawSplit>,
}

#[derive(Debug, Deserialize)]
struct RawSplit {
    #[serde(rename = "id")]
    id: RawId,
    #[serde(rename = "account")]
    account: RawId,
    #[serde(rename = "value")]
    value: String,
}

#[derive(Debug, Deserialize)]
struct RawId {
    #[serde(rename = "$value")]
    value: String,
}

#[derive(Debug, Deserialize)]
struct RawDate {
    #[serde(rename = "date")]
    date: String,
}

pub fn load_from_path(path: &str) -> Result<Ledger, PersistenceError> {
    let mut file = std::fs::File::open(path)?;
    let mut magic = [0u8; 2];
    let is_gz = if file.read_exact(&mut magic).is_ok() {
        magic == [0x1f, 0x8b]
    } else {
        false
    };
    
    file.seek(SeekFrom::Start(0))?;

    let mut buffer = Vec::new();
    if is_gz {
        let mut gz = GzDecoder::new(file);
        gz.read_to_end(&mut buffer)?;
    } else {
        file.read_to_end(&mut buffer)?;
    }

    let xml_str = std::str::from_utf8(&buffer).map_err(|_| PersistenceError::UnsupportedFormat)?;
    
    // Crude but effective: strip common GnuCash XML prefixes to simplify deserialization
    let xml_stripped = xml_str
        .replace("gnc:", "")
        .replace("act:", "")
        .replace("trn:", "")
        .replace("split:", "")
        .replace("ts:", "")
        .replace("book:", "")
        .replace("cd:", "")
        .replace("cmdty:", "");

    let gnc: GncV2 = from_str(&xml_stripped).map_err(|e| {
        log::error!("XML Parse Error: {}", e);
        PersistenceError::UnsupportedFormat
    })?;

    let mut domain_accounts = Vec::new();
    let mut account_id_map = std::collections::HashMap::new();

    for raw in gnc.book.accounts {
        let acc_id = Uuid::parse_str(&raw.id.value).map(AccountId::from).unwrap_or_else(|_| AccountId::new());
        let acc_type = match raw.account_type.as_str() {
            "BANK" => AccountType::Bank,
            "CASH" => AccountType::Cash,
            "CREDIT" => AccountType::Credit,
            "ASSET" => AccountType::Asset,
            "LIABILITY" => AccountType::Liability,
            "INCOME" => AccountType::Income,
            "EXPENSE" => AccountType::Expense,
            "EQUITY" => AccountType::Equity,
            _ => AccountType::Asset,
        };
        let mut acc = Account::new(raw.name.clone(), acc_type, CommodityId::new("USD"));
        acc.id = acc_id;
        domain_accounts.push(acc);
        account_id_map.insert(raw.id.value.clone(), acc_id);
    }

    let mut domain_transactions = Vec::new();
    for raw in gnc.book.transactions {
        let mut draft = DraftTransaction::new(CommodityId::new("USD"));
        draft.description = raw.description;
        
        // Parse date (simplified)
        if let Ok(date) = chrono::DateTime::parse_from_str(&raw.date_posted.date, "%Y-%m-%d %H:%M:%S %z") {
            draft.date = date.with_timezone(&chrono::Utc);
        }

        for s in raw.splits.splits {
            let acc_id = account_id_map.get(&s.account.value).copied().unwrap_or_else(AccountId::new);
            let amount = parse_money(&s.value).unwrap_or_else(|| Money::new(0, 1));
            draft = draft.add_split(Split::new(acc_id, amount));
        }

        if let Ok(tx) = draft.validate() {
            domain_transactions.push(tx);
        }
    }

    Ok(Ledger::new(domain_accounts, domain_transactions))
}

fn parse_money(s: &str) -> Option<Money> {
    let parts: Vec<&str> = s.split('/').collect();
    if parts.len() == 2 {
        let num = parts[0].parse::<i64>().ok()?;
        let den = parts[1].parse::<i64>().ok()?;
        if den == 0 { return None; }
        Some(Money::new(num, den))
    } else {
        s.parse::<i64>().ok().map(|n| Money::new(n, 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gnucash_xml() {
        let xml = r#"<?xml version="1.0" encoding="utf-8" ?>
<gnc-v2 xmlns:gnc="http://www.gnucash.org/XML/gnc" xmlns:act="http://www.gnucash.org/XML/act" xmlns:trn="http://www.gnucash.org/XML/trn" xmlns:ts="http://www.gnucash.org/XML/ts">
<gnc:book version="2.0.0">
  <gnc:account version="2.0.0">
    <act:name>Expenses</act:name>
    <act:id type="guid">ea1d654fedbf4d4c88be7df1b3e8be2a</act:id>
    <act:type>EXPENSE</act:type>
  </gnc:account>
  <gnc:account version="2.0.0">
    <act:name>Bank</act:name>
    <act:id type="guid">0d0c14d1661b46819c18319c21dc2666</act:id>
    <act:type>BANK</act:type>
  </gnc:account>
  <gnc:transaction version="2.0.0">
    <trn:id type="guid">9302636573c7490089c25608405d5420</trn:id>
    <trn:description>Walmart</trn:description>
    <trn:date-posted><ts:date>2026-04-18 00:00:00 +0000</ts:date></trn:date-posted>
    <trn:splits>
      <trn:split>
        <split:id type="guid">b302636573c7490089c25608405d5421</split:id>
        <split:account type="guid">ea1d654fedbf4d4c88be7df1b3e8be2a</split:account>
        <split:value>1234/100</split:value>
      </trn:split>
      <trn:split>
        <split:id type="guid">c302636573c7490089c25608405d5422</split:id>
        <split:account type="guid">0d0c14d1661b46819c18319c21dc2666</split:account>
        <split:value>-1234/100</split:value>
      </trn:split>
    </trn:splits>
  </gnc:transaction>
</gnc:book>
</gnc-v2>"#;

        // Note: Quick-xml from_str with Serde might require stripping prefixes if they aren't handled by rename.
        // For the sake of the test and current implementation, let's assume we might need to adjust tags if it fails.
        // However, we'll try to parse it as is.
        let result: Result<GncV2, _> = from_str(xml);
        assert!(result.is_ok(), "Failed to parse XML: {:?}", result.err());
        let gnc = result.unwrap();
        assert_eq!(gnc.book.accounts.len(), 2);
        assert_eq!(gnc.book.transactions.len(), 1);
        assert_eq!(gnc.book.transactions[0].description, "Walmart");
    }
}
