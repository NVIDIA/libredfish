use serde::{Deserialize, Serialize};

use super::ODataLinks;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct LogEntry {
    #[serde(flatten)]
    pub odata: ODataLinks,
    pub created: String,
    pub description: Option<String>,
    pub entry_code: Option<String>,
    pub entry_type: String,
    pub id: String,
    pub message: String, // this is the actionable string
    pub name: String,
    pub sensor_number: Option<i64>,
    pub sensor_type: Option<String>,
    pub severity: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct LogEntryCollection {
    #[serde(flatten)]
    pub odata: ODataLinks,
    pub name: String,
    pub description: String,
    pub members: Vec<LogEntry>,
}
