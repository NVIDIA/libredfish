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

use crate::{Chassis, REDFISH_ENDPOINT};
use std::{collections::HashMap, path::Path, time::Duration};
use reqwest::StatusCode;
use tokio::fs::File;

use crate::model::account_service::ManagerAccount;
use crate::model::sensor::{GPUSensors, Sensor, Sensors};
use crate::model::task::Task;
use crate::model::update_service::{ComponentType, TransferProtocolType, UpdateService};
use crate::{model::{
    boot::{BootSourceOverrideEnabled, BootSourceOverrideTarget},
    chassis::NetworkAdapter,
    sel::{LogEntry, LogEntryCollection},
    service_root::ServiceRoot,
    storage::Drives,
    BootOption, ComputerSystem, Manager, PCIeDevices,
    power::{Power, PowerSupply, Voltages},
    thermal::{LeakDetector, Temperature, TemperaturesOemNvidia, Thermal},
}, standard::RedfishStandard, Collection, NetworkDeviceFunction, ODataId, Redfish, RedfishError, Resource, PCIeDevice};
use crate::{MachineSetupDiff, MachineSetupStatus, JobState, RoleId};
use crate::model::thermal::Fan;

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

enum BootOptionMatchField {
    DisplayName,
    UefiDevicePath,
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
        self.s.get_firmware(id).await
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
        let mut url = format!("Chassis/PDB_0");
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
                if chassis_id == "PDB_0".to_string() {
                    // get amps and watts for power supply
                    if sensor.odata_id.contains("HSC_0_Pwr") {
                        url = sensor.odata_id.replace(&format!("/{REDFISH_ENDPOINT}/"), "");
                        let (_status_code, t): (StatusCode, Sensor) = self.s.client.get(&url).await?;
                        hsc0.last_power_output_watts = t.reading;
                        hsc0.power_output_watts = t.reading;
                        hsc0.power_capacity_watts = t.reading_range_max;
                    }
                    if sensor.odata_id.contains("HSC_0_Cur") {
                        url = sensor.odata_id.replace(&format!("/{REDFISH_ENDPOINT}/"), "");
                        let (_status_code, t): (StatusCode, Sensor) = self.s.client.get(&url).await?;
                        hsc0.power_output_amps = t.reading;
                    }
                    if sensor.odata_id.contains("HSC_1_Pwr") {
                        url = sensor.odata_id.replace(&format!("/{REDFISH_ENDPOINT}/"), "");
                        let (_status_code, t): (StatusCode, Sensor) = self.s.client.get(&url).await?;
                        hsc1.last_power_output_watts = t.reading;
                        hsc1.power_output_watts = t.reading;
                        hsc1.power_capacity_watts = t.reading_range_max;
                    }
                    if sensor.odata_id.contains("HSC_1_Cur") {
                        url = sensor.odata_id.replace(&format!("/{REDFISH_ENDPOINT}/"), "");
                        let (_status_code, t): (StatusCode, Sensor) = self.s.client.get(&url).await?;
                        hsc1.power_output_amps = t.reading;
                    }
                }
                // now all voltage sensors in all chassis
                if !sensor.odata_id.contains("Volt") {
                    continue;
                }
                url = sensor.odata_id.replace(&format!("/{REDFISH_ENDPOINT}/"), "");
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
        self.s.power(action).await
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
            if chassis.thermal.is_some() {
                url = format!("Chassis/{}/ThermalSubsystem/ThermalMetrics", chassis_id);
                let (_status_code, temps): (StatusCode, TemperaturesOemNvidia) = self.s.client.get(&url).await?;
                if let Some(temp) = temps.temperature_readings_celsius {
                    for t in temp {
                        let sensor: Temperature = Temperature::from(t);
                        temperatures.push(sensor);
                    }
                }
                // currently the gb200 bianca board we have uses liquid cooling
                // walk through leak detection sensors and add those
                url = format!("Chassis/{}/ThermalSubsystem/LeakDetection/LeakDetectors", chassis_id);
                let (_status_code, sensors): (StatusCode, Sensors) = self.s.client.get(&url).await?;
                for sensor in sensors.members {
                    url = sensor.odata_id.replace(&format!("/{REDFISH_ENDPOINT}/"), "");
                    let (_status_code, l): (StatusCode, LeakDetector) = self.s.client.get(&url).await?;
                    leak_detectors.push(l);
                }
            }
            if chassis.sensors.is_some() {
                // walk through Chassis/*/Sensors/*/*Temp*/
                url = format!("Chassis/{}/Sensors", chassis_id);
                let (_status_code, sensors): (StatusCode, Sensors) = self.s.client.get(&url).await?;
                for sensor in sensors.members {
                    if !sensor.odata_id.contains("Temp") {
                        continue;
                    }
                    url = sensor.odata_id.replace(&format!("/{REDFISH_ENDPOINT}/"), "");
                    let (_status_code, t): (StatusCode, Sensor) = self.s.client.get(&url).await?;
                    let sensor: Temperature = Temperature::from(t);
                    temperatures.push(sensor);
                }
            }

