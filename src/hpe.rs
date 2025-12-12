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
use std::{collections::HashMap, path::Path, time::Duration};

use serde_json::Value;

use crate::{
    model::{
        account_service::ManagerAccount,
        certificate::Certificate,
        chassis::{Assembly, Chassis, NetworkAdapter},
        component_integrity::ComponentIntegrities,
        network_device_function::NetworkDeviceFunction,
        oem::{
            hpe::{self, BootDevices},
            nvidia_dpu::NicMode,
        },
        power::Power,
        secure_boot::SecureBoot,
        sel::{LogEntry, LogEntryCollection},
        sensor::GPUSensors,
        service_root::{RedfishVendor, ServiceRoot},
        software_inventory::SoftwareInventory,
        storage::{self, Drives},
        task::Task,
        thermal::Thermal,
        update_service::{ComponentType, TransferProtocolType, UpdateService},
        BootOption, ComputerSystem, Manager, Slot, SystemStatus,
    },
    network::REDFISH_ENDPOINT,
    standard::RedfishStandard,
    BiosProfileType, Boot, BootOptions, Collection, Deserialize,
    EnabledDisabled::{self, Disabled, Enabled},
    JobState, MachineSetupDiff, MachineSetupStatus, OData, ODataId, PCIeDevice, PowerState,
    Redfish, RedfishError, Resource, RoleId, Serialize, Status, StatusInternal, SystemPowerControl,
};

// The following is specific for the HPE machine since the HPE redfish
// doesn't return pcie odata.id during power on transition
// HpeOData structure will try to capture all those 4 properties.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct HpeOData {
    #[serde(rename = "@odata.id")]
    pub odata_id: Option<String>, // This is unique for HPE machine
    #[serde(rename = "@odata.type")]
    pub odata_type: String,
    #[serde(rename = "@odata.etag")]
    pub odata_etag: Option<String>,
    #[serde(rename = "@odata.context")]
    pub odata_context: Option<String>,
}
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct HpePCIeDevice {
    #[serde(flatten)]
    pub odata: HpeOData,
    pub description: Option<String>,
    pub firmware_version: Option<String>,
    pub id: Option<String>,
    pub manufacturer: Option<String>,
    #[serde(rename = "GPUVendor")]
    pub gpu_vendor: Option<String>,
    pub name: Option<String>,
    pub part_number: Option<String>,
    pub serial_number: Option<String>,
    pub status: Option<SystemStatus>,
    pub slot: Option<Slot>,
    #[serde(default, rename = "PCIeFunctions")]
    pub pcie_functions: Option<ODataId>,
}

pub struct Bmc {
    s: RedfishStandard,
}

impl Bmc {
    pub fn new(s: RedfishStandard) -> Result<Bmc, RedfishError> {
        Ok(Bmc { s })
    }
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

    async fn get_power_state(&self) -> Result<PowerState, RedfishError> {
        self.s.get_power_state().await
    }

    async fn get_power_metrics(&self) -> Result<Power, RedfishError> {
        self.s.get_power_metrics().await
    }

    async fn power(&self, action: SystemPowerControl) -> Result<(), RedfishError> {
        if action == SystemPowerControl::ForceRestart {
            // hpe ilo does warm reset with gracefulrestart op
            self.s.power(SystemPowerControl::GracefulRestart).await
        } else if action == SystemPowerControl::ACPowercycle {
            let power_state = self.get_power_state().await?;
            match power_state {
                PowerState::Off => {}
                _ => {
                    self.s.power(SystemPowerControl::ForceOff).await?;
                }
            }
            let args: HashMap<String, String> =
                HashMap::from([("ResetType".to_string(), "AuxCycle".to_string())]);
            let url = format!(
                "Systems/{}/Actions/Oem/Hpe/HpeComputerSystemExt.SystemReset",
                self.s.system_id()
            );
            return self.s.client.post(&url, args).await.map(|_status_code| ());
        } else {
            self.s.power(action).await
        }
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
        reset_type: SystemPowerControl,
    ) -> Result<(), RedfishError> {
        self.s.chassis_reset(chassis_id, reset_type).await
    }

    async fn get_thermal_metrics(&self) -> Result<Thermal, RedfishError> {
        self.s.get_thermal_metrics().await
    }

    async fn get_gpu_sensors(&self) -> Result<Vec<GPUSensors>, RedfishError> {
        self.s.get_gpu_sensors().await
    }

    async fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError> {
        self.get_system_event_log().await
    }

