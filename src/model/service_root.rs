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
        let v = self.vendor_string().unwrap_or("Unknown".to_string());
        Some(match v.to_lowercase().as_str() {
            "ami" => RedfishVendor::AMI,
            "dell" => RedfishVendor::Dell,
            "hpe" => RedfishVendor::Hpe,
            "lenovo" => RedfishVendor::Lenovo,
            "nvidia" => match self.product.as_deref() {
                Some("P3809") => RedfishVendor::P3809, // could be gh200 compute or nvswitch
                Some("GB200 NVL") => RedfishVendor::NvidiaGBx00,
                _ => RedfishVendor::NvidiaDpu,
            },
            "wiwynn" => RedfishVendor::NvidiaGBx00,
            "supermicro" => RedfishVendor::Supermicro,
            "lite-on technology corp." => RedfishVendor::LiteOnPowerShelf,
            _ => RedfishVendor::Unknown,
        })
    }
}

#[cfg(test)]
mod test {
    use crate::model::service_root::RedfishVendor;

    #[test]
    fn test_supermicro_service_root() {
        let data = include_str!("testdata/supermicro_service_root.json");
        let result: super::ServiceRoot = serde_json::from_str(data).unwrap();
        assert_eq!(result.vendor().unwrap(), RedfishVendor::Supermicro);
    }
}
