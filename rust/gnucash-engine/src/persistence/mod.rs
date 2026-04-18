pub mod xml;

use crate::domain::types::EngineError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PersistenceError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("XML error: {0}")]
    Xml(#[from] quick_xml::DeError),

    #[error("Engine error: {0}")]
    Engine(#[from] EngineError),

    #[error("Unsupported format")]
    UnsupportedFormat,
}
