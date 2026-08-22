use std::collections::HashMap;

use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::parser) struct PreprocessedHereDocuments {
    pub(in crate::parser) command: String,
    pub(in crate::parser) bodies: HashMap<String, String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub(in crate::parser) enum HereDocumentError {
    #[error("<<: here-document delimited by end-of-file (wanted {0})")]
    DelimitedByEndOfFile(String),
}
