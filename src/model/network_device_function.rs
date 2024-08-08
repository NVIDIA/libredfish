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
use serde::{Deserialize, Serialize};

use super::{ODataId, ODataLinks};

/// http://redfish.dmtf.org/schemas/v1/NetworkDeviceFunction.v1_9_0.json
/// The NetworkDeviceFunction schema contains an inventory of software components.
/// This can include Network Device parameters such as MAC address, MTU size
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct NetworkDeviceFunction {
    #[serde(flatten)]
    pub odata: Option<ODataLinks>,
    pub description: Option<String>,
    pub id: Option<String>,
    pub ethernet: Option<Ethernet>,
    pub name: Option<String>,
    pub net_dev_func_capabilities: Option<Vec<String>>,
    pub net_dev_func_type: Option<String>,
    pub links: Option<NetworkDeviceFunctionLinks>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct NetworkDeviceFunctionLinks {
    #[serde(default, rename = "PCIeFunction")]
    pub pcie_function: Option<ODataId>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Ethernet {
    #[serde(flatten)]
    pub ethernet_interfaces: Option<ODataId>,
    #[serde(rename = "MACAddress")]
    pub mac_address: Option<String>,
    #[serde(rename = "MTUSize")]
    pub mtu_size: Option<i32>,
}
