use serde::{Deserialize, Serialize};

/// https://redfish.dmtf.org/schemas/v1/ComputerSystem.v1_20_1.json
/// The boot information for this resource.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Boot {
    pub automatic_retry_attempts: Option<i32>,
    pub automatic_retry_config: Option<AutomaticRetryConfig>,
    pub boot_next: Option<String>,
    #[serde(default)]
    pub boot_order: Vec<String>,
    pub boot_source_override_enabled: Option<BootSourceOverrideEnabled>,
    pub boot_source_override_target: Option<BootSourceOverrideTarget>,
    pub http_boot_uri: Option<String>,
    pub trusted_module_required_to_boot: Option<TrustedModuleRequiredToBoot>,
    pub uefi_target_boot_source_override: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum AutomaticRetryConfig {
    Disabled,
    RetryAttempts,
    RetryAlways,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum BootSourceOverrideEnabled {
    Once,
    Continuous,
    Disabled,
}

/// http://redfish.dmtf.org/schemas/v1/ComputerSystem.json#/definitions/BootSource
#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum BootSourceOverrideTarget {
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
    Recovery,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum TrustedModuleRequiredToBoot {
    Disabled,
    Required,
}
