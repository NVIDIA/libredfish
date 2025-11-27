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
use crate::{Chassis, EnabledDisabled, REDFISH_ENDPOINT};
use regex::Regex;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::sync::OnceLock;
use std::{collections::HashMap, path::Path, time::Duration};
use tokio::fs::File;

use crate::model::account_service::ManagerAccount;
use crate::model::certificate::Certificate;
use crate::model::component_integrity::{ComponentIntegrities, RegexToFirmwareIdOptions};
use crate::model::oem::nvidia_dpu::NicMode;
use crate::model::sensor::{GPUSensors, Sensor, Sensors};
use crate::model::service_root::RedfishVendor;
use crate::model::storage::DriveCollection;
use crate::model::task::Task;
use crate::model::thermal::Fan;
use crate::model::update_service::{ComponentType, TransferProtocolType, UpdateService};
use crate::{
    model::{
        boot::{BootSourceOverrideEnabled, BootSourceOverrideTarget},
        chassis::{Assembly, NetworkAdapter},
        power::{Power, PowerSupply, Voltages},
        sel::{LogEntry, LogEntryCollection},
        service_root::ServiceRoot,
        storage::Drives,
        thermal::{LeakDetector, Temperature, TemperaturesOemNvidia, Thermal},
        BootOption, ComputerSystem, Manager,
    },
    standard::RedfishStandard,
    BiosProfileType, Collection, NetworkDeviceFunction, ODataId, Redfish, RedfishError, Resource,
};
use crate::{JobState, MachineSetupDiff, MachineSetupStatus, RoleId};

const UEFI_PASSWORD_NAME: &str = "AdminPassword";

pub struct Bmc {
    s: RedfishStandard,
}

impl Bmc {
    pub fn new(s: RedfishStandard) -> Result<Bmc, RedfishError> {
        Ok(Bmc { s })
    }
}

#[derive(Copy, Clone)]
pub enum BootOptionName {
    Http,
    Pxe,
    Hdd,
}

impl BootOptionName {
    fn to_string(self) -> &'static str {
        match self {
            BootOptionName::Http => "UEFI HTTPv4",
            BootOptionName::Pxe => "UEFI PXEv4",
            BootOptionName::Hdd => "HD(",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
enum BootOptionMatchField {
    DisplayName,
    UefiDevicePath,
}

impl BootOptionMatchField {
    #[allow(dead_code)]
    fn to_string(self) -> &'static str {
        match self {
            BootOptionMatchField::DisplayName => "Display Name",
            BootOptionMatchField::UefiDevicePath => "Uefi Device Path",
        }
    }
}

impl Display for BootOptionMatchField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self, f)
    }
}

// Supported component to firmware mapping.
// GPU, Source: HGX_IRoT_GPU_X Target: HGX_FW_GPU_X
fn get_component_integrity_id_to_firmware_inventory_id_options(
) -> Result<&'static Vec<RegexToFirmwareIdOptions>, RedfishError> {
    static RE: OnceLock<Result<Vec<RegexToFirmwareIdOptions>, String>> = OnceLock::new();
    RE.get_or_init(|| {
        Ok(vec![RegexToFirmwareIdOptions {
            id_prefix: "HGX_FW_",
            // Assuming our static pattern is good, this is probably
            // safe, but still check for an error instead of unwrapping.
            pattern: Regex::new(r"HGX_IRoT_(GPU_\d+)").map_err(|e| e.to_string())?,
        }])
    })
    .as_ref()
    .map_err(|e| RedfishError::GenericError {
        error: format!("Failed to compile regex: {}", e),
    })
}

#[async_trait::async_trait]
impl Redfish for Bmc {
    async fn create_user(
        &self,
        username: &str,
        password: &str,
        role_id: RoleId,
    ) -> Result<(), RedfishError> {
        self.s.create_user(username, password, role_id).await
    }

    async fn delete_user(&self, username: &str) -> Result<(), RedfishError> {
        self.s.delete_user(username).await
    }

    async fn change_username(&self, old_name: &str, new_name: &str) -> Result<(), RedfishError> {
        self.s.change_username(old_name, new_name).await
    }

    async fn change_password(&self, user: &str, new: &str) -> Result<(), RedfishError> {
        self.s.change_password(user, new).await
    }

    async fn change_password_by_id(
        &self,
        account_id: &str,
        new_pass: &str,
    ) -> Result<(), RedfishError> {
        self.s.change_password_by_id(account_id, new_pass).await
    }

    async fn get_accounts(&self) -> Result<Vec<ManagerAccount>, RedfishError> {
        self.s.get_accounts().await
    }

    async fn get_firmware(
        &self,
        id: &str,
    ) -> Result<crate::model::software_inventory::SoftwareInventory, RedfishError> {
        let mut inv = self.s.get_firmware(id).await?;
        // BMC firmware gets prepended with "GB200Nvl-", (L, not 1!) so trim that off when we see it.
        inv.version = inv.version.map(|x| {
            x.strip_prefix("GB200Nvl-")
                .unwrap_or(x.as_str())
                .to_string()
        });
        Ok(inv)
    }

    async fn get_software_inventories(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_software_inventories().await
    }

    async fn get_tasks(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_tasks().await
    }

    async fn get_task(&self, id: &str) -> Result<crate::model::task::Task, RedfishError> {
        self.s.get_task(id).await
    }

    async fn get_power_state(&self) -> Result<crate::PowerState, RedfishError> {
        self.s.get_power_state().await
    }

