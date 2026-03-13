use chrono::{DateTime, NaiveDate, Utc};
use gnc_guid::GncGUID;
use gnc_numeric::GncNumeric;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum KvpValue {
    Int64(i64),
    Double(f64),
    Numeric(GncNumeric),
    String(String),
    Guid(GncGUID),
    Time64(DateTime<Utc>),
    List(Vec<KvpValue>),
    Frame(KvpFrame),
    Date(NaiveDate),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KvpFrame {
    pub slots: HashMap<String, KvpValue>,
}

#[derive(Error, Debug)]
pub enum KvpError {
    #[error("path not found: {0}")]
    PathNotFound(String),
    #[error("not a frame at path: {0}")]
    NotAFrame(String),
}

impl KvpFrame {
    pub fn new() -> Self {
        Self {
            slots: HashMap::new(),
        }
    }

    pub fn insert(&mut self, key: String, value: KvpValue) -> Option<KvpValue> {
        self.slots.insert(key, value)
    }

    pub fn get(&self, key: &str) -> Option<&KvpValue> {
        self.slots.get(key)
    }

    /// Set a value using a path of keys. Creates missing frames.
    pub fn set_path(&mut self, path: &[&str], value: KvpValue) -> Result<(), KvpError> {
        if path.is_empty() {
            return Ok(());
        }

        if path.len() == 1 {
            self.insert(path[0].to_string(), value);
            return Ok(());
        }

        let key = path[0];
        let remaining = &path[1..];

        let entry = self
            .slots
            .entry(key.to_string())
            .or_insert_with(|| KvpValue::Frame(KvpFrame::new()));

        if let KvpValue::Frame(ref mut subframe) = entry {
            subframe.set_path(remaining, value)
        } else {
            Err(KvpError::NotAFrame(key.to_string()))
        }
    }

    /// Get a value using a path of keys.
    pub fn get_path(&self, path: &[&str]) -> Option<&KvpValue> {
        if path.is_empty() {
            return None;
        }

        let key = path[0];
        let val = self.slots.get(key)?;

        if path.len() == 1 {
            Some(val)
        } else if let KvpValue::Frame(ref subframe) = val {
            subframe.get_path(&path[1..])
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kvp_basic() {
        let mut frame = KvpFrame::new();
        frame.insert("my-int".to_string(), KvpValue::Int64(42));
        frame.insert(
            "my-string".to_string(),
            KvpValue::String("hello".to_string()),
        );

        assert_eq!(frame.get("my-int"), Some(&KvpValue::Int64(42)));
        assert_eq!(
            frame.get("my-string"),
            Some(&KvpValue::String("hello".to_string()))
        );
    }

    #[test]
    fn test_kvp_path() {
        let mut frame = KvpFrame::new();
        frame
            .set_path(&["options", "display", "negative-red"], KvpValue::Int64(1))
            .unwrap();

        let val = frame.get_path(&["options", "display", "negative-red"]);
        assert_eq!(val, Some(&KvpValue::Int64(1)));

        // Check intermediate frames
        if let Some(KvpValue::Frame(sub)) = frame.get("options") {
            assert!(sub.get("display").is_some());
        } else {
            panic!("Missing intermediate frame");
        }
    }

    #[test]
    fn test_kvp_not_a_frame_error() {
        let mut frame = KvpFrame::new();
        frame.insert("key".to_string(), KvpValue::Int64(10));

        let result = frame.set_path(&["key", "subkey"], KvpValue::Int64(20));
        assert!(matches!(result, Err(KvpError::NotAFrame(_))));
    }
}
