use uuid::Uuid;
use std::fmt;
use std::str::FromStr;
use serde::{Serialize, Deserialize};
use thiserror::Error;

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct GncGUID {
    pub data: [u8; 16],
}

impl GncGUID {
    /// Create a new, random GncGUID (corresponds to guid_new_return)
    pub fn new() -> Self {
        let id = Uuid::new_v4();
        GncGUID { data: *id.as_bytes() }
    }

    /// Returns a GncGUID which is guaranteed to never reference any entity.
    pub const fn null() -> Self {
        GncGUID { data: [0u8; 16] }
    }

    /// Returns true if this is a null GUID.
    pub fn is_null(&self) -> bool {
        self.data == [0u8; 16]
    }

    /// Convert to a 32-character hex string (lowercase, no hyphens)
    pub fn to_string(&self) -> String {
        self.data.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

impl Default for GncGUID {
    fn default() -> Self {
        Self::null()
    }
}

impl fmt::Display for GncGUID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_string())
    }
}

#[derive(Error, Debug)]
#[error("invalid GUID string")]
pub struct ParseGuidError;

impl FromStr for GncGUID {
    type Err = ParseGuidError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 32 {
            return Err(ParseGuidError);
        }
        let mut data = [0u8; 16];
        for i in 0..16 {
            data[i] = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|_| ParseGuidError)?;
        }
        Ok(GncGUID { data })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_guid() {
        let g1 = GncGUID::new();
        let g2 = GncGUID::new();
        assert_ne!(g1, g2);
        assert!(!g1.is_null());
    }

    #[test]
    fn test_null_guid() {
        let g = GncGUID::null();
        assert!(g.is_null());
        assert_eq!(g.to_string(), "00000000000000000000000000000000");
    }

    #[test]
    fn test_string_conversion() {
        let g = GncGUID::new();
        let s = g.to_string();
        assert_eq!(s.len(), 32);
        let g_parsed = GncGUID::from_str(&s).unwrap();
        assert_eq!(g, g_parsed);
    }
}
