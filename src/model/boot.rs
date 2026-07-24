/*
 * SPDX-FileCopyrightText: Copyright (c) 2025 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
 * SPDX-License-Identifier: MIT
 *
 * Permission is hereby granted, free of charge, to any person obtaining a
 * copy of this software and associated documentation files (the "Software"),
 * to deal in the Software without restriction, including without limitation
 * the rights to use, copy, modify, merge, publish, distribute, sublicense,
 * and/or sell copies of the Software, and to permit persons to whom the
 * Software is furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL
 * THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
 * FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
 * DEALINGS IN THE SOFTWARE.
 */
use std::fmt;

use serde::{Deserialize, Serialize};

use super::ODataId;

/// https://redfish.dmtf.org/schemas/v1/ComputerSystem.v1_20_1.json
/// The boot information for this resource.
#[serde_with::skip_serializing_none]
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "PascalCase")]
pub struct Boot {
    pub automatic_retry_attempts: Option<i32>,
    pub automatic_retry_config: Option<AutomaticRetryConfig>,
    pub boot_next: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "<[_]>::is_empty")]
    pub boot_order: Vec<String>,
    pub boot_source_override_enabled: Option<BootSourceOverrideEnabled>,
    pub boot_source_override_target: Option<BootSourceOverrideTarget>,
    pub boot_source_override_mode: Option<BootSourceOverrideMode>,
    pub http_boot_uri: Option<String>,
    pub trusted_module_required_to_boot: Option<TrustedModuleRequiredToBoot>,
    pub uefi_target_boot_source_override: Option<String>,
    pub boot_options: Option<ODataId>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum AutomaticRetryConfig {
    Disabled,
    RetryAttempts,
    RetryAlways,
}

impl std::fmt::Display for AutomaticRetryConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum BootSourceOverrideEnabled {
    Once,
    Continuous,
    Disabled,
    #[serde(other)]
    InvalidValue,
}

impl fmt::Display for BootSourceOverrideEnabled {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// http://redfish.dmtf.org/schemas/v1/ComputerSystem.json#/definitions/BootSource
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
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
    #[serde(other)]
    InvalidValue,
}

impl fmt::Display for BootSourceOverrideTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub enum BootSourceOverrideMode {
    UEFI,
    Legacy,
    #[serde(other)]
    InvalidValue,
}

impl fmt::Display for BootSourceOverrideMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// Settings for a Redfish boot source override, applied via
/// [`Redfish::set_boot_override`](crate::Redfish::set_boot_override).
///
/// `target` and `enabled` are required. `mode` is typically `UEFI` for modern
/// systems and can be left `None` to keep the current mode unchanged. `http_boot_uri`
/// only applies when `target` is `UefiHttp`; if `None`, the firmware obtains the
/// boot URL from DHCP option 67 as specified by the UEFI HTTP Boot specification.
#[derive(Debug, Clone)]
pub struct BootOverride {
    pub target: BootSourceOverrideTarget,
    pub enabled: BootSourceOverrideEnabled,
    pub mode: Option<BootSourceOverrideMode>,
    pub http_boot_uri: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub enum TrustedModuleRequiredToBoot {
    Disabled,
    Required,
}

impl std::fmt::Display for TrustedModuleRequiredToBoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

/// Extracts the BootOption resource Id from a `BootOrder` entry.
///
/// Vera Rubin host BMC firmware reports entries like `Boot0019: Ubuntu` while
/// BootOption resources are addressed at `/BootOptions/Boot0019`.
pub fn boot_order_entry_boot_option_id(entry: &str) -> &str {
    entry.split_once(": ").map(|(id, _)| id).unwrap_or(entry)
}

/// Formats a BootOrder entry the way Vera Rubin firmware exposes it.
pub fn boot_order_entry_with_display_name(id: &str, display_name: &str) -> String {
    format!("{id}: {display_name}")
}

/// Returns true when the BMC uses `Id: DisplayName` strings in `BootOrder`.
pub fn boot_order_uses_display_name_suffix(entries: &[String]) -> bool {
    entries
        .iter()
        .any(|entry| boot_order_entry_boot_option_id(entry) != entry.as_str())
}

/// Prepends a boot option to `BootOrder`, preserving Vera Rubin firmware entry format.
pub fn boot_order_prepend_first(
    boot_order: &[String],
    target_id: &str,
    target_display_name: &str,
) -> Vec<String> {
    let first_entry = if boot_order_uses_display_name_suffix(boot_order) {
        boot_order_entry_with_display_name(target_id, target_display_name)
    } else {
        target_id.to_string()
    };

    let mut ordered = boot_order.to_vec();
    ordered.retain(|entry| boot_order_entry_boot_option_id(entry) != target_id);
    ordered.insert(0, first_entry);
    ordered
}

#[cfg(test)]
mod boot_order_entry_tests {
    use super::{
        boot_order_entry_boot_option_id, boot_order_entry_with_display_name,
        boot_order_prepend_first, boot_order_uses_display_name_suffix,
    };

    #[test]
    fn boot_order_entry_boot_option_id_strips_display_name_suffix() {
        assert_eq!(
            boot_order_entry_boot_option_id("Boot0019: Ubuntu"),
            "Boot0019"
        );
        assert_eq!(boot_order_entry_boot_option_id("Boot0010"), "Boot0010");
    }

    #[test]
    fn boot_order_entry_with_display_name_formats_like_firmware() {
        assert_eq!(
            boot_order_entry_with_display_name("Boot0019", "Ubuntu"),
            "Boot0019: Ubuntu"
        );
    }

    #[test]
    fn boot_order_uses_display_name_suffix_detects_firmware_format() {
        assert!(boot_order_uses_display_name_suffix(&[
            "Boot0019: Ubuntu".to_string(),
            "Boot0010: UEFI HTTPv4 (MAC:F4204D494ECC)".to_string(),
        ]));
        assert!(!boot_order_uses_display_name_suffix(&[
            "Boot0019".to_string(),
            "Boot0010".to_string(),
        ]));
    }

    #[test]
    fn boot_order_prepend_first_uses_id_colon_display_name_when_firmware_requires_it() {
        let current = vec![
            "Boot0010: UEFI HTTPv4 (MAC:AA)".to_string(),
            "Boot0019: Ubuntu".to_string(),
        ];

        let ordered = boot_order_prepend_first(&current, "Boot0019", "UEFI");

        assert_eq!(ordered[0], "Boot0019: UEFI");
        assert_eq!(ordered.len(), 2);
        assert_eq!(ordered[1], "Boot0010: UEFI HTTPv4 (MAC:AA)");
    }

    #[test]
    fn boot_order_prepend_first_uses_bare_id_for_legacy_boot_order_format() {
        let current = vec!["Boot0010".to_string(), "Boot0019".to_string()];

        let ordered = boot_order_prepend_first(&current, "Boot0019", "UEFI");

        assert_eq!(ordered[0], "Boot0019");
        assert_eq!(ordered[1], "Boot0010");
    }

    #[test]
    fn boot_order_prepend_first_replaces_existing_entry_for_same_boot_option_id() {
        let current = vec![
            "Boot0019: Ubuntu".to_string(),
            "Boot0010: UEFI HTTPv4 (MAC:AA)".to_string(),
        ];

        let ordered = boot_order_prepend_first(&current, "Boot0019", "UEFI");

        assert_eq!(ordered[0], "Boot0019: UEFI");
        assert_eq!(ordered.len(), 2);
        assert!(!ordered.contains(&"Boot0019: Ubuntu".to_string()));
    }
}