    async fn get_power_metrics(&self) -> Result<crate::Power, RedfishError> {
        let mut voltages = Vec::new();
        let mut power_supplies = Vec::new();
        // gb200 bianca has empty PowerSupplies on several chassis items
        // for now assemble power supply details from PDB_0 chassis entries
        let mut url = "Chassis/PDB_0".to_string();
        let (_status_code, pdb): (StatusCode, PowerSupply) = self.s.client.get(&url).await?;
        let mut hsc0 = pdb.clone();
        let mut hsc1 = pdb.clone();
        // voltage sensors are on several chassis items under sensors
        let chassis_all = self.s.get_chassis_all().await?;
        for chassis_id in chassis_all {
            url = format!("Chassis/{}", chassis_id);
            let (_status_code, chassis): (StatusCode, Chassis) = self.s.client.get(&url).await?;
            if chassis.sensors.is_none() {
                continue;
            }
            // walk through all Chassis/*/Sensors/ for voltage and PDB_0 for power supply details
            url = format!("Chassis/{}/Sensors", chassis_id);
            let (_status_code, sensors): (StatusCode, Sensors) = self.s.client.get(&url).await?;
            for sensor in sensors.members {
                if chassis_id == *"PDB_0" {
                    // get amps and watts for power supply
                    if sensor.odata_id.contains("HSC_0_Pwr") {
                        url = sensor
                            .odata_id
                            .replace(&format!("/{REDFISH_ENDPOINT}/"), "");
                        let (_status_code, t): (StatusCode, Sensor) =
                            self.s.client.get(&url).await?;
                        hsc0.last_power_output_watts = t.reading;
                        hsc0.power_output_watts = t.reading;
                        hsc0.power_capacity_watts = t.reading_range_max;
                    }
                    if sensor.odata_id.contains("HSC_0_Cur") {
                        url = sensor
                            .odata_id
                            .replace(&format!("/{REDFISH_ENDPOINT}/"), "");
                        let (_status_code, t): (StatusCode, Sensor) =
                            self.s.client.get(&url).await?;
                        hsc0.power_output_amps = t.reading;
                    }
                    if sensor.odata_id.contains("HSC_1_Pwr") {
                        url = sensor
                            .odata_id
                            .replace(&format!("/{REDFISH_ENDPOINT}/"), "");
                        let (_status_code, t): (StatusCode, Sensor) =
                            self.s.client.get(&url).await?;
                        hsc1.last_power_output_watts = t.reading;
                        hsc1.power_output_watts = t.reading;
                        hsc1.power_capacity_watts = t.reading_range_max;
                    }
                    if sensor.odata_id.contains("HSC_1_Cur") {
                        url = sensor
                            .odata_id
                            .replace(&format!("/{REDFISH_ENDPOINT}/"), "");
                        let (_status_code, t): (StatusCode, Sensor) =
                            self.s.client.get(&url).await?;
                        hsc1.power_output_amps = t.reading;
                    }
                }
                // now all voltage sensors in all chassis
                if !sensor.odata_id.contains("Volt") {
                    continue;
                }
                url = sensor
                    .odata_id
                    .replace(&format!("/{REDFISH_ENDPOINT}/"), "");
                let (_status_code, t): (StatusCode, Sensor) = self.s.client.get(&url).await?;
                let sensor: Voltages = Voltages::from(t);
                voltages.push(sensor);
            }
        }

        power_supplies.push(hsc0);
        power_supplies.push(hsc1);
        let power = Power {
            odata: None,
            id: "Power".to_string(),
            name: "Power".to_string(),
            power_control: vec![],
            power_supplies: Some(power_supplies),
            voltages: Some(voltages),
            redundancy: None,
        };
        Ok(power)
    }

    async fn power(&self, action: crate::SystemPowerControl) -> Result<(), RedfishError> {
        if action == crate::SystemPowerControl::ACPowercycle {
            let args: HashMap<String, String> =
                HashMap::from([("ResetType".to_string(), "AuxPowerCycle".to_string())]);
            return self
                .s
                .client
                .post(
                    "Chassis/BMC_0/Actions/Oem/NvidiaChassis.AuxPowerReset",
                    args,
                )
                .await
                .map(|_status_code| ());
        }

        self.s.power(action).await
    }

    fn ac_powercycle_supported_by_power(&self) -> bool {
        true
    }

    async fn bmc_reset(&self) -> Result<(), RedfishError> {
        self.s.bmc_reset().await
    }

    async fn chassis_reset(
        &self,
        chassis_id: &str,
        reset_type: crate::SystemPowerControl,
    ) -> Result<(), RedfishError> {
        self.s.chassis_reset(chassis_id, reset_type).await
    }

