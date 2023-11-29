use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::model::InvalidValueError;

/// "KCS Channel Control"
/// https://www.supermicro.com/manuals/other/redfish-ref-guide-html/Content/general-content/bmc-configuration-examples.htm
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum Privilege {
    Administrator,
    Operator,
    User,
    Callback,
}

impl FromStr for Privilege {
    type Err = InvalidValueError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Administrator" => Ok(Self::Administrator),
            "Operator" => Ok(Self::Operator),
            "User" => Ok(Self::User),
            "Callback" => Ok(Self::Callback),
            x => Err(InvalidValueError(format!("Invalid Privilege value: {x}"))),
        }
    }
}

impl fmt::Display for Privilege {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub struct FixedBootOrder {
    pub boot_mode_selected: BootMode,
    pub fixed_boot_order: Vec<String>,
    #[serde(rename = "UEFINetwork")]
    pub uefi_network: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Copy, Clone, Eq, PartialEq)]
#[allow(clippy::upper_case_acronyms)]
pub enum BootMode {
    Legacy,
    UEFI,
    Dual,
}
