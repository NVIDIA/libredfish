use serde::{Deserialize, Serialize};

use super::{ODataId, ODataLinks};

/// http://redfish.dmtf.org/schemas/v1/NetworkDeviceFunctionCollection.json
/// The NetworkDeviceFunctionCollection schema contains a collection of network device function instances.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct NetworkDeviceFunctionCollection {
    #[serde(flatten)]
    pub odata: Option<ODataLinks>,
    #[serde(default)]
    pub members: Vec<ODataId>,
}

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
    pub links: Vec<ODataId>,
    pub ethernet: Option<Ethernet>,
    pub name: Option<String>,
    pub net_dev_func_capabilities: Vec<String>,
    pub net_dev_func_type: Option<String>,
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
