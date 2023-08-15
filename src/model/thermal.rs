use crate::model::ODataId;
use serde::{Deserialize, Serialize};

use super::{ODataLinks, ResourceStatus, StatusVec};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct FansOemHp {
    #[serde(flatten)]
    pub fan_type: super::oem::hp::HpType,
    pub location: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct FansOem {
    pub hp: FansOemHp,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Fan {
    pub reading: i64,
    pub reading_units: String,
    pub fan_name: String,
    pub physical_context: String,
    pub sensor_number: Option<i64>,
    pub lower_threshold_critical: Option<i64>,
    pub lower_threshold_fatal: Option<i64>,
    pub status: ResourceStatus,
    pub upper_threshold_critical: Option<i64>,
    pub upper_threshold_fatal: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct TemperaturesOemHp {
    #[serde(flatten)]
    pub temp_type: super::oem::hp::HpType,
    pub location_xmm: i64,
    pub location_ymm: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct TemperaturesOem {
    pub hp: TemperaturesOemHp,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Temperature {
    pub name: String,
    pub sensor_number: Option<i64>,
    pub lower_threshold_critical: Option<i64>,
    pub lower_threshold_fatal: Option<i64>,
    pub physical_context: String,
    pub reading_celsius: Option<i64>,
    pub status: ResourceStatus,
    pub upper_threshold_critical: Option<i64>,
    pub upper_threshold_fatal: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Redundancy {
    pub max_num_supported: Option<i64>,
    pub member_id: String,
    pub min_num_needed: Option<i64>,
    pub mode: String,
    pub name: String,
    pub redundancy_enabled: bool,
    pub status: ResourceStatus,
    pub redundancy_set: Vec<ODataId>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Thermal {
    #[serde(flatten)]
    pub odata: ODataLinks,
    pub id: String,
    pub name: String,
    pub fans: Vec<Fan>,
    pub temperatures: Vec<Temperature>,
    pub redundancy: Option<Vec<Redundancy>>,
}

impl StatusVec for Thermal {
    fn get_vec(&self) -> Vec<ResourceStatus> {
        let mut v = Vec::with_capacity(self.fans.len() + self.temperatures.len());
        for res in &self.fans {
            v.push(res.status)
        }
        for res in &self.temperatures {
            v.push(res.status)
        }
        v
    }
}

#[cfg(test)]
mod test {
    #[test]
    fn test_thermal_parser() {
        // TODO: hpe test data is obsolete, needs to be updated from latest iLO BMC
        // with newer redfish schema
        // let test_data_hpe = include_str!("testdata/thermal-hpe.json");
        // let result_hpe: super::Thermal = serde_json::from_str(test_data_hpe).unwrap();
        // println!("result: {result_hpe:#?}");
        let test_data_dell = include_str!("testdata/thermal-dell.json");
        let result_dell: super::Thermal = serde_json::from_str(test_data_dell).unwrap();
        println!("result: {result_dell:#?}");
        let test_data_lenovo = include_str!("testdata/thermal-lenovo.json");
        let result_lenovo: super::Thermal = serde_json::from_str(test_data_lenovo).unwrap();
        println!("result: {result_lenovo:#?}");
    }
}
