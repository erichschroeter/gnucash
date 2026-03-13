use chrono::{DateTime, Utc};
use flate2::read::GzDecoder;
use gnc_engine::{Account, AccountType, Book, Split, Transaction};
use gnc_guid::GncGUID;
use gnc_numeric::GncNumeric;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::str::FromStr;

pub fn load_gnucash_file<P: AsRef<Path>>(path: P) -> Result<Book, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let mut buf_reader = BufReader::new(file);

    // Peek to see if it's gzipped
    let mut header = [0u8; 2];
    buf_reader.read_exact(&mut header)?;

    let mut xml_content = Vec::new();

    if header == [0x1f, 0x8b] {
        // Gzipped. We need to decode the whole file.
        // Reconstruct the stream with the header we already read.
        let full_stream = std::io::Cursor::new(header).chain(buf_reader);
        let mut decoder = GzDecoder::new(full_stream);
        decoder.read_to_end(&mut xml_content)?;
    } else {
        // Plain XML
        xml_content.extend_from_slice(&header);
        buf_reader.read_to_end(&mut xml_content)?;
    }

    parse_gnucash_xml(&xml_content)
}

pub fn parse_gnucash_xml(xml: &[u8]) -> Result<Book, Box<dyn std::error::Error>> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);

    let mut book = Book::new();
    let mut buf = Vec::new();

    // Account state
    let mut in_account = false;
    let mut current_account_name = String::new();
    let mut current_account_id = GncGUID::null();
    let mut current_account_type = AccountType::ASSET;
    let mut current_parent_id: Option<GncGUID> = None;

    // Transaction state
    let mut in_transaction = false;
    let mut current_txn_id = GncGUID::null();
    let mut current_txn_description = String::new();
    let mut current_txn_date = Utc::now();
    let mut current_txn_splits = Vec::new();

    // Split state
    let mut in_split = false;
    let mut current_split_id = GncGUID::null();
    let mut current_split_account_id = GncGUID::null();
    let mut current_split_value = GncNumeric::zero();
    let mut current_split_quantity = GncNumeric::zero();
    let mut current_split_reconciled = 'n';

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                match e.name().as_ref() {
                    b"gnc:account" => {
                        in_account = true;
                    }
                    b"act:name" if in_account => {
                        current_account_name = reader.read_text(e.name())?.into_owned();
                    }
                    b"act:id" if in_account => {
                        current_account_id = GncGUID::from_str(&reader.read_text(e.name())?)?;
                    }
                    b"act:type" if in_account => {
                        current_account_type = parse_account_type(&reader.read_text(e.name())?);
                    }
                    b"act:parent" if in_account => {
                        current_parent_id = Some(GncGUID::from_str(&reader.read_text(e.name())?)?);
                    }

                    b"gnc:transaction" => {
                        in_transaction = true;
                        current_txn_splits.clear();
                        current_txn_description.clear();
                    }
                    b"trn:id" if in_transaction => {
                        current_txn_id = GncGUID::from_str(&reader.read_text(e.name())?)?;
                    }
                    b"trn:description" if in_transaction => {
                        current_txn_description = reader.read_text(e.name())?.into_owned();
                    }
                    b"ts:date" if in_transaction => {
                        // This matches both trn:date-posted and trn:date-entered
                        // For MVP we just use whatever comes first (posted usually comes first)
                        current_txn_date = parse_date(&reader.read_text(e.name())?)?;
                    }

                    b"trn:split" if in_transaction => {
                        in_split = true;
                        current_split_id = GncGUID::null();
                        current_split_account_id = GncGUID::null();
                        current_split_value = GncNumeric::zero();
                        current_split_quantity = GncNumeric::zero();
                        current_split_reconciled = 'n';
                    }
                    b"split:id" if in_split => {
                        current_split_id = GncGUID::from_str(&reader.read_text(e.name())?)?;
                    }
                    b"split:account" if in_split => {
                        current_split_account_id = GncGUID::from_str(&reader.read_text(e.name())?)?;
                    }
                    b"split:value" if in_split => {
                        current_split_value = GncNumeric::from_str(&reader.read_text(e.name())?)?;
                    }
                    b"split:quantity" if in_split => {
                        current_split_quantity =
                            GncNumeric::from_str(&reader.read_text(e.name())?)?;
                    }
                    b"split:reconciled-state" if in_split => {
                        let s = reader.read_text(e.name())?;
                        current_split_reconciled = s.chars().next().unwrap_or('n');
                    }
                    _ => (),
                }
            }
            Ok(Event::End(ref e)) => match e.name().as_ref() {
                b"gnc:account" => {
                    in_account = false;
                    book.add_account(Account {
                        name: current_account_name.clone(),
                        id: current_account_id,
                        account_type: current_account_type,
                        parent_id: current_parent_id,
                    });
                    current_account_name.clear();
                    current_account_id = GncGUID::null();
                    current_account_type = AccountType::ASSET;
                    current_parent_id = None;
                }
                b"trn:split" => {
                    in_split = false;
                    current_txn_splits.push(Split {
                        id: current_split_id,
                        account_id: current_split_account_id,
                        value: current_split_value,
                        quantity: current_split_quantity,
                        reconciled: current_split_reconciled,
                    });
                }
                b"gnc:transaction" => {
                    in_transaction = false;
                    book.add_transaction(Transaction {
                        id: current_txn_id,
                        date_posted: current_txn_date,
                        description: current_txn_description.clone(),
                        splits: current_txn_splits.clone(),
                    });
                }
                _ => (),
            },
            Ok(Event::Eof) => break,
            Err(e) => return Err(Box::new(e)),
            _ => (),
        }
        buf.clear();
    }

    Ok(book)
}