            // gb200 has fans under chassis sensors instead of thermal like other vendors, look for them in Chassis_0
            if chassis_id == "Chassis_0".to_string() {
                url = format!("Chassis/{}/Sensors", chassis_id);
                let (_status_code, sensors): (StatusCode, Sensors) = self.s.client.get(&url).await?;
                for sensor in sensors.members {
                    if sensor.odata_id.contains("FAN") {
                        url = sensor.odata_id.replace(&format!("/{REDFISH_ENDPOINT}/"), "");
                        let (_status_code, fan): (StatusCode, Fan) = self.s.client.get(&url).await?;
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

    async fn get_drives_metrics(&self) -> Result<Vec<Drives>, RedfishError> {
        self.s.get_drives_metrics().await
    }

    async fn machine_setup(&self, boot_interface_mac: Option<&str>) -> Result<(), RedfishError> {
        self.disable_secure_boot().await?;
        self.set_boot_order_dpu_first(boot_interface_mac).await
    }

    async fn machine_setup_status(&self) -> Result<MachineSetupStatus, RedfishError> {
        let mut diffs = vec![];

        let sb = self.get_secure_boot().await?;
        if sb.secure_boot_enable.unwrap_or(false) {
            diffs.push(MachineSetupDiff {
                key: "SecureBoot".to_string(),
                expected: "false".to_string(),
                actual: "true".to_string(),
            });
        }

        Ok(MachineSetupStatus {
            is_done: diffs.is_empty(),
            diffs,
        })
    }

    async fn set_machine_password_policy(&self) -> Result<(), RedfishError> {
        use serde_json::Value::Number;
        // These are also the defaults
        let body = HashMap::from([
            // Never lock
            ("AccountLockoutThreshold", Number(0.into())),
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
        // OpenBMC does not provide a lockdown currently
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
                // : UefiHttp isn't in the GH200's list of AllowableValues, but it seems to work
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
        let mut out = Vec::new();

        // gb200 bianca has pcie devices on several chassis items
        let chassis_all = self.s.get_chassis_all().await?;
        for chassis_id in chassis_all {
            if chassis_id.contains("BMC") {
                continue;
            }

            let chassis = self.get_chassis(&chassis_id).await?;

            if let Some(member) = chassis.pcie_devices {
                let mut url = member
                    .odata_id
                    .replace(&format!("/{REDFISH_ENDPOINT}/"), "");

                let devices: PCIeDevices = match self.s.client.get(&url).await {
                    Ok((_status, x)) => x,
                    Err(_e) => {
                        continue;
                    }
                };
                for id in devices.members {
                    url = id.odata_id.replace(&format!("/{REDFISH_ENDPOINT}/"), "");
                    let p: PCIeDevice = self.s.client.get(&url).await?.1;
                    if p.id.is_none()
                        || p.status.is_none()
                        || !p
                        .status
                        .as_ref()
                        .unwrap()
                        .state
                        .as_ref()
                        .unwrap()
                        .to_lowercase()
                        .contains("enabled")
                    {
                        continue;
                    }
                    out.push(p);
                }
            }
        }
        out.sort_unstable_by(|a, b| a.manufacturer.partial_cmp(&b.manufacturer).unwrap());
        Ok(out)
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
        _component_type: ComponentType,
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

        let parameters = "{}".to_string();

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

    async fn add_secure_boot_certificate(&self, pem_cert: &str) -> Result<Task, RedfishError> {
        self.s.add_secure_boot_certificate(pem_cert).await
    }

    async fn get_chassis_all(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_chassis_all().await
    }

    async fn get_chassis(&self, id: &str) -> Result<crate::Chassis, RedfishError> {
        self.s.get_chassis(id).await
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
        Err(RedfishError::NotSupported(
            "GB200 doesn't have Systems EthernetInterface".to_string(),
        ))
    }

    async fn get_system_ethernet_interface(
        &self,
        _id: &str,
    ) -> Result<crate::EthernetInterface, RedfishError> {
        Err(RedfishError::NotSupported(
            "GB200 doesn't have Systems EthernetInterface".to_string(),
        ))
    }

    async fn get_ports(&self, chassis_id: &str, network_adapter: &str) -> Result<Vec<String>, RedfishError> {
        let url = format!(
            "Chassis/{}/NetworkAdapters/{}/Ports",
            chassis_id,
            network_adapter
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
        address: Option<&str>,
    ) -> Result<(), RedfishError> {
        let mac_address = match address {
            Some(x) => x.replace(':', "").to_uppercase(),
            None => {
                return Err(RedfishError::NotSupported(
                    "set_dpu_first_boot_order without mac address is not possible on GB200 since NetworkDeviceFunctions and PCIeDevices are missing".to_string(),
                ));
            }
        };
        let boot_option_name = format!("{} (MAC:{})", BootOptionName::Http.to_string(), mac_address);
        let boot_array = self
            .get_boot_options_ids_with_first(BootOptionName::Http, BootOptionMatchField::DisplayName, Some(&boot_option_name))
            .await?;
        self.change_boot_order(boot_array).await
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
}

impl Bmc {
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
        let name_str = with_name_str.unwrap_or(&with_name.to_string());
        let mut ordered = Vec::new(); // the final boot options
        let boot_options = self.s.get_system().await?.boot.boot_order;
        for member in boot_options {
            let b: BootOption = self.s.get_boot_option(member.as_str()).await?;
            let is_match = match match_field {
                BootOptionMatchField::DisplayName => b.display_name.starts_with(name_str),
                BootOptionMatchField::UefiDevicePath => {
                    matches!(b.uefi_device_path, Some(x) if x.starts_with(name_str))
                }
            };
            if is_match {
                ordered.insert(0, b.id);
            } else {
                ordered.push(b.id);
            }
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
}
