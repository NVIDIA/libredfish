use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct ManagerAccount {
    pub id: String,
    #[serde(rename = "UserName")]
    pub username: String,
    pub name: String,
    pub description: String,
    pub role_id: String,
    pub enabled: bool,
    pub locked: bool,
}

impl Ord for ManagerAccount {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}

impl PartialOrd for ManagerAccount {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ManagerAccount {
    fn eq(&self, other: &ManagerAccount) -> bool {
        self.id == other.id
    }
}
