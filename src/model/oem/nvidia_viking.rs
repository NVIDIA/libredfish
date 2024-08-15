use std::fmt;

use serde::{Deserialize, Serialize};

use crate::model::EnableDisable;
use crate::EnabledDisabled;

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
    pub acpi_spcr_baud_rate: String,
    pub acpi_spcr_console_redirection_enable: bool,
    pub acpi_spcr_flow_control: String,
    pub acpi_spcr_port: String,
    pub acpi_spcr_terminal_type: String,
    pub baud_rate0: String,
    pub boot_option1: String,
    pub boot_option2: String,
    pub boot_option3: String,
    pub boot_option4: String,
    pub boot_option5: String,
    pub boot_option6: String,
    pub boot_order: String,
    pub console_redirection_enable0: bool,
    pub enable_sgx: EnabledDisabled,
    pub kcs_interface_disable: String,
    pub ipv4_http: EnabledDisabled,
    pub ipv4_pxe: EnabledDisabled,
    pub ipv6_http: EnabledDisabled,
    pub ipv6_pxe: EnabledDisabled,
    pub processor_hyper_threading_disable: EnabledDisabled,
    pub processor_ltsx_enable: EnableDisable,
    pub processor_smx_enable: EnableDisable,
    pub processor_vmx_enable: EnableDisable,
    pub redfish_enable: EnabledDisabled,
    pub secure_boot_mode: String,
    pub secure_boot_support: EnabledDisabled,
    #[serde(rename = "SRIOVEnable")]
    pub sriov_enable: EnableDisable,
    pub terminal_type0: String,
    pub tpm_operation: String,
    pub tpm_support: EnableDisable,
    #[serde(rename = "VTdSupport")]
    pub vtd_support: EnableDisable,
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
pub struct BiosLockdownAttributes {
    pub kcs_interface_disable: String,
    pub redfish_enable: EnabledDisabled,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SetBiosLockdownAttributes {
    pub attributes: BiosLockdownAttributes,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct BiosSerialConsoleAttributes {
    pub acpi_spcr_baud_rate: String,
    pub acpi_spcr_console_redirection_enable: bool,
    pub acpi_spcr_flow_control: String,
    pub acpi_spcr_port: String,
    pub acpi_spcr_terminal_type: String,
    pub baud_rate0: String,
    pub console_redirection_enable0: bool,
    pub terminal_type0: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SetBiosSerialConsoleAttributes {
    pub attributes: BiosSerialConsoleAttributes,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct TpmAttributes {
    pub tpm_operation: String,
    pub tpm_support: EnableDisable,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SetTpmAttributes {
    pub attributes: TpmAttributes,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct VirtAttributes {
    #[serde(rename = "SRIOVEnable")]
    pub sriov_enable: EnableDisable,
    #[serde(rename = "VTdSupport")]
    pub vtd_support: EnableDisable,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SetVirtAttributes {
    pub attributes: VirtAttributes,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SgxAttributes {
    pub enable_sgx: EnabledDisabled,
    pub processor_ltsx_enable: EnableDisable,
    pub processor_smx_enable: EnableDisable,
    pub processor_vmx_enable: EnableDisable,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SetSgxAttributes {
    pub attributes: SgxAttributes,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct UefiHttpAttributes {
    pub ipv4_http: EnabledDisabled,
    pub ipv4_pxe: EnabledDisabled,
    pub ipv6_http: EnabledDisabled,
    pub ipv6_pxe: EnabledDisabled,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SetUefiHttpAttributes {
    pub attributes: UefiHttpAttributes,
}