fn parse_account_type(s: &str) -> AccountType {
    match s {
        "ROOT" => AccountType::ROOT,
        "ASSET" => AccountType::ASSET,
        "BANK" => AccountType::BANK,
        "CASH" => AccountType::CASH,
        "CREDIT" => AccountType::CREDIT,
        "LIABILITY" => AccountType::LIABILITY,
        "STOCK" => AccountType::STOCK,
        "MUTUAL" => AccountType::MUTUAL,
        "CURRENCY" => AccountType::CURRENCY,
        "INCOME" => AccountType::INCOME,
        "EXPENSE" => AccountType::EXPENSE,
        "EQUITY" => AccountType::EQUITY,
        "RECEIVABLE" => AccountType::RECEIVABLE,
        "PAYABLE" => AccountType::PAYABLE,
        "TRADING" => AccountType::TRADING,
        _ => AccountType::ASSET, // Fallback
    }
}

fn parse_date(s: &str) -> Result<DateTime<Utc>, Box<dyn std::error::Error>> {
    // Format: "2024-10-07 10:59:00 +0000"
    let dt = DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S %z")?;
    Ok(dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_xml() {
        let xml = r#"
            <gnc-v2>
                <gnc:account version="2.0.0">
                    <act:name>Root Account</act:name>
                    <act:id type="guid">00000000000000000000000000000001</act:id>
                    <act:type>ROOT</act:type>
                </gnc:account>
                <gnc:account version="2.0.0">
                    <act:name>Checking</act:name>
                    <act:id type="guid">00000000000000000000000000000002</act:id>
                    <act:type>BANK</act:type>
                    <act:parent type="guid">00000000000000000000000000000001</act:parent>
                </gnc:account>
                <gnc:transaction version="2.0.0">
                  <trn:id type="guid">c4392f63c28041e99fecb072c9ccd227</trn:id>
                  <trn:date-posted>
                    <ts:date>2024-10-07 10:59:00 +0000</ts:date>
                  </trn:date-posted>
                  <trn:description>The Home Depot</trn:description>
                  <trn:splits>
                    <trn:split>
                      <split:id type="guid">ce6d6cbf4db8455c94259df5823a245e</split:id>
                      <split:value>10129/100</split:value>
                      <split:quantity>10129/100</split:quantity>
                      <split:account type="guid">00000000000000000000000000000002</split:account>
                    </trn:split>
                  </trn:splits>
                </gnc:transaction>
            </gnc-v2>
        "#;
        let book = parse_gnucash_xml(xml.as_bytes()).unwrap();
        assert_eq!(book.accounts.len(), 2);
        assert_eq!(book.transactions.len(), 1);

        let txn = book.list_transactions()[0];
        assert_eq!(txn.description, "The Home Depot");
        assert_eq!(txn.splits.len(), 1);
        assert_eq!(txn.splits[0].value.num, 10129);
    }
}
