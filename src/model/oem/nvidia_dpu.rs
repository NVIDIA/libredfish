use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::EnabledDisabled;

/// Attributes part of response from ARM DPU for Systems/:id/Bios
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct BiosAttributes {
    #[serde(alias = "Boot Partition Protection", alias = "BootPartitionProtection")]
    pub boot_partition_protection: Option<bool>,
    pub current_uefi_password: Option<String>,
    pub date_time: Option<String>,
    #[serde(alias = "Disable PCIe", alias = "DisablePCIe")]
    pub disable_pcie: Option<bool>,
    #[serde(alias = "Disable SPMI", alias = "DisableSPMI")]
    pub disable_spmi: Option<bool>,
    #[serde(alias = "Disable TMFF", alias = "DisableTMFF")]
    pub disable_tmff: Option<bool>,
    pub emmc_wipe: Option<bool>,
    #[serde(alias = "Enable 2nd eMMC", alias = "Enable2ndeMMC")]
    pub enable_second_emmc: Option<bool>,
    #[serde(alias = "Enable OP-TEE", alias = "EnableOPTEE")]
    pub enable_op_tee: Option<bool>,
    #[serde(alias = "Enable SMMU", alias = "EnableSMMU")]
    pub enable_smmu: Option<bool>,
    #[serde(alias = "Field Mode", alias = "FieldMode")]
    pub field_mode: Option<bool>,
    #[serde(alias = "Host Privilege Level", alias = "HostPrivilegeLevel")]
    pub host_privilege_level: Option<HostPrivilegeLevel>,
    #[serde(alias = "Internal CPU Model", alias = "InternalCPUModel")]
    pub internal_cpu_model: Option<InternalCPUModel>,
    pub reset_efi_vars: Option<bool>,
    #[serde(alias = "SPCR UART", alias = "SPCR_UART")]
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

#[derive(Copy, Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum NicMode {
    #[serde(rename = "DpuMode", alias = "Dpu")]
    Dpu,
    #[serde(rename = "NicMode", alias = "Nic")]
    Nic,
}

impl FromStr for NicMode {
    type Err = ();

    fn from_str(input: &str) -> Result<NicMode, Self::Err> {
        // strip quotes from the string
        let normalized_input = input.replace('"', "");
        if normalized_input == "NicMode" {
            Ok(NicMode::Nic)
        } else if normalized_input == "DpuMode" {
            Ok(NicMode::Dpu)
        } else {
            Err(())
        }
    }
}

impl fmt::Display for NicMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}
