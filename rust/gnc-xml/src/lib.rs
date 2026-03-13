use flate2::read::GzDecoder;
use gnc_engine::{Account, AccountType, Book};
use gnc_guid::GncGUID;
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
    
    let mut in_account = false;
    let mut current_account_name = String::new();
    let mut current_account_id = GncGUID::null();
    let mut current_account_type = AccountType::ASSET;
    let mut current_parent_id: Option<GncGUID> = None;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                match e.name().as_ref() {
                    b"gnc:account" => {
                        in_account = true;
                        current_account_name.clear();
                        current_account_id = GncGUID::null();
                        current_account_type = AccountType::ASSET;
                        current_parent_id = None;
                    }
                    b"act:name" if in_account => {
                        current_account_name = reader.read_text(e.name())?.into_owned();
                    }
                    b"act:id" if in_account => {
                        let id_str = reader.read_text(e.name())?;
                        current_account_id = GncGUID::from_str(&id_str)?;
                    }
                    b"act:type" if in_account => {
                        let type_str = reader.read_text(e.name())?;
                        current_account_type = parse_account_type(&type_str);
                    }
                    b"act:parent" if in_account => {
                        let parent_str = reader.read_text(e.name())?;
                        current_parent_id = Some(GncGUID::from_str(&parent_str)?);
                    }
                    _ => (),
                }
            }
            Ok(Event::End(ref e)) => {
                if e.name().as_ref() == b"gnc:account" {
                    in_account = false;
                    let account = Account {
                        name: current_account_name.clone(),
                        id: current_account_id,
                        account_type: current_account_type,
                        parent_id: current_parent_id,
                    };
                    book.add_account(account);
                }
            }
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
            </gnc-v2>
        "#;
        let book = parse_gnucash_xml(xml.as_bytes()).unwrap();
        assert_eq!(book.accounts.len(), 2);
        
        let acc = book.list_accounts();
        let checking = acc.iter().find(|a| a.name == "Checking").unwrap();
        assert_eq!(checking.account_type, AccountType::BANK);
    }
}
