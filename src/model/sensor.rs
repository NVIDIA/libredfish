use std::fmt::Display;
use serde::{Deserialize, Serialize};
use crate::model::{ODataId, ResourceStatus};
use crate::OData;

#[derive(Debug, Clone)]
pub struct GPUSensors {
    pub gpu_id: String,
    pub sensors: Vec<Sensor>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Sensor {
    #[serde(flatten)]
    pub odata: OData,
    pub id: Option<String>,
    pub name: Option<String>,
    pub physical_context: Option<PhysicalContext>,
    pub reading: Option<f64>,
    pub reading_type: Option<ReadingType>,
    pub reading_units: Option<String>,
    pub reading_range_max: Option<f64>,
    pub reading_range_min: Option<f64>,
    pub status: Option<ResourceStatus>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Sensors {
    #[serde(flatten)]
    pub odata: OData,
    pub members: Vec<ODataId>,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Default, Copy, Clone, Eq, PartialEq)]
pub enum PhysicalContext {
    #[default]
    Room,
    Intake,
    Exhaust,
    LiquidInlet,
    LiquidOutlet,
    Front,
    Back,
    Upper,
    Lower,
    CPU,
    CPUSubsystem,
    GPU,
    GPUSubsystem,
    FPGA,
    Accelerator,
    ASIC,
    Backplane,
    SystemBoard,
    PowerSupply,
    PowerSubsystem,
    VoltageRegulator,
    Rectifier,
    StorageDevice,
    NetworkingDevice,
    ComputeBay,
    StorageBay,
    NetworkBay,
    ExpansionBay,
    PowerSupplyBay,
    Memory,
    MemorySubsystem,
    Chassis,
    Fan,
    CoolingSubsystem,
    Motor,
    Transformer,
    ACUtilityInput,
    ACStaticBypassInput,
    ACMaintenanceBypassInput,
    DCBus,
    ACOutput,
    ACInput,
    TrustedModule,
    Board,
    Transceiver,
    Battery,
    Pump,
}

#[derive(Debug, Serialize, Deserialize, Default, Copy, Clone, Eq, PartialEq)]
pub enum ReadingType {
    #[default]
    Temperature,
    Humidity,
    Power,
    EnergykWh,
    EnergyJoules,
    EnergyWh,
    ChargeAh,
    Voltage,
    Current,
    Frequency,
    Pressure,
    PressurekPa,
    PressurePa,
    LiquidLevel,
    Rotational,
    AirFlow,
    AirFlowCMM,
    LiquidFlow,
    LiquidFlowLPM,
    Barometric,
    Altitude,
    Percent,
    AbsoluteHumidity,
    Heat,
}

impl Display for PhysicalContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self, f)
    }
}