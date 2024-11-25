/*
 * SPDX-FileCopyrightText: Copyright (c) 2023 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
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

use super::oem::ManagerExtensions;
use crate::model::{ODataId, ODataLinks};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Managers {
    #[serde(flatten)]
    pub odata: ODataLinks,
    pub description: Option<String>,
    pub members: Vec<ODataId>,
    pub name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Manager {
    #[serde(flatten)]
    pub odata: ODataLinks,
    pub actions: Action,
    pub command_shell: Option<Commandshell>,
    pub description: Option<String>,
    pub ethernet_interfaces: ODataId,
    pub firmware_version: String,
    pub graphical_console: Commandshell,
    pub id: String,
    pub log_services: ODataId,
    pub manager_type: String,
    pub model: Option<String>,
    pub name: String,
    pub network_protocol: ODataId,
    pub status: Status,
    #[serde(rename = "UUID")]
    pub uuid: String,
    pub oem: Option<ManagerExtensions>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ActionsManagerReset {
    pub target: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Action {
    #[serde(rename = "#Manager.Reset")]
    pub manager_reset: ActionsManagerReset,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Commandshell {
    pub connect_types_supported: Vec<String>,
    pub enabled: Option<bool>,
    pub max_concurrent_sessions: i64,
    pub service_enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Status {
    pub state: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Availableaction {
    pub action: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, Clone, Copy)]
pub enum ManagerResetType {
    GracefulRestart,
    ForceRestart,
}

impl fmt::Display for ManagerResetType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[cfg(test)]
mod test {
    use crate::model::oem::{dell, hpe};

    #[test]
    fn test_manager_parser_hp() {
        let test_data = include_str!("testdata/manager_hp.json");
        let result: hpe::Manager = serde_json::from_str(test_data).unwrap();
        println!("result: {result:#?}");
    }

    #[test]
    fn test_manager_parser_dell() {
        let test_data2 = include_str!("testdata/manager_dell.json");
        let m: super::Manager = serde_json::from_str(test_data2).unwrap();
        assert!(m.oem.is_some());
        let oem = m.oem.unwrap();
        assert!(oem.dell.is_some());
        assert!(oem.lenovo.is_none());
    }

    #[test]
    fn test_manager_parser_lenovo() {
        let test_data2 = include_str!("testdata/manager_lenovo.json");
        let m: super::Manager = serde_json::from_str(test_data2).unwrap();
        assert!(m.oem.is_some());
        let oem = m.oem.unwrap();
        assert!(oem.dell.is_none());
        assert!(oem.lenovo.is_some());
        if let Some(lenovo) = oem.lenovo {
            assert_eq!(lenovo.recipients_settings.retry_count, 5);
        }
    }

    #[test]
    fn test_manager_parser_dell_attrs() {
        let test_data3 = include_str!("testdata/manager_dell_attrs.json");
        let result3: dell::AttributesResult = serde_json::from_str(test_data3).unwrap();
        println!("result3: {result3:#?}");
    }
}
