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
use std::collections::HashMap;
use std::fmt;

use model::{OData, ODataId};
use serde::{Deserialize, Serialize};

use crate::model;

/// https://redfish.dmtf.org/schemas/v1/ServiceRoot.v1_16_0.json
/// This type shall contain information about deep operations that the service supports.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct ServiceRoot {
    #[serde(flatten)]
    pub odata: OData,
    pub product: Option<String>,
    pub redfish_version: String,
    pub vendor: Option<String>,
    /// Vendor forced by the override file; set in `get_service_root` and returned by
    /// `vendor()` ahead of auto-detection. Not part of the Redfish schema.
    #[serde(skip)]
    pub override_vendor: Option<RedfishVendor>,
    #[serde(rename = "UUID")]
    pub uuid: Option<String>,
    pub oem: Option<HashMap<String, serde_json::Value>>,
    pub update_service: Option<HashMap<String, serde_json::Value>>,
    pub account_service: Option<ODataId>,
    pub certificate_service: Option<ODataId>,
    pub chassis: Option<ODataId>,
    pub component_integrity: Option<ODataId>,
    pub event_service: Option<ODataId>,
    pub license_service: Option<ODataId>,
    pub fabrics: Option<ODataId>,
    pub managers: Option<ODataId>,
    pub session_service: Option<ODataId>,
    pub systems: Option<ODataId>,
    pub tasks: Option<ODataId>,
    pub telemetry_service: Option<ODataId>,
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Clone, Copy, Debug, PartialEq, Hash, Eq, Serialize, Deserialize)]
pub enum RedfishVendor {
    Lenovo,
    LenovoAMI,
    Dell,
    NvidiaDpu,
    Supermicro,
    AMI, // Viking DGX H100
    Hpe,
    NvidiaGH200,    // grace-hopper 200
    NvidiaGBx00, // all Grace-Blackwell combinations 200, .. since openbmc fw and redfish schema are the same
    NvidiaGBSwitch, // GB NVLink switch
    P3809, // dummy for P3809, needs to be set to NvidiaGH200 or NvidiaGBSwitch based on chassis
    LiteOnPowerShelf,
    DeltaPowerShelf,
    Rune,
    Unknown,
}

impl fmt::Display for RedfishVendor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl ServiceRoot {
    /// Vendor provided by Redfish ServiceRoot
    pub fn vendor_string(&self) -> Option<String> {
        // If there is no "Vendor" key in ServiceRoot, look for an "Oem" entry. It will have a
        // single key which is the vendor name.
        self.vendor.as_ref().cloned().or_else(|| match &self.oem {
            Some(oem) => oem.keys().next().cloned(),
            None => None,
        })
    }

    pub fn vendor(&self) -> Option<RedfishVendor> {
        // A forced override vendor wins over auto-detection.
        if self.override_vendor.is_some() {
            return self.override_vendor;
        }
        let v = self.vendor_string().unwrap_or("Unknown".to_string());
        Some(match v.to_lowercase().as_str() {
            "ami" => RedfishVendor::AMI,
            "dell" => RedfishVendor::Dell,
            "hpe" => RedfishVendor::Hpe,
            "lenovo" => {
                if self.has_ami_bmc() {
                    RedfishVendor::LenovoAMI
                } else {
                    RedfishVendor::Lenovo
                }
            }
            "nvidia" => match self.product.as_deref() {
                Some("P3809") => RedfishVendor::P3809, // could be gh200 compute or nvswitch
                Some("GB200 NVL") | Some("GB BMC") => RedfishVendor::NvidiaGBx00,
                _ => RedfishVendor::NvidiaDpu,
            },
            "wiwynn" => RedfishVendor::NvidiaGBx00,
            "supermicro" => RedfishVendor::Supermicro,
            "lite-on technology corp." => RedfishVendor::LiteOnPowerShelf,
            "delta" => RedfishVendor::DeltaPowerShelf,
            "rune" => RedfishVendor::Rune,
            _ => RedfishVendor::Unknown,
        })
    }

    /// Check if this system has an AMI-based BMC (indicated by "Ami" key in OEM field)
    pub fn has_ami_bmc(&self) -> bool {
        self.oem
            .as_ref()
            .map(|oem| oem.keys().any(|k| k.to_lowercase() == "ami"))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod test {
    use crate::model::service_root::{RedfishVendor, ServiceRoot};

    #[test]
    fn test_supermicro_service_root() {
        let data = include_str!("testdata/supermicro_service_root.json");
        let result: super::ServiceRoot = serde_json::from_str(data).unwrap();
        assert_eq!(result.vendor().unwrap(), RedfishVendor::Supermicro);
    }

    #[test]
    fn test_nvidia_gb_bmc_service_root() {
        let result = ServiceRoot {
            vendor: Some("NVIDIA".to_string()),
            product: Some("GB BMC".to_string()),
            ..Default::default()
        };
        assert_eq!(result.vendor().unwrap(), RedfishVendor::NvidiaGBx00);
    }

    #[test]
    fn test_nvidia_bluefield_service_root() {
        let result = ServiceRoot {
            vendor: Some("NVIDIA".to_string()),
            product: Some("BlueField-3".to_string()),
            ..Default::default()
        };
        assert_eq!(result.vendor().unwrap(), RedfishVendor::NvidiaDpu);
    }

    #[test]
    fn override_vendor_wins_over_detection() {
        // A pinned override vendor (stamped by get_service_root) takes precedence
        // over whatever the BMC reports, so detection/auto-detect honor it.
        let result = ServiceRoot {
            vendor: Some("dell".to_string()),
            override_vendor: Some(RedfishVendor::Rune),
            ..Default::default()
        };
        assert_eq!(result.vendor().unwrap(), RedfishVendor::Rune);
    }
}
