use crate::EnabledDisabled;
use serde::{Deserialize, Serialize};
use std::fmt;

/// Attributes part of response from ARM DPU for Systems/:id/Bios
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct BiosAttributes {
    #[serde(rename = "Boot Partition Protection")]
    pub boot_partition_protection: Option<bool>,
    pub current_uefi_password: Option<String>,
    pub date_time: Option<String>,
    #[serde(rename = "Disable PCIe")]
    pub disable_pcie: Option<bool>,
    #[serde(rename = "Disable SPMI")]
    pub disable_spmi: Option<bool>,
    #[serde(rename = "Disable TMFF")]
    pub disable_tmff: Option<bool>,
    pub emmc_wipe: Option<bool>,
    #[serde(rename = "Enable 2nd eMMC")]
    pub enable_second_emmc: Option<bool>,
    #[serde(rename = "Enable OP-TEE")]
    pub enable_op_tee: Option<bool>,
    #[serde(rename = "Enable SMMU")]
    pub enable_smmu: Option<bool>,
    #[serde(rename = "Field Mode")]
    pub field_mode: Option<bool>,
    #[serde(rename = "Host Privilege Level")]
    pub host_privilege_level: Option<HostPrivilegeLevel>,
    #[serde(rename = "Internal CPU Model")]
    pub internal_cpu_model: Option<InternalCPUModel>,
    pub reset_efi_vars: Option<bool>,
    #[serde(rename = "SPCR UART")]
    pub spcr_uart: Option<EnabledDisabled>,
    pub uefi_password: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum InternalCPUModel {
    Separated,
    Embedded,
    Unavailable,
}

impl fmt::Display for InternalCPUModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum HostPrivilegeLevel {
    Privileged,
    Restricted,
    Unavailable,
}

impl fmt::Display for HostPrivilegeLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