    async fn get_bmc_event_log(
        &self,
        from: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<LogEntry>, RedfishError> {
        let manager_id = self.s.manager_id();
        let url = format!("Managers/{manager_id}/LogServices/IEL/Entries");
        self.s.fetch_bmc_event_log(url, from).await
    }

    async fn get_drives_metrics(&self) -> Result<Vec<Drives>, RedfishError> {
        self.s.get_drives_metrics().await
    }

    async fn bios(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        self.s.bios().await
    }

    async fn set_bios(
        &self,
        values: HashMap<String, serde_json::Value>,
    ) -> Result<(), RedfishError> {
        self.s.set_bios(values).await
    }

    async fn reset_bios(&self) -> Result<(), RedfishError> {
        let hp_bios = self.s.bios().await?;
        // Access the Actions map
        let actions = hp_bios
            .get("Actions")
            .and_then(|v: &Value| v.as_object())
            .ok_or(RedfishError::NoContent)?;
        // Access the "#Bios.ResetBios" action
        let reset = actions
            .get("#Bios.ResetBios")
            .and_then(|v| v.as_object())
            .ok_or(RedfishError::NoContent)?;
        // Access the "target" URL
        let target = reset
            .get("target")
            .and_then(|v| v.as_str())
            .ok_or(RedfishError::NoContent)?;
        let url = target.replace(&format!("/{REDFISH_ENDPOINT}/"), "");
        self.s
            .client
            .req::<(), ()>(reqwest::Method::POST, &url, None, None, None, Vec::new())
            .await
            .map(|_resp| Ok(()))?
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
        self.setup_serial_console().await?;
        self.clear_tpm().await?;
        self.set_virt_enable().await?;
        self.set_uefi_nic_boot().await?;
        self.set_boot_order(BootDevices::Pxe).await
    }

    async fn machine_setup_status(
        &self,
        boot_interface_mac: Option<&str>,
    ) -> Result<MachineSetupStatus, RedfishError> {
        // Check BIOS and BMC attributes
        let mut diffs = self.diff_bios_bmc_attr().await?;

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

        // Check lockdown status
        let lockdown = self.lockdown_status().await?;
        if !lockdown.is_fully_enabled() {
            diffs.push(MachineSetupDiff {
                key: "lockdown".to_string(),
                expected: "Enabled".to_string(),
                actual: lockdown.status.to_string(),
            });
        }

        Ok(MachineSetupStatus {
            is_done: diffs.is_empty(),
            diffs,
        })
    }

    async fn set_machine_password_policy(&self) -> Result<(), RedfishError> {
        use serde_json::Value;
        let hpe = Value::Object(serde_json::Map::from_iter(vec![
            (
                "AuthFailureDelayTimeSeconds".to_string(),
                Value::Number(2.into()), // Hpe iLO 5 only allows 2, 5, 10, 30
            ),
            (
                "AuthFailureLoggingThreshold".to_string(),
                Value::Number(0.into()), // Hpe iLO 5 only allows 0, 1, 2, 3, 5
            ),
            (
                "AuthFailuresBeforeDelay".to_string(),
                Value::Number(0.into()), // Hpe iLO 5 only allows 0, 1, 3, 5
            ),
            ("EnforcePasswordComplexity".to_string(), Value::Bool(false)),
        ]));
        let mut oem = serde_json::Map::new();
        oem.insert("Hpe".to_string(), hpe);

        let mut body = HashMap::new();
        body.insert("Oem".to_string(), serde_json::Value::Object(oem));

        self.s
            .client
            .patch("AccountService", body)
            .await
            .map(|_status_code| ())
    }

    async fn lockdown(&self, target: EnabledDisabled) -> Result<(), RedfishError> {
        match target {
            Enabled => self.enable_lockdown().await,
            Disabled => self.disable_lockdown().await,
        }
    }

    async fn lockdown_status(&self) -> Result<Status, RedfishError> {
        let mut url = format!("Systems/{}/Bios", self.s.system_id());
        let (_status_code, bios): (_, hpe::Bios) = self.s.client.get(url.as_str()).await?;
        let bios = bios.attributes;
        url = format!("Managers/{}", self.s.manager_id());
        let (_status, bmc): (_, hpe::SetOemHpeLockdown) = self.s.client.get(url.as_str()).await?;
        let message = format!(
            "usb_boot={}, virtual_nic_enabled={}",
            bios.usb_boot.as_deref().unwrap_or("Unknown"),
            bmc.oem.hpe.virtual_nic_enabled
        );
        // todo: kcs_enabled
        Ok(Status {
            message,
            status: if bios.usb_boot.as_deref() == Some("Disabled")
                && !bmc.oem.hpe.virtual_nic_enabled
            // todo: && bios.kcs_enabled.as_deref() == Some("false")
            {
                StatusInternal::Enabled
            // todo: if bios.usb_boot.as_deref() == Some("Enabled") && bios.kcs_enabled.as_deref() == Some("true")
            } else if bios.usb_boot.as_deref() == Some("Enabled") && bmc.oem.hpe.virtual_nic_enabled
            {
                StatusInternal::Disabled
            } else {
                StatusInternal::Partial
            },
        })
    }

    async fn setup_serial_console(&self) -> Result<(), RedfishError> {
        let serial_console = hpe::BiosSerialConsoleAttributes {
            embedded_serial_port: "Com2Irq3".to_string(),
            ems_console: "Virtual".to_string(),
            serial_console_baud_rate: "BaudRate115200".to_string(),
            serial_console_emulation: "Vt100Plus".to_string(),
            serial_console_port: "Virtual".to_string(),
            uefi_serial_debug_level: "ErrorsOnly".to_string(),
            virtual_serial_port: "Com1Irq4".to_string(),
        };
        let set_serial_attrs = hpe::SetBiosSerialConsoleAttributes {
            attributes: serial_console,
        };
        let url = format!("Systems/{}/Bios/settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_serial_attrs)
            .await
            .map(|_status_code| ())
    }

    async fn serial_console_status(&self) -> Result<Status, RedfishError> {
        self.bios_serial_console_status().await
        // TODO: add bmc serial console service status
    }

    async fn get_boot_options(&self) -> Result<BootOptions, RedfishError> {
        self.s.get_boot_options().await
    }

    async fn get_boot_option(&self, option_id: &str) -> Result<BootOption, RedfishError> {
        self.s.get_boot_option(option_id).await
    }

    async fn boot_first(&self, target: Boot) -> Result<(), RedfishError> {
        // TODO: possibly remove this redundant matching, the enum is based on the bmc capabilities
        match target {
            Boot::Pxe => self.set_boot_order(BootDevices::Pxe).await,
            Boot::HardDisk => self.set_boot_order(BootDevices::Hdd).await,
            Boot::UefiHttp => self.set_boot_order(BootDevices::UefiHttp).await,
        }
    }

    async fn boot_once(&self, target: Boot) -> Result<(), RedfishError> {
        self.boot_first(target).await
    }

    async fn clear_tpm(&self) -> Result<(), RedfishError> {
        let tpm = hpe::TpmAttributes {
            tpm2_operation: "Clear".to_string(),
            tpm_visibility: "Visible".to_string(),
        };
        let set_tpm_attrs = hpe::SetTpmAttributes { attributes: tpm };
        let url = format!("Systems/{}/Bios/settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_tpm_attrs)
            .await
            .map(|_status_code| ())
    }

    async fn pending(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        let url = format!("Systems/{}/Bios/settings/", self.s.system_id());
        self.s.pending_with_url(&url).await
    }

    async fn clear_pending(&self) -> Result<(), RedfishError> {
        // TODO
        Ok(())
    }

    async fn pcie_devices(&self) -> Result<Vec<PCIeDevice>, RedfishError> {
        let mut out = Vec::new();
        let chassis = self.get_chassis(self.s.system_id()).await?;
        let pcie_devices_odata = match chassis.pcie_devices {
            Some(odata) => odata,
            None => return Ok(vec![]),
        };
        let url = pcie_devices_odata
            .odata_id
            .replace(&format!("/{REDFISH_ENDPOINT}/"), "");
        let pcie_devices = self.s.get_members(&url).await?;
        let mut devices: Vec<HpePCIeDevice> = Vec::new();
        for pcie_oid in pcie_devices {
            let dev_url = format!("{}/{}", &url, pcie_oid);
            let (_, hpe_pcie) = self.s.client.get(&dev_url).await?;
            devices.push(hpe_pcie);
        }
        // for mut pcie in devices.members {
        for hpe_pcie in devices {
            let mut pcie = PCIeDevice {
                odata: OData {
                    odata_type: hpe_pcie.odata.odata_type,
                    odata_id: hpe_pcie.odata.odata_id.unwrap_or_default(),
                    odata_etag: hpe_pcie.odata.odata_etag,
                    odata_context: hpe_pcie.odata.odata_context,
                },
                description: hpe_pcie.description,
                firmware_version: hpe_pcie.firmware_version,
                id: hpe_pcie.id,
                manufacturer: hpe_pcie.manufacturer,
                gpu_vendor: hpe_pcie.gpu_vendor,
                name: hpe_pcie.name,
                part_number: hpe_pcie.part_number,
                serial_number: hpe_pcie.serial_number,
                status: hpe_pcie.status,
                slot: hpe_pcie.slot,
                pcie_functions: hpe_pcie.pcie_functions,
            };
            if pcie.status.is_none() {
                continue;
            }
            if let Some(serial) = pcie.serial_number.take() {
                // DPUs has serial numbers like this: "MT2246XZ0908   "
                pcie.serial_number = Some(serial.trim().to_string())
            }
            out.push(pcie);
        }
        out.sort_unstable_by(|a, b| a.manufacturer.cmp(&b.manufacturer));

        Ok(out)
    }

    async fn update_firmware(&self, firmware: tokio::fs::File) -> Result<Task, RedfishError> {
        self.s.update_firmware(firmware).await
    }

    async fn update_firmware_multipart(
        &self,
        filename: &Path,
        reboot: bool,
        timeout: Duration,
        component_type: ComponentType,
    ) -> Result<String, RedfishError> {
        self.s
            .update_firmware_multipart(filename, reboot, timeout, component_type)
            .await
    }

    async fn get_tasks(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_tasks().await
    }

    async fn get_task(&self, id: &str) -> Result<crate::model::task::Task, RedfishError> {
        self.s.get_task(id).await
    }

    async fn get_firmware(&self, id: &str) -> Result<SoftwareInventory, RedfishError> {
        self.s.get_firmware(id).await
    }

    async fn get_software_inventories(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_software_inventories().await
    }

    async fn get_system(&self) -> Result<ComputerSystem, RedfishError> {
        self.s.get_system().await
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

    async fn get_secure_boot(&self) -> Result<SecureBoot, RedfishError> {
        self.s.get_secure_boot().await
    }

    async fn enable_secure_boot(&self) -> Result<(), RedfishError> {
        self.s.enable_secure_boot().await
    }

    async fn disable_secure_boot(&self) -> Result<(), RedfishError> {
        self.s.disable_secure_boot().await
    }

    async fn get_network_device_function(
        &self,
        chassis_id: &str,
        id: &str,
        port: Option<&str>,
    ) -> Result<NetworkDeviceFunction, RedfishError> {
        self.s
            .get_network_device_function(chassis_id, id, port)
            .await
    }

    async fn get_network_device_functions(
        &self,
        chassis_id: &str,
    ) -> Result<Vec<String>, RedfishError> {
        self.s.get_network_device_functions(chassis_id).await
    }

    async fn get_chassis_all(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_members("Chassis").await
    }

    async fn get_chassis(&self, id: &str) -> Result<Chassis, RedfishError> {
        self.s.get_chassis(id).await
    }

    async fn get_chassis_assembly(&self, chassis_id: &str) -> Result<Assembly, RedfishError> {
        self.s.get_chassis_assembly(chassis_id).await
    }

    async fn get_chassis_network_adapters(
        &self,
        chassis_id: &str,
    ) -> Result<Vec<String>, RedfishError> {
        let chassis = self.s.get_chassis(chassis_id).await?;
        if let Some(network_adapters_odata) = chassis.network_adapters {
            let url = network_adapters_odata
                .odata_id
                .replace(&format!("/{REDFISH_ENDPOINT}/"), "");
            // let url = format!("Chassis/{}/NetworkAdapters", chassis_id);
            self.s.get_members(&url).await
        } else {
            Ok(Vec::new())
        }
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
        let url = format!("Systems/{}/BaseNetworkAdapters", system_id);
        self.s.get_members(&url).await
    }

    async fn get_base_network_adapter(
        &self,
        system_id: &str,
        id: &str,
    ) -> Result<NetworkAdapter, RedfishError> {
        let url = format!("Systems/{}/BaseNetworkAdapters/{}", system_id, id);
        let (_, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    async fn get_ports(
        &self,
        chassis_id: &str,
        network_adapter: &str,
    ) -> Result<Vec<String>, RedfishError> {
        self.s.get_ports(chassis_id, network_adapter).await
    }

    async fn get_port(
        &self,
        chassis_id: &str,
        network_adapter: &str,
        id: &str,
    ) -> Result<crate::NetworkPort, RedfishError> {
        self.s.get_port(chassis_id, network_adapter, id).await
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
        self.s.get_system_ethernet_interfaces().await
    }

    async fn get_system_ethernet_interface(
        &self,
        id: &str,
    ) -> Result<crate::EthernetInterface, RedfishError> {
        self.s.get_system_ethernet_interface(id).await
    }

    async fn change_uefi_password(
        &self,
        current_uefi_password: &str,
        new_uefi_password: &str,
    ) -> Result<Option<String>, RedfishError> {
        let hp_bios = self.s.bios().await?;
        // Access the Actions map
        let actions = hp_bios
            .get("Actions")
            .and_then(|v| v.as_object())
            .ok_or(RedfishError::NoContent)?;
        // Access the "#Bios.ChangePassword" action
        let change_password = actions
            .get("#Bios.ChangePassword")
            .and_then(|v| v.as_object())
            .ok_or(RedfishError::NoContent)?;
        // Access the "target" URL
        let target = change_password
            .get("target")
            .and_then(|v| v.as_str())
            .ok_or(RedfishError::NoContent)?;

        let mut arg = HashMap::new();
        arg.insert("PasswordName", "AdministratorPassword".to_string());
        arg.insert("OldPassword", current_uefi_password.to_string());
        arg.insert("NewPassword", new_uefi_password.to_string());

        let url = target.replace(&format!("/{REDFISH_ENDPOINT}/"), "");
        self.s.client.post(&url, arg).await?;

        Ok(None)
    }

    async fn change_boot_order(&self, boot_array: Vec<String>) -> Result<(), RedfishError> {
        self.s.change_boot_order(boot_array).await
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
        let url = format!(
            "Managers/{}/Actions/Oem/Hpe/HpeiLO.ResetToFactoryDefaults",
            self.s.manager_id()
        );
        let mut arg = HashMap::new();
        arg.insert("Action", "HpeiLO.ResetToFactoryDefaults".to_string());
        arg.insert("ResetType", "Default".to_string());
        self.s.client.post(&url, arg).await.map(|_resp| Ok(()))?
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

    async fn get_update_service(&self) -> Result<UpdateService, RedfishError> {
        self.s.get_update_service().await
    }

    async fn set_boot_order_dpu_first(
        &self,
        mac_address: &str,
    ) -> Result<Option<String>, RedfishError> {
        let mac = mac_address.to_string().to_uppercase();

        let all = self.get_boot_options().await?;
        let mut boot_ref = None;
        for b in all.members {
            let id = b.odata_id_get()?;
            let opt = self.get_boot_option(id).await?;
            let opt_name = opt.display_name.to_uppercase();
            if opt_name.contains("HTTP") && opt_name.contains("IPV4") && opt_name.contains(&mac) {
                boot_ref = Some(opt.boot_option_reference);
                break;
            }
        }
        let Some(boot_ref) = boot_ref else {
            return Err(RedfishError::MissingBootOption(format!("HTTP IPv4 {mac}")));
        };

        match self.set_first_boot(&boot_ref).await {
            Err(RedfishError::HTTPErrorCode {
                url,
                status_code,
                response_body,
            }) => {
                if response_body.contains("UnableToModifyDuringSystemPOST") {
                    tracing::info!(
                        "redfish set_first_boot might fail due to HPE POST race condition, ignore."
                    );
                    Ok(None)
                } else {
                    Err(RedfishError::HTTPErrorCode {
                        url,
                        status_code,
                        response_body,
                    })
                }
            }
            Ok(()) => Ok(None),
            Err(e) => Err(e),
        }
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
        self.s.enable_infinite_boot().await
    }

    async fn is_infinite_boot_enabled(&self) -> Result<Option<bool>, RedfishError> {
        self.s.is_infinite_boot_enabled().await
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
        componnent_integrity_id: &str,
    ) -> Result<crate::model::software_inventory::SoftwareInventory, RedfishError> {
        self.s
            .get_firmware_for_component(componnent_integrity_id)
            .await
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

        let sc = self.serial_console_status().await?;
        if !sc.is_fully_enabled() {
            diffs.push(MachineSetupDiff {
                key: "serial_console".to_string(),
                expected: "Enabled".to_string(),
                actual: sc.status.to_string(),
            });
        }

        // clear_tpm has no 'check' operation, so skip that

        let virt = self.get_virt_enabled().await?;
        if virt != EnabledDisabled::Enabled {
            diffs.push(MachineSetupDiff {
                key: "Processors_IntelVirtualizationTechnology".to_string(),
                expected: EnabledDisabled::Enabled.to_string(),
                actual: virt.to_string(),
            });
        }

        let (dhcpv4, http_support) = self.get_uefi_nic_boot().await?;
        if dhcpv4 != EnabledDisabled::Enabled {
            diffs.push(MachineSetupDiff {
                key: "Dhcpv4".to_string(),
                expected: EnabledDisabled::Enabled.to_string(),
                actual: dhcpv4.to_string(),
            });
        }
        if http_support != "Auto" {
            diffs.push(MachineSetupDiff {
                key: "HttpSupport".to_string(),
                expected: "Auto".to_string(),
                actual: http_support,
            });
        }

        Ok(diffs)
    }

    async fn enable_bios_lockdown(&self) -> Result<(), RedfishError> {
        let lockdown_attrs = hpe::BiosLockdownAttributes {
            //            kcs_enabled: None, // todo: this needs to be set to "false" based on the bmc and bios ver
            usb_boot: Disabled,
        };
        let set_lockdown = hpe::SetBiosLockdownAttributes {
            attributes: lockdown_attrs,
        };
        let url = format!("Systems/{}/Bios/settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_lockdown)
            .await
            .map(|_status_code| ())
    }

    async fn enable_bmc_lockdown(&self) -> Result<(), RedfishError> {
        let lockdown_attrs = hpe::OemHpeLockdownAttrs {
            virtual_nic_enabled: false,
        };
        let set_lockdown1 = hpe::OemHpeLockdown {
            hpe: lockdown_attrs,
        };
        let set_lockdown2 = hpe::SetOemHpeLockdown { oem: set_lockdown1 };
        let url = format!("Managers/{}/", self.s.manager_id());
        self.s
            .client
            .patch(&url, set_lockdown2)
            .await
            .map(|_status_code| ())
    }

    async fn enable_bmc_lockdown2(&self) -> Result<(), RedfishError> {
        let netlockdown_attrs = hpe::OemHpeLockdownNetworkProtocolAttrs { kcs_enabled: false };
        let set_netlockdown1 = hpe::OemHpeNetLockdown {
            hpe: netlockdown_attrs,
        };
        let set_netlockdown2 = hpe::SetOemHpeNetLockdown {
            oem: set_netlockdown1,
        };
        let url = format!("Managers/{}/NetworkProtocol", self.s.manager_id());
        self.s
            .client
            .patch(&url, set_netlockdown2)
            .await
            .map(|_status_code| ())
    }

    async fn check_fw_version(&self) -> bool {
        let ilo_manager = self.get_manager().await;
        match ilo_manager {
            Ok(manager) => {
                let fw_parts: Vec<&str> = manager.firmware_version.split_whitespace().collect();
                let fw_major: i32 = fw_parts[1].parse().unwrap_or_default();
                let fw_minor: f32 = fw_parts[2][1..].parse().unwrap_or(0.0);
                fw_major >= 6 && fw_minor >= 1.40
            }
            Err(_) => false,
        }
    }

    async fn enable_lockdown(&self) -> Result<(), RedfishError> {
        if self.check_fw_version().await {
            self.enable_bmc_lockdown2().await?;
        }
        self.enable_bios_lockdown().await?;
        self.enable_bmc_lockdown().await
    }

    async fn disable_bios_lockdown(&self) -> Result<(), RedfishError> {
        let lockdown_attrs = hpe::BiosLockdownAttributes {
            //            kcs_enabled: None, // todo: this needs to be set to "false" based on the bmc and bios ver
            usb_boot: Enabled,
        };
        let set_lockdown = hpe::SetBiosLockdownAttributes {
            attributes: lockdown_attrs,
        };
        let url = format!("Systems/{}/Bios/settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_lockdown)
            .await
            .map(|_status_code| ())
    }

    async fn disable_bmc_lockdown(&self) -> Result<(), RedfishError> {
        let lockdown_attrs = hpe::OemHpeLockdownAttrs {
            virtual_nic_enabled: true,
        };
        let set_lockdown1 = hpe::OemHpeLockdown {
            hpe: lockdown_attrs,
        };
        let set_lockdown2 = hpe::SetOemHpeLockdown { oem: set_lockdown1 };
        let url = format!("Managers/{}/", self.s.manager_id());
        self.s
            .client
            .patch(&url, set_lockdown2)
            .await
            .map(|_status_code| ())
    }

    async fn disable_bmc_lockdown2(&self) -> Result<(), RedfishError> {
        let netlockdown_attrs = hpe::OemHpeLockdownNetworkProtocolAttrs { kcs_enabled: false };
        let set_netlockdown1 = hpe::OemHpeNetLockdown {
            hpe: netlockdown_attrs,
        };
        let set_netlockdown2 = hpe::SetOemHpeNetLockdown {
            oem: set_netlockdown1,
        };
        let url = format!("Managers/{}/NetworkProtocol", self.s.manager_id());
        self.s
            .client
            .patch(&url, set_netlockdown2)
            .await
            .map(|_status_code| ())
    }

    async fn disable_lockdown(&self) -> Result<(), RedfishError> {
        if self.check_fw_version().await {
            self.disable_bmc_lockdown2().await?;
        }
        self.disable_bios_lockdown().await?;
        self.disable_bmc_lockdown().await
    }

    /// Both Intel and AMD have virtualization technologies that help fix the issue of x86 instruction
    /// architecture not being virtualizable.
    /// get_enable_virtualization_key returns the KEY for enabling virtualization in the bios attributes
    /// map that the Lenovo's BMC returns when querying the bios attributes registry. The string returned
    /// will depend on the processors within the given HPE.
    async fn get_enable_virtualization_key(
        &self,
        bios_attributes: &Value,
    ) -> Result<&str, RedfishError> {
        const INTEL_ENABLE_VIRTUALIZATION_KEY: &str = "IntelProcVtd";
        const AMD_ENABLE_VIRTUALIZATION_KEY: &str = "ProcAmdIoVt";

        // Intel specific
        if bios_attributes
            .get(INTEL_ENABLE_VIRTUALIZATION_KEY)
            .is_some()
        {
            Ok(INTEL_ENABLE_VIRTUALIZATION_KEY)
        // AMD specific
        } else if bios_attributes.get(AMD_ENABLE_VIRTUALIZATION_KEY).is_some() {
            Ok(AMD_ENABLE_VIRTUALIZATION_KEY)
        } else {
            Err(RedfishError::MissingKey {
                key: format!(
                    "{}/{}",
                    INTEL_ENABLE_VIRTUALIZATION_KEY, AMD_ENABLE_VIRTUALIZATION_KEY
                )
                .to_string(),
                url: format!("Systems/{}/Bios", self.s.system_id()),
            })
        }
    }

    async fn set_virt_enable(&self) -> Result<(), RedfishError> {
        let bios = self.s.bios_attributes().await?;
        let mut body = HashMap::new();
        let enable_virtualization_key = self.get_enable_virtualization_key(&bios).await?;
        body.insert(
            "Attributes",
            HashMap::from([(enable_virtualization_key, "Enabled")]),
        );
        let url = format!("Systems/{}/Bios/settings", self.s.system_id());
        self.s.client.patch(&url, body).await.map(|_status_code| ())
    }

    async fn get_virt_enabled(&self) -> Result<EnabledDisabled, RedfishError> {
        let bios = self.s.bios_attributes().await?;
        let enable_virtualization_key = self.get_enable_virtualization_key(&bios).await?;
        let Some(val) = bios.get(enable_virtualization_key) else {
            return Err(RedfishError::MissingKey {
                key: enable_virtualization_key.to_string(),
                url: "bios".to_string(),
            });
        };
        let Some(val) = val.as_str() else {
            return Err(RedfishError::InvalidKeyType {
                key: enable_virtualization_key.to_string(),
                expected_type: "str".to_string(),
                url: "bios".to_string(),
            });
        };
        val.parse().map_err(|_e| RedfishError::InvalidKeyType {
            key: enable_virtualization_key.to_string(),
            expected_type: "EnabledDisabled".to_string(),
            url: "bios".to_string(),
        })
    }

    async fn set_uefi_nic_boot(&self) -> Result<(), RedfishError> {
        let uefi_nic_boot = hpe::UefiHttpAttributes {
            dhcpv4: Enabled,
            http_support: "Auto".to_string(),
        };
        let set_uefi_nic_boot = hpe::SetUefiHttpAttributes {
            attributes: uefi_nic_boot,
        };
        let url = format!("Systems/{}/Bios/settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_uefi_nic_boot)
            .await
            .map(|_status_code| ())
    }

    async fn get_uefi_nic_boot(&self) -> Result<(EnabledDisabled, String), RedfishError> {
        let bios = self.s.bios_attributes().await?;

        let dhcpv4 = bios
            .get("Dhcpv4")
            .and_then(|v| v.as_str())
            .ok_or(RedfishError::MissingKey {
                key: "Dhcpv4".to_string(),
                url: "bios".to_string(),
            })?
            .parse()
            .map_err(|_| RedfishError::InvalidKeyType {
                key: "Dhcpv4".to_string(),
                expected_type: "EnabledDisabled".to_string(),
                url: "bios".to_string(),
            })?;

        let http_support = bios
            .get("HttpSupport")
            .and_then(|v| v.as_str())
            .ok_or(RedfishError::MissingKey {
                key: "HttpSupport".to_string(),
                url: "bios".to_string(),
            })?
            .to_string();

        Ok((dhcpv4, http_support))
    }

    async fn change_boot_order(&self, boot_array: Vec<String>) -> Result<(), RedfishError> {
        let new_boot_order = hpe::SetOemHpeBoot {
            persistent_boot_config_order: boot_array,
        };
        let url = format!("Systems/{}/Bios/oem/hpe/boot/settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, new_boot_order)
            .await
            .map(|_status_code| ())
    }

    async fn set_boot_order(&self, name: BootDevices) -> Result<(), RedfishError> {
        let boot_array = match self.get_boot_options_ids_with_first(name).await? {
            None => {
                return Err(RedfishError::MissingBootOption(name.to_string()));
            }
            Some(b) => b,
        };
        self.change_boot_order(boot_array).await
    }

    async fn get_boot_options_ids_with_first(
        &self,
        device: BootDevices,
    ) -> Result<Option<Vec<String>>, RedfishError> {
        let with_name_str = match device {
            BootDevices::Pxe => "nic.",
            BootDevices::UefiHttp => "nic.",
            BootDevices::Hdd => "hd.",
            _ => ".",
        };
        let mut ordered = Vec::new(); // the final boot options
        let url = format!("Systems/{}/Bios/oem/hpe/boot/", self.s.system_id());
        let (_, body): (_, hpe::OemHpeBoot) = self.s.client.get(&url).await?;

        for member in body.persistent_boot_config_order {
            if member.to_ascii_lowercase().contains(with_name_str) {
                ordered.insert(0, member);
                continue;
            }
            ordered.push(member);
        }
        Ok(Some(ordered))
    }

    async fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError> {
        let url = format!("Systems/{}/LogServices/IML/Entries", self.s.system_id());
        let (_status_code, log_entry_collection): (_, LogEntryCollection) =
            self.s.client.get(&url).await?;
        let log_entries = log_entry_collection.members;
        Ok(log_entries)
    }

    async fn bios_serial_console_status(&self) -> Result<Status, RedfishError> {
        let message = String::new();

        let enabled = true;
        let disabled = false;
        /*
        let url = &format!("Systems/{}/Bios", self.s.system_id());
        let (_status_code, bios): (_, hpe::Bios) = self.s.client.get(url).await?;
        let bios = bios.attributes;

        let val = bios.embedded_serial_port;
        message.push_str(&format!("embedded_serial_port={val} "));
        if &val == "Com2Irq3" {
            // enabled
            disabled = false;
        } else {
            // disabled
            enabled = false;
        }

        let val = bios.ems_console;
        message.push_str(&format!("ems_console={val} "));
        if &val == "Virtual" {
            disabled = false;
        } else {
            enabled = false;
        }

        let val = bios.serial_console_baud_rate;
        message.push_str(&format!("serial_console_baud_rate={val} "));
        if &val != "BaudRate115200" {
            enabled = false;
        }

        let val = bios.serial_console_emulation;
        message.push_str(&format!("serial_console_emulation={val} "));
        if &val != "Vt100Plus" {
            enabled = false;
        }

        let val = bios.serial_console_port;
        message.push_str(&format!("serial_console_port={val} "));
        if &val != "Virtual" {
            enabled = false;
        }

        let val = bios.virtual_serial_port;
        message.push_str(&format!("virtual_serial_port={val} "));
        if &val != "Com1Irq4" {
            enabled = false;
        }
        */
        Ok(Status {
            message,
            status: match (enabled, disabled) {
                (true, _) => StatusInternal::Enabled,
                (_, true) => StatusInternal::Disabled,
                _ => StatusInternal::Partial,
            },
        })
    }

    /// Set this option as the first one in BootOrder.
    /// boot_ref should look like e.g. "Boot0028"
    async fn set_first_boot(&self, boot_ref: &str) -> Result<(), RedfishError> {
        let mut order = self.get_system().await?.boot.boot_order;
        let Some(source_pos) = order.iter().position(|bo| bo == boot_ref) else {
            return Err(RedfishError::MissingBootOption(format!(
                "BootOrder does not contain '{boot_ref}'"
            )));
        };
        order.swap(0, source_pos);

        let body = HashMap::from([("Boot", HashMap::from([("BootOrder", order)]))]);
        let url = format!("Systems/{}", self.s.system_id());
        self.s.client.patch(&url, body).await.map(|_status_code| ())
    }

    async fn get_expected_and_actual_first_boot_option(
        &self,
        boot_interface_mac: &str,
    ) -> Result<(Option<String>, Option<String>), RedfishError> {
        let mac = boot_interface_mac.to_string().to_uppercase();

        let all = self.get_boot_options().await?;
        let mut expected_first_boot_option = None;
        for b in all.members {
            let id = b.odata_id_get()?;
            let opt = self.get_boot_option(id).await?;
            let opt_name = opt.display_name.to_uppercase();
            if opt_name.contains("HTTP") && opt_name.contains("IPV4") && opt_name.contains(&mac) {
                expected_first_boot_option = Some(opt.boot_option_reference);
                break;
            }
        }

        let order = self.get_system().await?.boot.boot_order;
        let actual_first_boot_option = order.first().cloned();

        Ok((expected_first_boot_option, actual_first_boot_option))
    }

    // move hpe specific code here
    #[allow(dead_code)]
    pub async fn get_array_controller(
        &self,
        controller_id: u64,
    ) -> Result<storage::ArrayController, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/",
            self.s.system_id(),
            controller_id
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    #[allow(dead_code)]
    pub async fn get_array_controllers(&self) -> Result<storage::ArrayControllers, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/",
            self.s.system_id()
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    /// Query the smart array status from the server
    #[allow(dead_code)]
    pub async fn get_smart_array_status(
        &self,
        controller_id: u64,
    ) -> Result<storage::SmartArray, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/",
            self.s.system_id(),
            controller_id
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    #[allow(dead_code)]
    pub async fn get_logical_drives(
        &self,
        controller_id: u64,
    ) -> Result<storage::LogicalDrives, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/LogicalDrives/",
            self.s.system_id(),
            controller_id
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    #[allow(dead_code)]
    pub async fn get_physical_drive(
        &self,
        drive_id: u64,
        controller_id: u64,
    ) -> Result<storage::DiskDrive, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/DiskDrives/{}/",
            self.s.system_id(),
            controller_id,
            drive_id,
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    #[allow(dead_code)]
    pub async fn get_physical_drives(
        &self,
        controller_id: u64,
    ) -> Result<storage::DiskDrives, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/DiskDrives/",
            self.s.system_id(),
            controller_id
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    #[allow(dead_code)]
    pub async fn get_storage_enclosures(
        &self,
        controller_id: u64,
    ) -> Result<storage::StorageEnclosures, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/StorageEnclosures/",
            self.s.system_id(),
            controller_id
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    #[allow(dead_code)]
    pub async fn get_storage_enclosure(
        &self,
        controller_id: u64,
        enclosure_id: u64,
    ) -> Result<storage::StorageEnclosure, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/StorageEnclosures/{}/",
            self.s.system_id(),
            controller_id,
            enclosure_id,
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }
}
