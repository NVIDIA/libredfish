use serde::{Deserialize, Serialize};

use super::{LinkType, ODataId, ODataLinks, ResourceStatus, StatusVec};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct OemHpSnmppowerthresholdalert {
    pub duration_in_min: i64,
    pub threshold_watts: i64,
    pub trigger: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OemHp {
    #[serde(flatten)]
    pub oem_type: super::oem::hp::HpType,
    #[serde(rename = "SNMPPowerThresholdAlert")]
    pub snmp_power_threshold_alert: OemHpSnmppowerthresholdalert,
    #[serde(flatten)]
    pub links: LinkType,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Oem {
    pub hp: OemHp,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct PowerLimit {
    pub limit_exception: Option<String>,
    pub limit_in_watts: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct PowerMetrics {
    pub average_consumed_watts: i64, // we need to track this metric
    pub interval_in_min: i64,
    pub max_consumed_watts: i64,
    pub min_consumed_watts: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct PowerControl {
    pub power_allocated_watts: Option<f64>,
    pub power_capacity_watts: Option<f64>,
    pub power_consumed_watts: Option<f64>,
    pub power_requested_watts: Option<f64>,
    pub power_limit: Option<PowerLimit>,
    pub power_metrics: PowerMetrics,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct PowersuppliesOemHpPowersupplystatus {
    pub state: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct PowerSuppliesOemHp {
    #[serde(flatten)]
    pub power_type: super::oem::hp::HpType,
    pub average_power_output_watts: i64,
    pub bay_number: i64,
    pub hotplug_capable: bool,
    pub max_power_output_watts: i64,
    pub mismatched: bool,
    pub power_supply_status: PowersuppliesOemHpPowersupplystatus,
    #[serde(rename = "iPDUCapable")]
    pub i_pdu_capable: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct PowerSuppliesOem {
    pub hp: PowerSuppliesOemHp,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct InputRanges {
    pub input_type: String,
    pub minimum_voltage: i64,
    pub maximum_voltage: i64,
    pub output_wattage: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct PowerSupply {
    pub firmware_version: String,
    // we need to track this metric
    pub last_power_output_watts: Option<f64>, // not in Supermicro or NVIDIA DPU
    // we need to track this metric
    pub line_input_voltage: i64,
    pub line_input_voltage_type: String,
    pub efficiency_percent: Option<f64>, // not in Supermicro or NVIDIA DPU
    pub hot_pluggable: Option<bool>,
    pub model: String,
    pub name: String,
    pub input_ranges: Option<Vec<InputRanges>>, // only present sometimes on Supermicro
    pub power_capacity_watts: Option<i64>,      // present but 'null' on Supermicro
    pub power_input_watts: Option<f64>,
    pub power_output_watts: Option<f64>,
    pub power_supply_type: String,
    pub serial_number: String,
    pub spare_part_number: Option<String>,
    pub part_number: Option<String>, // Supermicro
    pub status: ResourceStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Redundancy {
    pub max_num_supported: i64,
    pub member_id: String,
    pub min_num_needed: i64,
    pub mode: String,
    pub name: String,
    pub redundancy_set: Vec<ODataId>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Voltages {
    pub name: String,
    pub physical_context: String,
    pub reading_volts: Option<f64>,
    pub lower_threshold_critical: Option<f64>,
    pub upper_threshold_critical: Option<f64>,
    pub status: ResourceStatus,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Power {
    #[serde(flatten)]
    pub odata: ODataLinks,
    pub id: String,
    pub name: String,
    pub power_control: Vec<PowerControl>,
    pub power_supplies: Option<Vec<PowerSupply>>,
    pub voltages: Vec<Voltages>,
    pub redundancy: Option<Vec<Redundancy>>,
}

impl StatusVec for Power {
    fn get_vec(&self) -> Vec<ResourceStatus> {
        let mut v: Vec<ResourceStatus> = Vec::new();
        if self.power_supplies.is_some() {
            for res in self.power_supplies.clone().unwrap() {
                v.push(res.status)
            }
        }
        v
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_power_parser() {
        // TODO: hpe test data is obsolete, needs to be updated from latest iLO BMC
        // with newer redfish schema
        // let test_data_hpe = include_str!("testdata/power-hpe.json");
        // let result_hpe: super::Power = serde_json::from_str(test_data_hpe).unwrap();
        // println!("result_hpe: {result_hpe:#?}");
        let test_data_dell = include_str!("testdata/power-dell.json");
        let result_dell: super::Power = serde_json::from_str(test_data_dell).unwrap();
        println!("result_dell: {result_dell:#?}");
        let test_data_lenovo = include_str!("testdata/power-lenovo.json");
        let result_lenovo: super::Power = serde_json::from_str(test_data_lenovo).unwrap();
        println!("result_lenovo: {result_lenovo:#?}");
    }
}
