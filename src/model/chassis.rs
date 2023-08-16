use serde::{Deserialize, Serialize};

use super::{ODataId, ODataLinks, ResourceStatus};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChassisActions {
    #[serde(rename = "#Chassis.Reset")]
    pub chassis_reset: Option<ChassisAction>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ChassisAction {
    #[serde(rename = "@Redfish.ActionInfo")]
    pub title: Option<String>,
    pub target: Option<String>,        // URL path of the action
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum ChassisType {
    Rack,
    Blade,
    Enclosure,
    StandAlone,
    RackMount,
    Card,
    Cartridge,
    Row,
    Pod,
    Expansion,
    Sidecar,
    Zone,
    Sled,
    Shelf,
    Drawer,
    Module,
    Component,
    IPBasedDrive,
    RackGroup,
    StorageEnclosure,
    ImmersionTank,
    HeatExchanger,
    Other
}

/// http://redfish.dmtf.org/schemas/v1/ChassisCollection.json
/// The ChassisCollection schema contains a collection of chassis instances.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct ChassisCollection {
    #[serde(flatten)]
    pub odata: Option<ODataLinks>,
    #[serde(default)]
    pub members: Vec<ODataId>,
}

/// http://redfish.dmtf.org/schemas/v1/Chassis.v1_23_0.json
/// The Chassis schema contains an inventory of chassis components.  
/// This can include chassis parameters such as chassis type, model, etc.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Chassis {
    #[serde(flatten)]
    pub odata: Option<ODataLinks>,
    pub description: Option<String>,
    pub id: Option<String>,
    #[serde(default)]
    pub links: Vec<ODataId>,
    pub name: Option<String>,
    pub status: Option<ResourceStatus>,
    pub model: Option<String>,
    pub location: Option<ODataId>,
    pub actions: Option<ChassisActions>,
    pub chassis_type: Option<ChassisType>,
    pub environment_metrics: Option<ODataId>,
    pub assembly: Option<ODataId>,
    pub network_adapters: Option<ODataId>,
    pub controls: Option<ODataId>,
    pub pcie_devices: Option<ODataId>,
    pub pcie_slots: Option<ODataId>,
    pub power: Option<ODataId>,
    pub power_subsystem: Option<ODataId>,
    pub sensors: Option<ODataId>,
    pub thermal: Option<ODataId>,
    pub thermal_subsystem: Option<ODataId>,
    pub trusted_components: Option<ODataId>,
    pub manufacturer: Option<String>,
    pub part_number: Option<String>,
    pub serial_number: Option<String>,
}
