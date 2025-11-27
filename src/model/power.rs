/*
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
use super::{LinkType, ODataId, ODataLinks, ResourceStatus, StatusVec};
use crate::model::sensor::Sensor;
use serde::{Deserialize, Serialize};

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
    pub oem_type: super::oem::hpe::HpType,
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
    pub average_consumed_watts: Option<i64>, // we need to track this metric
    pub interval_in_min: Option<i64>,
    pub max_consumed_watts: Option<i64>,
    pub min_consumed_watts: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct PowerControl {
    pub member_id: String,
    pub power_allocated_watts: Option<f64>,
    pub power_capacity_watts: Option<f64>,
    pub power_consumed_watts: Option<f64>,
    pub power_requested_watts: Option<f64>,
    pub power_limit: Option<PowerLimit>,
    pub power_metrics: Option<PowerMetrics>,
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
    pub power_type: super::oem::hpe::HpType,
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
    pub input_type: Option<String>,
    pub minimum_voltage: Option<i64>,
    pub maximum_voltage: Option<i64>,
    pub output_wattage: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct PowerSupply {
    pub firmware_version: Option<String>,
    // we need to track this metric
    pub last_power_output_watts: Option<f64>, // not in Supermicro or NVIDIA DPU
    // we need to track this metric
    pub line_input_voltage: Option<f64>,
    pub line_input_voltage_type: Option<String>,
    pub efficiency_percent: Option<f64>, // not in Supermicro or NVIDIA DPU
    pub hot_pluggable: Option<bool>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub name: String,
    pub input_ranges: Option<Vec<InputRanges>>, // only present sometimes on Supermicro
    pub power_output_amps: Option<f64>,
    pub power_capacity_watts: Option<f64>, // present but 'null' on Supermicro
    pub power_input_watts: Option<f64>,
    pub power_output_watts: Option<f64>,
    pub power_supply_type: Option<String>,
    pub serial_number: Option<String>,
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
    pub physical_context: Option<String>,
    pub reading_volts: Option<f64>,
    pub lower_threshold_critical: Option<f64>,
    pub upper_threshold_critical: Option<f64>,
    pub status: ResourceStatus,
}

impl From<Sensor> for Voltages {
    fn from(sensor: Sensor) -> Self {
        let physical_context = sensor
            .physical_context
            .map(|physical_context| physical_context.to_string());
        Self {
            name: sensor.name.unwrap_or_default(),
            physical_context,
            reading_volts: sensor.reading,
            lower_threshold_critical: None,
            upper_threshold_critical: None,
            status: sensor.status.unwrap_or_default(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Power {
    #[serde(flatten)]
    pub odata: Option<ODataLinks>,
    pub id: String,
    pub name: String,
    pub power_control: Vec<PowerControl>,
    pub power_supplies: Option<Vec<PowerSupply>>,
    pub voltages: Option<Vec<Voltages>>,
    pub redundancy: Option<Vec<Redundancy>>,
}

impl StatusVec for Power {
    fn get_vec(&self) -> Vec<ResourceStatus> {
        let mut v: Vec<ResourceStatus> = Vec::new();
        if let Some(power_supplies) = &self.power_supplies {
            for res in power_supplies {
                v.push(res.status);
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
        let test_data_lenovo = include_str!("testdata/power-lenovo_health_critical.json");
        let result_lenovo: super::Power = serde_json::from_str(test_data_lenovo).unwrap();
        println!("power-lenovo_health_critical: {result_lenovo:#?}");
    }
}