    async fn get_thermal_metrics(&self) -> Result<crate::Thermal, RedfishError> {
        let mut temperatures = Vec::new();
        let mut fans = Vec::new();
        let mut leak_detectors = Vec::new();

        // gb200 bianca has temperature sensors in several chassis items
        let chassis_all = self.s.get_chassis_all().await?;
        for chassis_id in chassis_all {
            let mut url = format!("Chassis/{}", chassis_id);
            let (_status_code, chassis): (StatusCode, Chassis) = self.s.client.get(&url).await?;
            if chassis.thermal_subsystem.is_some() {
                url = format!("Chassis/{}/ThermalSubsystem/ThermalMetrics", chassis_id);
                let (_status_code, temps): (StatusCode, TemperaturesOemNvidia) =
                    self.s.client.get(&url).await?;
                if let Some(temp) = temps.temperature_readings_celsius {
                    for t in temp {
                        let sensor: Temperature = Temperature::from(t);
                        temperatures.push(sensor);
                    }
                }
                // currently the gb200 bianca board we have uses liquid cooling
                // walk through leak detection sensors and add those
                url = format!(
                    "Chassis/{}/ThermalSubsystem/LeakDetection/LeakDetectors",
                    chassis_id
                );

                let res: Result<(StatusCode, Sensors), RedfishError> =
                    self.s.client.get(&url).await;

                if let Ok((_, sensors)) = res {
                    for sensor in sensors.members {
                        url = sensor
                            .odata_id
                            .replace(&format!("/{REDFISH_ENDPOINT}/"), "");
                        let (_status_code, l): (StatusCode, LeakDetector) =
                            self.s.client.get(&url).await?;
                        leak_detectors.push(l);
                    }
                }
            }
            if chassis.sensors.is_some() {
                // Special handling for GB200s that may not have all their drives installed but still have sensors
                if let Some(backplane_num) = chassis_id.strip_prefix("StorageBackplane_") {
                    url = format!("Chassis/{}/Drives", chassis_id);

                    // Fetch drives and find their respective sensor
                    if let Ok((_status_code, drives)) =
                        self.s.client.get::<DriveCollection>(&url).await
                    {
                        for sensor in drives
                            .members
                            .iter()
                            .filter_map(|drive| {
                                // Extract drive slot ID: "/path/NVMe_SSD_200" -> "200" -> 200
                                let drive_id = drive
                                    .odata_id
                                    .split('/')
                                    .next_back()?
                                    .split('_')
                                    .next_back()?
                                    .parse::<u32>()
                                    .ok()?;

                                Some((drive_id % 4, backplane_num))
                            })
                            .map(|(sensor_index, backplane)| {
                                format!(
                                    "Chassis/{}/Sensors/StorageBackplane_{}_SSD_{}_Temp_0",
                                    chassis_id, backplane, sensor_index
                                )
                            })
                        {
                            // Fetch sensor and add to temperatures if successful
                            if let Ok((_status_code, sensor_data)) =
                                self.s.client.get::<Sensor>(&sensor).await
                            {
                                temperatures.push(Temperature::from(sensor_data));
                            }
                        }
                    }
                } else {
                    // walk through Chassis/*/Sensors/*/*Temp*/
                    url = format!("Chassis/{}/Sensors", chassis_id);
                    let (_status_code, sensors): (StatusCode, Sensors) =
                        self.s.client.get(&url).await?;
                    for sensor in sensors.members {
                        if !sensor.odata_id.contains("Temp") {
                            continue;
                        }
                        url = sensor
                            .odata_id
                            .replace(&format!("/{REDFISH_ENDPOINT}/"), "");
                        let (_status_code, t): (StatusCode, Sensor) =
                            self.s.client.get(&url).await?;
                        let sensor: Temperature = Temperature::from(t);
                        temperatures.push(sensor);
                    }
                }
            }

            // gb200 has fans under chassis sensors instead of thermal like other vendors, look for them in Chassis_0
            if chassis_id == *"Chassis_0" {
                url = format!("Chassis/{}/Sensors", chassis_id);
                let (_status_code, sensors): (StatusCode, Sensors) =
                    self.s.client.get(&url).await?;
                for sensor in sensors.members {
                    if sensor.odata_id.contains("FAN") {
                        url = sensor
                            .odata_id
                            .replace(&format!("/{REDFISH_ENDPOINT}/"), "");
                        let (_status_code, fan): (StatusCode, Fan) =
                            self.s.client.get(&url).await?;
                        fans.push(fan);
                    }
                }
            }
        }
        let thermals = Thermal {
            temperatures,
            fans,
            leak_detectors: Some(leak_detectors),
            ..Default::default()
        };
        Ok(thermals)
    }

    async fn get_gpu_sensors(&self) -> Result<Vec<GPUSensors>, RedfishError> {
        Err(RedfishError::NotSupported(
            "GB200 has no sensors under Chassis/HGX_GPU_#/Sensors/".to_string(),
        ))
    }

    async fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError> {
        self.get_system_event_log().await
    }

