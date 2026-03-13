use chrono::{DateTime, Utc};
use flate2::read::GzDecoder;
use gnc_engine::{Account, AccountType, Book, Split, Transaction};
use gnc_guid::GncGUID;
use gnc_kvp::{KvpFrame, KvpValue};
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

    let mut header = [0u8; 2];
    buf_reader.read_exact(&mut header)?;

    let mut xml_content = Vec::new();

    if header == [0x1f, 0x8b] {
        let full_stream = std::io::Cursor::new(header).chain(buf_reader);
        let mut decoder = GzDecoder::new(full_stream);
        decoder.read_to_end(&mut xml_content)?;
    } else {
        xml_content.extend_from_slice(&header);
        buf_reader.read_to_end(&mut xml_content)?;
    }

    let mut book = parse_gnucash_xml(&xml_content)?;
    book.rebuild_index();
    Ok(book)
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
    let mut current_account_kvp = KvpFrame::new();

    // Transaction state
    let mut in_transaction = false;
    let mut current_txn_id = GncGUID::null();
    let mut current_txn_description = String::new();
    let mut current_txn_date = Utc::now();
    let mut current_txn_splits = Vec::new();
    let mut current_txn_kvp = KvpFrame::new();

    // Split state
    let mut in_split = false;
    let mut current_split_id = GncGUID::null();
    let mut current_split_account_id = GncGUID::null();
    let mut current_split_value = GncNumeric::zero();
    let mut current_split_quantity = GncNumeric::zero();
    let mut current_split_reconciled = 'n';
    let mut current_split_kvp = KvpFrame::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
                b"gnc:account" => {
                    in_account = true;
                    current_account_kvp = KvpFrame::new();
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
                b"slot" if in_account && !in_split && !in_transaction => {
                    if let Some((key, value)) = parse_kvp_slot(&mut reader, &mut buf)? {
                        current_account_kvp.insert(key, value);
                    }
                }

                b"gnc:transaction" => {
                    in_transaction = true;
                    current_txn_splits.clear();
                    current_txn_description.clear();
                    current_txn_kvp = KvpFrame::new();
                }
                b"trn:id" if in_transaction => {
                    current_txn_id = GncGUID::from_str(&reader.read_text(e.name())?)?;
                }
                b"trn:description" if in_transaction => {
                    current_txn_description = reader.read_text(e.name())?.into_owned();
                }
                b"ts:date" if in_transaction => {
                    current_txn_date = parse_date(&reader.read_text(e.name())?)?;
                }
                b"slot" if in_transaction && !in_split => {
                    if let Some((key, value)) = parse_kvp_slot(&mut reader, &mut buf)? {
                        current_txn_kvp.insert(key, value);
                    }
                }

                b"trn:split" if in_transaction => {
                    in_split = true;
                    current_split_id = GncGUID::null();
                    current_split_account_id = GncGUID::null();
                    current_split_value = GncNumeric::zero();
                    current_split_quantity = GncNumeric::zero();
                    current_split_reconciled = 'n';
                    current_split_kvp = KvpFrame::new();
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
                    current_split_quantity = GncNumeric::from_str(&reader.read_text(e.name())?)?;
                }
                b"split:reconciled-state" if in_split => {
                    let s = reader.read_text(e.name())?;
                    current_split_reconciled = s.chars().next().unwrap_or('n');
                }
                b"slot" if in_split => {
                    if let Some((key, value)) = parse_kvp_slot(&mut reader, &mut buf)? {
                        current_split_kvp.insert(key, value);
                    }
                }
                b"slot" if !in_account && !in_transaction && !in_split => {
                    if let Some((key, value)) = parse_kvp_slot(&mut reader, &mut buf)? {
                        book.kvp.insert(key, value);
                    }
                }
                _ => (),
            },
            Ok(Event::End(ref e)) => match e.name().as_ref() {
                b"gnc:account" => {
                    in_account = false;
                    book.add_account(Account {
                        name: current_account_name.clone(),
                        id: current_account_id,
                        account_type: current_account_type,
                        parent_id: current_parent_id,
                        kvp: current_account_kvp.clone(),
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
                        kvp: current_split_kvp.clone(),
                    });
                }
                b"gnc:transaction" => {
                    in_transaction = false;
                    book.add_transaction(Transaction {
                        id: current_txn_id,
                        date_posted: current_txn_date,
                        description: current_txn_description.clone(),
                        splits: current_txn_splits.clone(),
                        kvp: current_txn_kvp.clone(),
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

fn parse_kvp_slot<R: std::io::BufRead>(
    reader: &mut Reader<R>,
    buf: &mut Vec<u8>,
) -> Result<Option<(String, KvpValue)>, Box<dyn std::error::Error>> {
    let mut key = String::new();
    let mut value: Option<KvpValue> = None;

    loop {
        match reader.read_event_into(buf)? {
            Event::Start(ref e) => {
                let name = e.name().as_ref().to_vec();
                match name.as_slice() {
                    b"slot:key" => {
                        key = match reader.read_event_into(&mut Vec::new())? {
                            Event::Text(e) => e.unescape()?.into_owned(),
                            _ => String::new(),
                        };
                    }
                    b"slot:value" => {
                        let type_attr = e
                            .attributes()
                            .filter_map(|a| a.ok())
                            .find(|a| a.key.as_ref() == b"type")
                            .map(|a| String::from_utf8_lossy(&a.value).into_owned())
                            .unwrap_or_else(|| "string".to_string());

                        value = match type_attr.as_str() {
                            "string" | "integer" | "numeric" | "guid" => {
                                let content = match reader.read_event_into(&mut Vec::new())? {
                                    Event::Text(e) => e.unescape()?.into_owned(),
                                    _ => String::new(),
                                };
                                match type_attr.as_str() {
                                    "string" => Some(KvpValue::String(content)),
                                    "integer" => {
                                        Some(KvpValue::Int64(content.parse().unwrap_or(0)))
                                    }
                                    "numeric" => Some(KvpValue::Numeric(
                                        GncNumeric::from_str(&content)
                                            .unwrap_or(GncNumeric::zero()),
                                    )),
                                    "guid" => Some(KvpValue::Guid(
                                        GncGUID::from_str(&content).unwrap_or(GncGUID::null()),
                                    )),
                                    _ => unreachable!(),
                                }
                            }
                            "frame" => {
                                let mut frame = KvpFrame::new();
                                loop {
                                    match reader.read_event_into(buf)? {
                                        Event::Start(ref sub_e)
                                            if sub_e.name().as_ref() == b"slot" =>
                                        {
                                            if let Some((k, v)) = parse_kvp_slot(reader, buf)? {
                                                frame.insert(k, v);
                                            }
                                        }
                                        Event::End(ref sub_e)
                                            if sub_e.name().as_ref() == b"slot:value" =>
                                        {
                                            break;
                                        }
                                        _ => (),
                                    }
                                    buf.clear();
                                }
                                Some(KvpValue::Frame(frame))
                            }
                            _ => None,
                        };
                    }
                    _ => (),
                }
            }
            Event::End(ref e) if e.name().as_ref() == b"slot" => {
                break;
            }
            _ => (),
        }
        buf.clear();
    }

    if key.is_empty() || value.is_none() {
        Ok(None)
    } else {
        Ok(Some((key, value.unwrap())))
    }
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
        _ => AccountType::ASSET,
    }
}

fn parse_date(s: &str) -> Result<DateTime<Utc>, Box<dyn std::error::Error>> {
    let dt = DateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S %z")?;
    Ok(dt.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_kvp_types() {
        let xml = r#"
            <gnc-v2>
                <gnc:account version="2.0.0">
                    <act:name>KVP Tester</act:name>
                    <act:id type="guid">00000000000000000000000000000001</act:id>
                    <act:type>BANK</act:type>
                    <slot>
                        <slot:key>my-string</slot:key>
                        <slot:value type="string">hello</slot:value>
                    </slot>
                    <slot>
                        <slot:key>my-int</slot:key>
                        <slot:value type="integer">12345</slot:value>
                    </slot>
                    <slot>
                        <slot:key>my-numeric</slot:key>
                        <slot:value type="numeric">1/3</slot:value>
                    </slot>
                </gnc:account>
            </gnc-v2>
        "#;
        let book = parse_gnucash_xml(xml.as_bytes()).unwrap();
        let acc = book.list_accounts()[0];
        assert_eq!(
            acc.kvp.get("my-string"),
            Some(&KvpValue::String("hello".to_string()))
        );
        assert_eq!(acc.kvp.get("my-int"), Some(&KvpValue::Int64(12345)));
        if let Some(KvpValue::Numeric(n)) = acc.kvp.get("my-numeric") {
            assert_eq!(n.num, 1);
            assert_eq!(n.denom, 3);
        } else {
            panic!("Numeric KVP failed");
        }
    }

    #[test]
    fn test_parse_kvp_recursive() {
        let xml = r#"
            <gnc-v2>
                <gnc:account version="2.0.0">
                    <act:name>Nested Tester</act:name>
                    <act:id type="guid">00000000000000000000000000000002</act:id>
                    <act:type>BANK</act:type>
                    <slot>
                        <slot:key>options</slot:key>
                        <slot:value type="frame">
                            <slot>
                                <slot:key>display</slot:key>
                                <slot:value type="frame">
                                    <slot>
                                        <slot:key>color</slot:key>
                                        <slot:value type="string">red</slot:value>
                                    </slot>
                                </slot:value>
                            </slot>
                        </slot:value>
                    </slot>
                </gnc:account>
            </gnc-v2>
        "#;
        let book = parse_gnucash_xml(xml.as_bytes()).unwrap();
        let acc = book.list_accounts()[0];
        let color = acc.kvp.get_path(&["options", "display", "color"]);
        assert_eq!(color, Some(&KvpValue::String("red".to_string())));
    }

    #[test]
    fn test_parse_transaction_kvp() {
        let xml = r#"
            <gnc-v2>
                <gnc:transaction version="2.0.0">
                  <trn:id type="guid">c4392f63c28041e99fecb072c9ccd227</trn:id>
                  <trn:date-posted><ts:date>2024-10-07 10:59:00 +0000</ts:date></trn:date-posted>
                  <trn:description>Txn with KVP</trn:description>
                  <slot>
                    <slot:key>txn-note</slot:key>
                    <slot:value type="string">important</slot:value>
                  </slot>
                  <trn:splits>
                    <trn:split>
                      <split:id type="guid">ce6d6cbf4db8455c94259df5823a245e</split:id>
                      <split:value>1/1</split:value>
                      <split:quantity>1/1</split:quantity>
                      <split:account type="guid">00000000000000000000000000000003</split:account>
                      <slot>
                        <slot:key>split-meta</slot:key>
                        <slot:value type="integer">1</slot:value>
                      </slot>
                    </trn:split>
                  </trn:splits>
                </gnc:transaction>
            </gnc-v2>
        "#;
        let book = parse_gnucash_xml(xml.as_bytes()).unwrap();
        let txn = book.list_transactions()[0];
        assert_eq!(
            txn.kvp.get("txn-note"),
            Some(&KvpValue::String("important".to_string()))
        );
        assert_eq!(
            txn.splits[0].kvp.get("split-meta"),
            Some(&KvpValue::Int64(1))
        );
    }
}
