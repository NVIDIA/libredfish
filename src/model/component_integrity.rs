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

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::model::ODataId;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct ComponentIntegrities {
    pub members: Vec<ComponentIntegrity>,
    pub name: String,
    #[serde(rename = "Members@odata.count")]
    pub count: i16,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct ComponentIntegrity {
    pub component_integrity_enabled: bool,
    pub component_integrity_type: String,
    pub component_integrity_type_version: String,
    pub id: String,
    pub name: String,
    pub target_component_uri: Option<String>,
    pub spdm: Option<SPDMData>,
    pub actions: Option<SPDMActions>,
    pub links: Option<ComponentsProtectedLinks>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct ComponentsProtectedLinks {
    pub components_protected: Vec<ODataId>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SPDMData {
    pub identity_authentication: ResponderAuthentication,
    pub requester: ODataId,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct ResponderAuthentication {
    pub component_certificate: ODataId,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SPDMActions {
    #[serde(rename = "#ComponentIntegrity.SPDMGetSignedMeasurements")]
    pub get_signed_measurements: Option<SPDMGetSignedMeasurements>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SPDMGetSignedMeasurements {
    #[serde(rename = "@Redfish.ActionInfo")]
    pub action_info: String,
    pub target: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct CaCertificate {
    pub certificate_string: String,
    pub certificate_type: String,
    pub certificate_usage_types: Vec<String>,
    pub id: String,
    pub name: String,
    #[serde(rename = "SPDM")]
    pub spdm: SlotInfo,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SlotInfo {
    pub slot_id: u16,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Evidence {
    pub hashing_algorithm: String,
    pub signed_measurements: String,
    pub signing_algorithm: String,
    pub version: String,
}

pub struct RegexToFirmwareIdOptions {
    pub pattern: Regex,
    pub id_prefix: &'static str,
    // if suffix is needed, add another member `id_suffix` here.
}

#[cfg(test)]
mod tests {
    use crate::model::component_integrity::CaCertificate;

    #[test]
    fn test_ca_certificate_serialization_deserialization() {
        let ca_certificate = r#"{
    "@odata.id": "/redfish/v1/Chassis/HGX_IRoT_GPU_0/Certificates/CertChain",
    "@odata.type": " #Certificate.v1_5_0.Certificate",
    "CertificateString": "-----BEGIN CERTIFICATE-----\nMIIDdDCCAvqgAwZ0UBCk+3B6JuSijznMdCaX+lwxJ0Eq7V\nSFpkQATVveySG/Qo8NreDDAfu5dAcVBr\n-----END CERTIFICATE-----\n",
    "CertificateType": "PEMchain",
    "CertificateUsageTypes": [
        "Device"
    ],
    "Id": "CertChain",
    "Name": "HGX_IRoT_GPU_0 Certificate Chain",
    "SPDM": {
        "SlotId": 0
    }
}"#;

        // test Deserialization
        let parsed_certificate: CaCertificate = serde_json::from_str(ca_certificate).unwrap();
        assert_eq!(parsed_certificate.id, "CertChain");
        assert_eq!(parsed_certificate.spdm.slot_id, 0);
        assert_eq!(parsed_certificate.certificate_usage_types.len(), 1);

        // test simple serialization
        serde_json::to_string(&parsed_certificate).unwrap();
    }
}
