use std::fmt;

use serde::{Deserialize, Serialize};

use crate::model::EnableDisable;
use crate::EnabledDisabled;

pub const DEFAULT_ACPI_SPCR_BAUD_RATE: &str = "115200";
pub const DEFAULT_BAUD_RATE0: &str = DEFAULT_ACPI_SPCR_BAUD_RATE;
pub const DEFAULT_ACPI_SPCR_CONSOLE_REDIRECTION_ENABLE: bool = true;
pub const DEFAULT_ACPI_SPCR_FLOW_CONTROL: &str = "None";
pub const DEFAULT_ACPI_SPCR_PORT: &str = "COM0";
pub const DEFAULT_ACPI_SPCR_TERMINAL_TYPE: &str = "VT-UTF8";
pub const DEFAULT_CONSOLE_REDIRECTION_ENABLE0: bool = true;
pub const DEFAULT_TERMINAL_TYPE0: &str = "ANSI";
pub const DEFAULT_TPM_SUPPORT: EnableDisable = EnableDisable::Enable;
pub const DEFAULT_TPM_OPERATION: &str = "TPM Clear";
pub const DEFAULT_SRIOV_ENABLE: EnableDisable = EnableDisable::Enable;
pub const DEFAULT_VTD_SUPPORT: EnableDisable = EnableDisable::Enable;
pub const DEFAULT_IPV4_HTTP: EnabledDisabled = EnabledDisabled::Enabled;
pub const DEFAULT_IPV4_PXE: EnabledDisabled = EnabledDisabled::Disabled;
pub const DEFAULT_IPV6_HTTP: EnabledDisabled = EnabledDisabled::Enabled;
pub const DEFAULT_IPV6_PXE: EnabledDisabled = EnabledDisabled::Disabled;
pub const DEFAULT_REDFISH_ENABLE: EnabledDisabled = EnabledDisabled::Enabled;

pub const DEFAULT_KCS_INTERFACE_DISABLE: &str = KCS_INTERFACE_DISABLE_DENY_ALL;
pub const KCS_INTERFACE_DISABLE_DENY_ALL: &str = "Deny All";
pub const KCS_INTERFACE_DISABLE_ALLOW_ALL: &str = "Allow All";
pub const RECOMMENDED_BIOS_VERSION: &str = "01.05.03";
pub const MINIMUM_BIOS_VERSION: &str = "1.01.03";
pub const RECOMMENDED_BMC_FW_VERSION: &str = "24.09.17";
pub const MINIMUM_BMC_FW_VERSION: &str = "23.11.09";

#[derive(Debug, Deserialize, Serialize, Copy, Clone, Eq, PartialEq)]
#[allow(clippy::upper_case_acronyms)]
pub enum BootDevices {
    None,
    Pxe,
    Floppy,
    Cd,
    Usb,
    Hdd,
    BiosSetup,
    Utilities,
    Diags,
    UefiShell,
    UefiTarget,
    SDCard,
    UefiHttp,
    RemoteDrive,
    UefiBootNext,
}

impl fmt::Display for BootDevices {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct BiosAttributes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acpi_spcr_baud_rate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acpi_spcr_console_redirection_enable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acpi_spcr_flow_control: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acpi_spcr_port: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub acpi_spcr_terminal_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baud_rate0: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub console_redirection_enable0: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_type0: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "SRIOVEnable")]
    pub sriov_enable: Option<EnableDisable>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "VTdSupport")]
    pub vtd_support: Option<EnableDisable>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv4_http: Option<EnabledDisabled>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv4_pxe: Option<EnabledDisabled>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv6_http: Option<EnabledDisabled>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ipv6_pxe: Option<EnabledDisabled>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tpm_operation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tpm_support: Option<EnableDisable>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kcs_interface_disable: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redfish_enable: Option<EnabledDisabled>,
}

impl Default for BiosAttributes {
    fn default() -> Self {
        BiosAttributes {
            acpi_spcr_baud_rate: None,
            acpi_spcr_console_redirection_enable: None,
            acpi_spcr_flow_control: None,
            acpi_spcr_port: None,
            acpi_spcr_terminal_type: None,
            baud_rate0: None,
            console_redirection_enable0: None,
            terminal_type0: None,
            sriov_enable: None,
            vtd_support: None,
            ipv4_http: None,
            ipv4_pxe: None,
            ipv6_http: None,
            ipv6_pxe: None,
            tpm_operation: None,
            tpm_support: None,
            kcs_interface_disable: None,
            redfish_enable: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Bios {
    #[serde(rename = "@odata.context")]
    pub odata_context: String,
    pub attributes: BiosAttributes,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SetBiosAttributes {
    pub attributes: BiosAttributes,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct BmcSerialConsoleAttributes {
    pub bit_rate: String,
    pub data_bits: String,
    pub flow_control: String,
    pub interface_enabled: bool,
    pub parity: String,
    pub stop_bits: String,
}