    async fn get_bmc_event_log(
        &self,
        from: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<LogEntry>, RedfishError> {
        self.s.get_bmc_event_log(from).await
    }

    async fn get_drives_metrics(&self) -> Result<Vec<Drives>, RedfishError> {
        self.s.get_drives_metrics().await
    }

    async fn machine_setup(
        &self,
        _boot_interface_mac: Option<&str>,
        _bios_profiles: &HashMap<
            RedfishVendor,
            HashMap<String, HashMap<BiosProfileType, HashMap<String, serde_json::Value>>>,
        >,
        _selected_profile: BiosProfileType,
    ) -> Result<(), RedfishError> {
        self.disable_secure_boot().await?;

        let bios_attrs = self.machine_setup_attrs().await?;
        let mut attrs = HashMap::new();
        attrs.extend(bios_attrs);
        let body = HashMap::from([("Attributes", attrs)]);
        let url = format!("Systems/{}/Bios/Settings", self.s.system_id());
        self.s.client.patch(&url, body).await.map(|_status_code| ())
    }

    async fn machine_setup_status(
        &self,
        boot_interface_mac: Option<&str>,
    ) -> Result<MachineSetupStatus, RedfishError> {
        // Check BIOS and BMC attributes
        let mut diffs = self.diff_bios_bmc_attr().await?;

        // Check the first boot option
        if let Some(mac) = boot_interface_mac {
            let (expected, actual) = self.get_expected_and_actual_first_boot_option(mac).await?;
            if expected.is_none() || expected != actual {
                diffs.push(MachineSetupDiff {
                    key: "boot_first".to_string(),
                    expected: expected.unwrap_or_else(|| "Not found".to_string()),
                    actual: actual.unwrap_or_else(|| "Not found".to_string()),
                });
            }
        }

        // We don't lockdown on GB200, so we don't need to check for it

        Ok(MachineSetupStatus {
            is_done: diffs.is_empty(),
            diffs,
        })
    }

    async fn set_machine_password_policy(&self) -> Result<(), RedfishError> {
        use serde_json::Value::Number;
        // These are also the defaults
        let body = HashMap::from([
            /* we were able to set AccountLockoutThreshold on the initial 3 GB200 trays we received
               however, with the recent trays we received, it is not happy with setting a value of 0
               for AccountLockoutThreshold: "The property 'AccountLockoutThreshold' with the requested value
               of '0' could not be written because the value does not meet the constraints of the implementation."
               Never lock
              ("AccountLockoutThreshold", Number(0.into())),

              instead, use the same threshold that we picked for vikings: the bmc will lock the account out after 4 attempts
            */
            ("AccountLockoutThreshold", Number(4.into())),
            // 600 is the smallest value it will accept. 10 minutes, in seconds.
            ("AccountLockoutDuration", Number(600.into())),
        ]);
        self.s
            .client
            .patch("AccountService", body)
            .await
            .map(|_status_code| ())
    }

    async fn lockdown(&self, _target: crate::EnabledDisabled) -> Result<(), RedfishError> {
        // OpenBMC does not provide a lockdown
        Ok(())
    }

    async fn lockdown_status(&self) -> Result<crate::Status, RedfishError> {
        self.s.lockdown_status().await
    }

    async fn setup_serial_console(&self) -> Result<(), RedfishError> {
        self.s.setup_serial_console().await
    }

    async fn serial_console_status(&self) -> Result<crate::Status, RedfishError> {
        self.s.serial_console_status().await
    }

    async fn get_boot_options(&self) -> Result<crate::BootOptions, RedfishError> {
        self.s.get_boot_options().await
    }

    async fn get_boot_option(&self, option_id: &str) -> Result<BootOption, RedfishError> {
        self.s.get_boot_option(option_id).await
    }

    async fn boot_once(&self, target: crate::Boot) -> Result<(), RedfishError> {
        match target {
            crate::Boot::Pxe => {
                self.set_boot_override(
                    BootSourceOverrideTarget::Pxe,
                    BootSourceOverrideEnabled::Once,
                )
                .await
            }
            crate::Boot::HardDisk => {
                self.set_boot_override(
                    BootSourceOverrideTarget::Hdd,
                    BootSourceOverrideEnabled::Once,
                )
                .await
            }
            crate::Boot::UefiHttp => {
                self.set_boot_override(
                    BootSourceOverrideTarget::UefiHttp,
                    BootSourceOverrideEnabled::Once,
                )
                .await
            }
        }
    }

    async fn boot_first(&self, target: crate::Boot) -> Result<(), RedfishError> {
        match target {
            crate::Boot::Pxe => self.set_boot_order(BootOptionName::Pxe).await,
            crate::Boot::HardDisk => {
                // We're looking for a UefiDevicePath like this:
                // HD(1,GPT,A04D0F1E-E02F-4725-9434-0699B52D8FF2,0x800,0x100000)/\\EFI\\ubuntu\\shimaa64.efi
                // The DisplayName will be something like "ubuntu".
                let boot_array = self
                    .get_boot_options_ids_with_first(
                        BootOptionName::Hdd,
                        BootOptionMatchField::UefiDevicePath,
                        None,
                    )
                    .await?;
                self.change_boot_order(boot_array).await
            }
            crate::Boot::UefiHttp => self.set_boot_order(BootOptionName::Http).await,
        }
    }

    async fn clear_tpm(&self) -> Result<(), RedfishError> {
        self.s.clear_tpm().await
    }

    async fn pcie_devices(&self) -> Result<Vec<crate::PCIeDevice>, RedfishError> {
        self.s.pcie_devices().await
    }

    async fn update_firmware(
        &self,
        firmware: tokio::fs::File,
    ) -> Result<crate::model::task::Task, RedfishError> {
        self.s.update_firmware(firmware).await
    }

    async fn get_update_service(&self) -> Result<UpdateService, RedfishError> {
        self.s.get_update_service().await
    }

    async fn update_firmware_multipart(
        &self,
        filename: &Path,
        _reboot: bool,
        timeout: Duration,
        component_type: ComponentType,
    ) -> Result<String, RedfishError> {
        let firmware = File::open(&filename)
            .await
            .map_err(|e| RedfishError::FileError(format!("Could not open file: {}", e)))?;

        let update_service = self.s.get_update_service().await?;

        if update_service.multipart_http_push_uri.is_empty() {
            return Err(RedfishError::NotSupported(
                "Host BMC does not support HTTP multipart push".to_string(),
            ));
        }

        let parameters =
            serde_json::to_string(&UpdateParameters::new(component_type)).map_err(|e| {
                RedfishError::JsonSerializeError {
                    url: "".to_string(),
                    object_debug: "".to_string(),
                    source: e,
                }
            })?;

        let (_status_code, _loc, body) = self
            .s
            .client
            .req_update_firmware_multipart(
                filename,
                firmware,
                parameters,
                &update_service.multipart_http_push_uri,
                true,
                timeout,
            )
            .await?;

        let task: Task =
            serde_json::from_str(&body).map_err(|e| RedfishError::JsonDeserializeError {
                url: update_service.multipart_http_push_uri,
                body,
                source: e,
            })?;

        Ok(task.id)
    }

    async fn bios(
        &self,
    ) -> Result<std::collections::HashMap<String, serde_json::Value>, RedfishError> {
        self.s.bios().await
    }

    async fn set_bios(
        &self,
        values: HashMap<String, serde_json::Value>,
    ) -> Result<(), RedfishError> {
        self.s.set_bios(values).await
    }

    async fn reset_bios(&self) -> Result<(), RedfishError> {
        self.s.reset_bios().await
    }

    async fn pending(
        &self,
    ) -> Result<std::collections::HashMap<String, serde_json::Value>, RedfishError> {
        self.s.pending().await
    }

    async fn clear_pending(&self) -> Result<(), RedfishError> {
        self.s.clear_pending().await
    }

    async fn get_system(&self) -> Result<ComputerSystem, RedfishError> {
        self.s.get_system().await
    }

    async fn get_secure_boot(&self) -> Result<crate::model::secure_boot::SecureBoot, RedfishError> {
        self.s.get_secure_boot().await
    }

    async fn enable_secure_boot(&self) -> Result<(), RedfishError> {
        self.s.enable_secure_boot().await
    }

    async fn disable_secure_boot(&self) -> Result<(), RedfishError> {
        self.s.disable_secure_boot().await
    }

    async fn get_secure_boot_certificate(
        &self,
        database_id: &str,
        certificate_id: &str,
    ) -> Result<Certificate, RedfishError> {
        self.s
            .get_secure_boot_certificate(database_id, certificate_id)
            .await
    }

    async fn get_secure_boot_certificates(
        &self,
        database_id: &str,
    ) -> Result<Vec<String>, RedfishError> {
        self.s.get_secure_boot_certificates(database_id).await
    }

    async fn add_secure_boot_certificate(
        &self,
        pem_cert: &str,
        database_id: &str,
    ) -> Result<Task, RedfishError> {
        self.s
            .add_secure_boot_certificate(pem_cert, database_id)
            .await
    }

    async fn get_chassis_all(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_chassis_all().await
    }

    async fn get_chassis(&self, id: &str) -> Result<crate::Chassis, RedfishError> {
        self.s.get_chassis(id).await
    }

    async fn get_chassis_assembly(&self, chassis_id: &str) -> Result<Assembly, RedfishError> {
        self.s.get_chassis_assembly(chassis_id).await
    }

    async fn get_chassis_network_adapters(
        &self,
        chassis_id: &str,
    ) -> Result<Vec<String>, RedfishError> {
        self.s.get_chassis_network_adapters(chassis_id).await
    }

    async fn get_chassis_network_adapter(
        &self,
        chassis_id: &str,
        id: &str,
    ) -> Result<NetworkAdapter, RedfishError> {
        self.s.get_chassis_network_adapter(chassis_id, id).await
    }

    async fn get_base_network_adapters(
        &self,
        system_id: &str,
    ) -> Result<Vec<String>, RedfishError> {
        self.s.get_base_network_adapters(system_id).await
    }

    async fn get_base_network_adapter(
        &self,
        system_id: &str,
        id: &str,
    ) -> Result<NetworkAdapter, RedfishError> {
        self.s.get_base_network_adapter(system_id, id).await
    }

    async fn get_manager_ethernet_interfaces(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_manager_ethernet_interfaces().await
    }

    async fn get_manager_ethernet_interface(
        &self,
        id: &str,
    ) -> Result<crate::EthernetInterface, RedfishError> {
        self.s.get_manager_ethernet_interface(id).await
    }

    async fn get_system_ethernet_interfaces(&self) -> Result<Vec<String>, RedfishError> {
        Ok(vec![])
    }

    async fn get_system_ethernet_interface(
        &self,
        id: &str,
    ) -> Result<crate::EthernetInterface, RedfishError> {
        Err(RedfishError::NotSupported(format!(
            "GB200 doesn't have Systems EthernetInterface {id}"
        )))
    }

    async fn get_ports(
        &self,
        chassis_id: &str,
        network_adapter: &str,
    ) -> Result<Vec<String>, RedfishError> {
        let url = format!(
            "Chassis/{}/NetworkAdapters/{}/Ports",
            chassis_id, network_adapter
        );
        self.s.get_members(&url).await
    }

    async fn get_port(
        &self,
        chassis_id: &str,
        network_adapter: &str,
        id: &str,
    ) -> Result<crate::NetworkPort, RedfishError> {
        let url = format!(
            "Chassis/{}/NetworkAdapters/{}/Ports/{}",
            chassis_id, network_adapter, id
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    async fn get_network_device_function(
        &self,
        _chassis_id: &str,
        _id: &str,
        _port: Option<&str>,
    ) -> Result<NetworkDeviceFunction, RedfishError> {
        Err(RedfishError::NotSupported(
            "GB200 doesn't have Device Functions in NetworkAdapters yet".to_string(),
        ))
    }

    /// http://redfish.dmtf.org/schemas/v1/NetworkDeviceFunctionCollection.json
    async fn get_network_device_functions(
        &self,
        _chassis_id: &str,
    ) -> Result<Vec<String>, RedfishError> {
        Err(RedfishError::NotSupported(
            "GB200 doesn't have Device Functions in NetworkAdapters yet".to_string(),
        ))
    }

    // Set current_uefi_password to "" if there isn't one yet. By default there isn't a password.
    /// Set new_uefi_password to "" to disable it.
    async fn change_uefi_password(
        &self,
        current_uefi_password: &str,
        new_uefi_password: &str,
    ) -> Result<Option<String>, RedfishError> {
        self.s
            .change_bios_password(UEFI_PASSWORD_NAME, current_uefi_password, new_uefi_password)
            .await
    }

    async fn change_boot_order(&self, boot_array: Vec<String>) -> Result<(), RedfishError> {
        let body = HashMap::from([("Boot", HashMap::from([("BootOrder", boot_array)]))]);
        let url = format!("Systems/{}/Settings", self.s.system_id());
        self.s.client.patch(&url, body).await?;
        Ok(())
    }

    async fn get_service_root(&self) -> Result<ServiceRoot, RedfishError> {
        self.s.get_service_root().await
    }

    async fn get_systems(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_systems().await
    }

    async fn get_managers(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_managers().await
    }

    async fn get_manager(&self) -> Result<Manager, RedfishError> {
        self.s.get_manager().await
    }

    async fn bmc_reset_to_defaults(&self) -> Result<(), RedfishError> {
        self.s.bmc_reset_to_defaults().await
    }

    async fn get_job_state(&self, job_id: &str) -> Result<JobState, RedfishError> {
        self.s.get_job_state(job_id).await
    }

    async fn get_collection(&self, id: ODataId) -> Result<Collection, RedfishError> {
        self.s.get_collection(id).await
    }

    async fn get_resource(&self, id: ODataId) -> Result<Resource, RedfishError> {
        self.s.get_resource(id).await
    }

    async fn set_boot_order_dpu_first(
        &self,
        address: &str,
    ) -> Result<Option<String>, RedfishError> {
        let mac_address = address.replace(':', "").to_uppercase();
        let boot_option_name =
            format!("{} (MAC:{})", BootOptionName::Http.to_string(), mac_address);
        let boot_array = self
            .get_boot_options_ids_with_first(
                BootOptionName::Http,
                BootOptionMatchField::DisplayName,
                Some(&boot_option_name),
            )
            .await?;
        self.change_boot_order(boot_array).await?;
        Ok(None)
    }

    async fn clear_uefi_password(
        &self,
        current_uefi_password: &str,
    ) -> Result<Option<String>, RedfishError> {
        self.change_uefi_password(current_uefi_password, "").await
    }

    async fn get_base_mac_address(&self) -> Result<Option<String>, RedfishError> {
        self.s.get_base_mac_address().await
    }

    async fn lockdown_bmc(&self, target: crate::EnabledDisabled) -> Result<(), RedfishError> {
        self.s.lockdown_bmc(target).await
    }

    async fn is_ipmi_over_lan_enabled(&self) -> Result<bool, RedfishError> {
        self.s.is_ipmi_over_lan_enabled().await
    }

    async fn enable_ipmi_over_lan(
        &self,
        target: crate::EnabledDisabled,
    ) -> Result<(), RedfishError> {
        self.s.enable_ipmi_over_lan(target).await
    }

    async fn update_firmware_simple_update(
        &self,
        image_uri: &str,
        targets: Vec<String>,
        transfer_protocol: TransferProtocolType,
    ) -> Result<Task, RedfishError> {
        self.s
            .update_firmware_simple_update(image_uri, targets, transfer_protocol)
            .await
    }

    async fn enable_rshim_bmc(&self) -> Result<(), RedfishError> {
        self.s.enable_rshim_bmc().await
    }

    async fn clear_nvram(&self) -> Result<(), RedfishError> {
        self.s.clear_nvram().await
    }

    async fn get_nic_mode(&self) -> Result<Option<NicMode>, RedfishError> {
        self.s.get_nic_mode().await
    }

    async fn set_nic_mode(&self, mode: NicMode) -> Result<(), RedfishError> {
        self.s.set_nic_mode(mode).await
    }

    async fn enable_infinite_boot(&self) -> Result<(), RedfishError> {
        let attrs: HashMap<String, serde_json::Value> =
            HashMap::from([("EmbeddedUefiShell".to_string(), "Disabled".into())]);
        let body = HashMap::from([("Attributes", attrs)]);
        let url = format!("Systems/{}/Bios/Settings", self.s.system_id());
        self.s.client.patch(&url, body).await.map(|_status_code| ())
    }

    async fn is_infinite_boot_enabled(&self) -> Result<Option<bool>, RedfishError> {
        let embedded_uefi_shell = self.get_embedded_uefi_shell_status().await?;
        // Infinite boot is enabled when EmbeddedUefiShell is disabled
        Ok(Some(embedded_uefi_shell == EnabledDisabled::Disabled))
    }

    async fn set_host_rshim(&self, enabled: EnabledDisabled) -> Result<(), RedfishError> {
        self.s.set_host_rshim(enabled).await
    }

    async fn get_host_rshim(&self) -> Result<Option<EnabledDisabled>, RedfishError> {
        self.s.get_host_rshim().await
    }

    async fn set_idrac_lockdown(&self, enabled: EnabledDisabled) -> Result<(), RedfishError> {
        self.s.set_idrac_lockdown(enabled).await
    }

    async fn get_boss_controller(&self) -> Result<Option<String>, RedfishError> {
        self.s.get_boss_controller().await
    }

    async fn decommission_storage_controller(
        &self,
        controller_id: &str,
    ) -> Result<Option<String>, RedfishError> {
        self.s.decommission_storage_controller(controller_id).await
    }

    async fn create_storage_volume(
        &self,
        controller_id: &str,
        volume_name: &str,
        raid_type: &str,
    ) -> Result<Option<String>, RedfishError> {
        self.s
            .create_storage_volume(controller_id, volume_name, raid_type)
            .await
    }

    async fn is_boot_order_setup(&self, boot_interface_mac: &str) -> Result<bool, RedfishError> {
        let (expected, actual) = self
            .get_expected_and_actual_first_boot_option(boot_interface_mac)
            .await?;
        Ok(expected.is_some() && expected == actual)
    }

    async fn is_bios_setup(&self, _boot_interface_mac: Option<&str>) -> Result<bool, RedfishError> {
        let diffs = self.diff_bios_bmc_attr().await?;
        Ok(diffs.is_empty())
    }

    async fn get_component_integrities(&self) -> Result<ComponentIntegrities, RedfishError> {
        self.s.get_component_integrities().await
    }

    async fn get_firmware_for_component(
        &self,
        component_integrity_id: &str,
    ) -> Result<crate::model::software_inventory::SoftwareInventory, RedfishError> {
        let mut id = None;

        for value in get_component_integrity_id_to_firmware_inventory_id_options()? {
            if let Some(capture) = value.pattern.captures(component_integrity_id) {
                id = Some(format!(
                    "{}{}",
                    value.id_prefix,
                    capture
                        .get(1)
                        .ok_or_else(|| RedfishError::GenericError {
                            error: format!(
                                "Empty capture for {}, id_prefix: {}",
                                component_integrity_id, value.id_prefix
                            )
                        })?
                        .as_str()
                ));
                break;
            }
        }

        let Some(id) = id else {
            return Err(RedfishError::NotSupported(format!(
                "No component match for {}",
                component_integrity_id
            )));
        };
        self.get_firmware(&id).await
    }

    async fn get_component_ca_certificate(
        &self,
        url: &str,
    ) -> Result<crate::model::component_integrity::CaCertificate, RedfishError> {
        self.s.get_component_ca_certificate(url).await
    }

    async fn trigger_evidence_collection(
        &self,
        url: &str,
        nonce: &str,
    ) -> Result<Task, RedfishError> {
        self.s.trigger_evidence_collection(url, nonce).await
    }

    async fn get_evidence(
        &self,
        url: &str,
    ) -> Result<crate::model::component_integrity::Evidence, RedfishError> {
        self.s.get_evidence(url).await
    }
}

impl Bmc {
    /// Check BIOS and BMC attributes and return differences
    async fn diff_bios_bmc_attr(&self) -> Result<Vec<MachineSetupDiff>, RedfishError> {
        let mut diffs = vec![];

        // Check BIOS and BMC attributes
        let sb = self.get_secure_boot().await?;
        if sb.secure_boot_enable.unwrap_or(false) {
            diffs.push(MachineSetupDiff {
                key: "SecureBoot".to_string(),
                expected: "false".to_string(),
                actual: "true".to_string(),
            });
        }

        let bios = self.s.bios_attributes().await?;
        let expected_attrs = self.machine_setup_attrs().await?;
        for (key, expected) in expected_attrs {
            let Some(actual) = bios.get(&key) else {
                diffs.push(MachineSetupDiff {
                    key: key.to_string(),
                    expected: expected.to_string(),
                    actual: "_missing_".to_string(),
                });
                continue;
            };
            // expected and actual are serde_json::Value which are not comparable, so to_string
            let act = actual.to_string();
            let exp = expected.to_string();
            if act != exp {
                diffs.push(MachineSetupDiff {
                    key: key.to_string(),
                    expected: exp,
                    actual: act,
                });
            }
        }

        Ok(diffs)
    }

    async fn get_expected_and_actual_first_boot_option(
        &self,
        boot_interface_mac: &str,
    ) -> Result<(Option<String>, Option<String>), RedfishError> {
        let mac_address = boot_interface_mac.replace(':', "").to_uppercase();
        let boot_option_name =
            format!("{} (MAC:{})", BootOptionName::Http.to_string(), mac_address);

        let boot_options = self.s.get_system().await?.boot.boot_order;

        // Get actual first boot option
        let actual_first_boot_option = if let Some(first) = boot_options.first() {
            Some(self.s.get_boot_option(first.as_str()).await?.display_name)
        } else {
            None
        };

        // Find expected boot option
        let mut expected_first_boot_option = None;
        for member in &boot_options {
            let b = self.s.get_boot_option(member.as_str()).await?;
            if b.display_name.starts_with(&boot_option_name) {
                expected_first_boot_option = Some(b.display_name);
                break;
            }
        }

        Ok((expected_first_boot_option, actual_first_boot_option))
    }

    async fn set_boot_override(
        &self,
        override_target: BootSourceOverrideTarget,
        override_enabled: BootSourceOverrideEnabled,
    ) -> Result<(), RedfishError> {
        let mut data: HashMap<String, String> = HashMap::new();
        data.insert(
            "BootSourceOverrideEnabled".to_string(),
            format!("{}", override_enabled),
        );
        data.insert(
            "BootSourceOverrideTarget".to_string(),
            format!("{}", override_target),
        );
        let url = format!("Systems/{}/Settings ", self.s.system_id());
        self.s
            .client
            .patch(&url, HashMap::from([("Boot", data)]))
            .await?;
        Ok(())
    }

    // name: The name of the device you want to make the first boot choice.
    async fn set_boot_order(&self, name: BootOptionName) -> Result<(), RedfishError> {
        let boot_array = self
            .get_boot_options_ids_with_first(name, BootOptionMatchField::DisplayName, None)
            .await?;
        self.change_boot_order(boot_array).await
    }

    // A Vec of string boot option names, with the one you want first.
    //
    // Example: get_boot_options_ids_with_first(lenovo::BootOptionName::Network) might return
    // ["Boot0003", "Boot0002", "Boot0001", "Boot0004"] where Boot0003 is Network. It has been
    // moved to the front ready for sending as an update.
    // The order of the other boot options does not change.
    //
    // If the boot option you want is not found returns Ok(None)
    async fn get_boot_options_ids_with_first(
        &self,
        with_name: BootOptionName,
        match_field: BootOptionMatchField,
        with_name_str: Option<&str>,
    ) -> Result<Vec<String>, RedfishError> {
        let name_str = with_name_str.unwrap_or(with_name.to_string());
        let mut ordered = Vec::new(); // the final boot options
        let boot_options = self.s.get_system().await?.boot.boot_order;
        let mut found_matching_boot_option = false;
        for member in &boot_options {
            let b: BootOption = self.s.get_boot_option(member.as_str()).await?;
            let is_match = match match_field {
                BootOptionMatchField::DisplayName => b.display_name.starts_with(name_str),
                BootOptionMatchField::UefiDevicePath => {
                    matches!(b.uefi_device_path, Some(x) if x.starts_with(name_str))
                }
            };
            if is_match {
                ordered.insert(0, b.id);
                found_matching_boot_option = true;
            } else {
                ordered.push(b.id);
            }
        }

        if !found_matching_boot_option {
            return Err(RedfishError::GenericError { error: format!("Could not find boot option matching {name_str} on {}; boot options: {boot_options:#?}", match_field) });
        }

        Ok(ordered)
    }

    async fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError> {
        let url = format!("Systems/{}/LogServices/SEL/Entries", self.s.system_id());
        let (_status_code, log_entry_collection): (_, LogEntryCollection) =
            self.s.client.get(&url).await?;
        let log_entries = log_entry_collection.members;
        Ok(log_entries)
    }

    async fn machine_setup_attrs(&self) -> Result<Vec<(String, serde_json::Value)>, RedfishError> {
        let mut bios_attrs: Vec<(String, serde_json::Value)> = vec![];

        // Enabled TPM
        bios_attrs.push(("TPM".into(), "Enabled".into()));

        // Disabled EmbeddedUefiShell (infinite boot workaround)
        bios_attrs.push(("EmbeddedUefiShell".into(), "Disabled".into()));

        // Enable Option ROM so that the DPU will show up in the Host's network devce list
        // Otherwise, we will never see the DPU's Host PF MAC in the boot option list
        if let Some(curr_bios_attributes) = self.s.bios_attributes().await?.as_object() {
            for attribute in curr_bios_attributes.keys() {
                if attribute.contains("Pcie6DisableOptionROM") {
                    bios_attrs.push((attribute.into(), false.into()));
                }
            }
        }

        Ok(bios_attrs)
    }

    // Get the current status of the EmbeddedUefiShell BIOS attribute
    async fn get_embedded_uefi_shell_status(&self) -> Result<EnabledDisabled, RedfishError> {
        if let Some(bios_attributes) = self.s.bios_attributes().await?.as_object() {
            let embedded_uefi_shell = bios_attributes
                .get("EmbeddedUefiShell")
                .and_then(|v| v.as_str())
                .ok_or_else(|| RedfishError::MissingKey {
                    key: "EmbeddedUefiShell".to_string(),
                    url: format!("Systems/{}/Bios", self.s.system_id()),
                })?;

            match embedded_uefi_shell {
                "Enabled" => Ok(EnabledDisabled::Enabled),
                "Disabled" => Ok(EnabledDisabled::Disabled),
                _ => Err(RedfishError::InvalidValue {
                    url: format!("Systems/{}/Bios", self.s.system_id()),
                    field: "EmbeddedUefiShell".to_string(),
                    err: crate::model::InvalidValueError(format!(
                        "Expected 'Enabled' or 'Disabled', got '{}'",
                        embedded_uefi_shell
                    )),
                }),
            }
        } else {
            Err(RedfishError::InvalidKeyType {
                key: "Attributes".to_string(),
                expected_type: "Object".to_string(),
                url: format!("Systems/{}/Bios", self.s.system_id()),
            })
        }
    }
}

// UpdateParameters is what is sent for a multipart firmware upload's metadata.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct UpdateParameters {
    #[serde(skip_serializing_if = "Option::is_none")]
    targets: Option<Vec<String>>,
    force_update: bool,
}

impl UpdateParameters {
    pub fn new(component: ComponentType) -> UpdateParameters {
        let targets = match component {
            ComponentType::Unknown => None,
            _ => Some(vec![match component {
                ComponentType::BMC => "/redfish/v1/Chassis/BMC_0".to_string(),
                ComponentType::EROTBMC => "/redfish/v1/Chassis/HGX_ERoT_BMC_0".to_string(),
                ComponentType::EROTBIOS => {
                    "/redfish/v1/UpdateService/FirmwareInventory/EROT_BIOS_0".to_string()
                }
                ComponentType::HGXBMC | ComponentType::UEFI => {
                    "/redfish/v1/Chassis/HGX_Chassis_0".to_string()
                }
                _ => "unreachable".to_string(),
            }]),
        };
        UpdateParameters {
            targets,
            force_update: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_update_parameters_targets_all_variants() {
        let cases: Vec<(ComponentType, Option<Vec<String>>)> = vec![
            (ComponentType::Unknown, None),
            (
                ComponentType::BMC,
                Some(vec!["/redfish/v1/Chassis/BMC_0".to_string()]),
            ),
            (
                ComponentType::EROTBMC,
                Some(vec!["/redfish/v1/Chassis/HGX_ERoT_BMC_0".to_string()]),
            ),
            (
                ComponentType::EROTBIOS,
                Some(vec![
                    "/redfish/v1/UpdateService/FirmwareInventory/EROT_BIOS_0".to_string(),
                ]),
            ),
            (
                ComponentType::HGXBMC,
                Some(vec!["/redfish/v1/Chassis/HGX_Chassis_0".to_string()]),
            ),
            (
                ComponentType::UEFI,
                Some(vec!["/redfish/v1/Chassis/HGX_Chassis_0".to_string()]),
            ),
            (
                ComponentType::CPLDMID,
                Some(vec!["unreachable".to_string()]),
            ),
            (ComponentType::CPLDMB, Some(vec!["unreachable".to_string()])),
            (
                ComponentType::CPLDPDB,
                Some(vec!["unreachable".to_string()]),
            ),
            (
                ComponentType::PSU { num: 1 },
                Some(vec!["unreachable".to_string()]),
            ),
            (
                ComponentType::PCIeSwitch { num: 2 },
                Some(vec!["unreachable".to_string()]),
            ),
            (
                ComponentType::PCIeRetimer { num: 3 },
                Some(vec!["unreachable".to_string()]),
            ),
        ];

        for (component, expected_targets) in cases {
            let params = UpdateParameters::new(component.clone());
            assert_eq!(
                params.targets, expected_targets,
                "Failed for component: {:?}",
                component
            );
            assert!(
                params.force_update,
                "Force update not true for: {:?}",
                component
            );
        }
    }
}
