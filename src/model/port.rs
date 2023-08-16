use serde::{Deserialize, Serialize};

use super::{ODataId, ODataLinks, LinkStatus};


/// http://redfish.dmtf.org/schemas/v1/NetworkPortCollection.json
/// The NetworkPortCollection schema contains a collection of network port instances.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct NetworkPortCollection {
    #[serde(flatten)]
    pub odata: Option<ODataLinks>,
    #[serde(default)]
    pub members: Vec<ODataId>,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum LinkNetworkTechnology {
    Ethernet,
    InfiniBand,
    FibreChannel,
}

/// http://redfish.dmtf.org/schemas/v1/NetworkPort.v1_4_1.json
/// The NetworkPort schema contains an inventory of software components.  
/// This can include Network Device parameters such as current speed, link status, etc.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct NetworkPort {
    #[serde(flatten)]
    pub odata: Option<ODataLinks>,
    pub description: Option<String>,
    pub id: Option<String>,
    pub name: Option<String>,
    pub link_status: Option<LinkStatus>,
    pub link_network_technology: Option<LinkNetworkTechnology>,
    pub current_speed_gbps: Option<i32>,
}