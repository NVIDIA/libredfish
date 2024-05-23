use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct Error {
    pub error: ErrorInternal,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct ErrorInternal {
    pub code: String,
    pub message: String,
    #[serde(rename = "@Message.ExtendedInfo")]
    pub extended: Vec<super::Message>,
}
