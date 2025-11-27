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

use reqwest::{header::HeaderMap, Method};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::fs::File;

use crate::{
    model::{
        account_service::ManagerAccount,
        certificate::Certificate,
        chassis::{Assembly, Chassis, NetworkAdapter},
        component_integrity::ComponentIntegrities,
        network_device_function::NetworkDeviceFunction,
        oem::{
            dell::{self, ShareParameters, StorageCollection, SystemConfiguration},
            nvidia_dpu::NicMode,
        },
        power::Power,
        resource::ResourceCollection,
        secure_boot::SecureBoot,
        sel::{LogEntry, LogEntryCollection},
        sensor::GPUSensors,
        service_root::{RedfishVendor, ServiceRoot},
        software_inventory::SoftwareInventory,
        storage::Drives,
        task::Task,
        thermal::Thermal,
        update_service::{ComponentType, TransferProtocolType, UpdateService},
        BootOption, ComputerSystem, InvalidValueError, Manager, OnOff,
    },
    standard::RedfishStandard,
    BiosProfileType, Boot, BootOptions, Collection, EnabledDisabled, JobState, MachineSetupDiff,
    MachineSetupStatus, ODataId, PCIeDevice, PowerState, Redfish, RedfishError, Resource, RoleId,
    Status, StatusInternal, SystemPowerControl,
};

const UEFI_PASSWORD_NAME: &str = "SetupPassword";

const MAX_ACCOUNT_ID: u8 = 16;

pub struct Bmc {
    s: RedfishStandard,
}

#[async_trait::async_trait]
impl Redfish for Bmc {
    async fn create_user(
        &self,
        username: &str,
        password: &str,
        role_id: RoleId,
    ) -> Result<(), RedfishError> {
        // Find an unused ID
        // 'root' is typically ID 2 on an iDrac, and ID 1 might be special
        let mut account_id = 3;
        let mut is_free = false;
        while !is_free && account_id <= MAX_ACCOUNT_ID {
            let a = match self.s.get_account_by_id(&account_id.to_string()).await {
                Ok(a) => a,
                Err(_) => {
                    is_free = true;
                    break;
                }
            };
            if let Some(false) = a.enabled {
                is_free = true;
                break;
            }
            account_id += 1;
        }
        if !is_free {
            return Err(RedfishError::TooManyUsers);
        }

        // Edit that unused account to be ours. That's how iDrac account creation works.
        self.s
            .edit_account(account_id, username, password, role_id, true)
            .await
    }

    async fn delete_user(&self, username: &str) -> Result<(), RedfishError> {
        self.s.delete_user(username).await
    }

    async fn change_username(&self, old_name: &str, new_name: &str) -> Result<(), RedfishError> {
        self.s.change_username(old_name, new_name).await
    }

    async fn change_password(&self, username: &str, new_pass: &str) -> Result<(), RedfishError> {
        self.s.change_password(username, new_pass).await
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
        if action == SystemPowerControl::ACPowercycle {
            let is_lockdown = self.is_lockdown().await?;
            let bios_attrs = self.s.bios_attributes().await?;
            let uefi_var_access = bios_attrs
                .get("UefiVariableAccess")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            if is_lockdown || uefi_var_access == "Controlled" {
                return Err(RedfishError::GenericError {
                    error: "Cannot perform AC power cycle while system is locked down. Disable lockdown, reboot, verify BIOS attribute 'UefiVariableAccess' is 'Standard', and then try again.".to_string(),
                });
            }
            self.perform_ac_power_cycle().await
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

    async fn get_update_service(&self) -> Result<UpdateService, RedfishError> {
        self.s.get_update_service().await
    }

    async fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError> {
        self.get_system_event_log().await
    }

    async fn get_bmc_event_log(
        &self,
        from: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<LogEntry>, RedfishError> {
        // Different Dell timestamp formats (UTC-5, DST, etc..) are making filtering and comparing very difficult
        self.s.get_bmc_event_log(from).await
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
        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset, // requires reboot to apply
        };

        let set_attrs = dell::GenericSetBiosAttrs {
            redfish_settings_apply_time: apply_time,
            attributes: values,
        };

        let url = format!("Systems/{}/Bios/Settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_attrs)
            .await
            .map(|_status_code| ())
    }

    async fn reset_bios(&self) -> Result<(), RedfishError> {
        self.s.factory_reset_bios().await
    }

    async fn get_base_mac_address(&self) -> Result<Option<String>, RedfishError> {
        self.s.get_base_mac_address().await
    }

    async fn machine_setup(
        &self,
        boot_interface_mac: Option<&str>,
        bios_profiles: &HashMap<
            RedfishVendor,
            HashMap<String, HashMap<BiosProfileType, HashMap<String, serde_json::Value>>>,
        >,
        selected_profile: BiosProfileType,
    ) -> Result<(), RedfishError> {
        self.delete_job_queue().await?;

        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset, // requires reboot to apply
        };

        let (nic_slot, has_dpu) = match boot_interface_mac {
            Some(mac) => {
                let slot: String = self.dpu_nic_slot(mac).await?;
                (slot, true)
            }
            // Zero-DPU case
            None => ("".to_string(), false),
        };

        // dell idrac requires applying all bios settings at once.
        let machine_settings = self.machine_setup_attrs(&nic_slot).await?;
        let set_machine_attrs = dell::SetBiosAttrs {
            redfish_settings_apply_time: apply_time,
            attributes: machine_settings,
        };
        // Convert to a more generic HashMap to allow merging with the extra BIOS values
        let as_json =
            serde_json::to_string(&set_machine_attrs).map_err(|e| RedfishError::GenericError {
                error: { e.to_string() },
            })?;
        let mut set_machine_attrs: HashMap<String, serde_json::Value> =
            serde_json::from_str(as_json.as_str()).map_err(|e| RedfishError::GenericError {
                error: { e.to_string() },
            })?;
        if let Some(dell) = bios_profiles.get(&RedfishVendor::Dell) {
            let model = crate::model_coerce(
                self.get_system()
                    .await?
                    .model
                    .unwrap_or("".to_string())
                    .as_str(),
            );
            if let Some(all_extra_values) = dell.get(&model) {
                if let Some(extra_values) = all_extra_values.get(&selected_profile) {
                    tracing::debug!("Setting extra BIOS values: {extra_values:?}");
                    set_machine_attrs.extend(extra_values.clone());
                }
            }
        }

        let url = format!("Systems/{}/Bios/Settings/", self.s.system_id());
        match self.s.client.patch(&url, set_machine_attrs).await? {
            (_, Some(headers)) => self.parse_job_id_from_response_headers(&url, headers).await,
            (_, None) => Err(RedfishError::NoHeader),
        }?;

        self.machine_setup_oem().await?;
        self.setup_bmc_remote_access().await?;

        if has_dpu {
            Ok(())
        } else {
            // Usually a missing DPU is an error, but for zero-dpu it isn't
            // Tell the caller and let them decide
            Err(RedfishError::NoDpu)
        }
    }

    async fn machine_setup_status(
        &self,
        boot_interface_mac: Option<&str>,
    ) -> Result<MachineSetupStatus, RedfishError> {
        // Check BIOS and BMC attributes
        let mut diffs = self.diff_bios_bmc_attr(boot_interface_mac).await?;

        // Check lockdown
        let lockdown = self.lockdown_status().await?;
        if !lockdown.is_fully_enabled() {
            diffs.push(MachineSetupDiff {
                key: "lockdown".to_string(),
                expected: "Enabled".to_string(),
                actual: lockdown.status.to_string(),
            });
        }

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

        Ok(MachineSetupStatus {
            is_done: diffs.is_empty(),
            diffs,
        })
    }

    /// iDRAC does not suport changing password policy. They support IP blocking instead.
    /// https://github.com/dell/iDRAC-Redfish-Scripting/issues/295
    async fn set_machine_password_policy(&self) -> Result<(), RedfishError> {
        // These are all password policy a Dell has, and they are all read only.
        // Redfish will reject attempts to modify them.
        // - AccountLockoutThreshold
        // - AccountLockoutDuration
        // - AccountLockoutCounterResetAfter
        // - AuthFailureLoggingThreshold
        Ok(())
    }

    async fn lockdown(&self, target: EnabledDisabled) -> Result<(), RedfishError> {
        use EnabledDisabled::*;
        // XE9680's can't PXE boot for some reason
        let system = self.s.get_system().await?;
        let entry = match system.model.as_deref() {
            Some("PowerEdge XE9680") => dell::BootDevices::UefiHttp,
            _ => dell::BootDevices::PXE,
        };
        match target {
            Enabled => {
                //self.enable_bios_lockdown().await?;
                self.enable_bmc_lockdown(entry).await
            }
            Disabled => {
                self.disable_bmc_lockdown(entry).await?;
                // BIOS lockdown blocks impi, ensure it's disabled even though we never set it
                self.disable_bios_lockdown().await
            }
        }
    }

    async fn lockdown_status(&self) -> Result<Status, RedfishError> {
        let mut message = String::new();
        let enabled = EnabledDisabled::Enabled.to_string();
        let disabled = EnabledDisabled::Disabled.to_string();

        // BMC lockdown

        let (attrs, url) = self.manager_attributes().await?;

        let key = "Lockdown.1.SystemLockdown";
        let system_lockdown = attrs
            .get(key)
            .ok_or_else(|| RedfishError::MissingKey {
                key: key.to_string(),
                url: url.to_string(),
            })?
            .as_str()
            .ok_or_else(|| RedfishError::InvalidKeyType {
                key: key.to_string(),
                expected_type: "&str".to_string(),
                url: url.to_string(),
            })?;

        let key = "Racadm.1.Enable";
        let racadm = attrs
            .get(key)
            .ok_or_else(|| RedfishError::MissingKey {
                key: key.to_string(),
                url: url.to_string(),
            })?
            .as_str()
            .ok_or_else(|| RedfishError::InvalidKeyType {
                key: key.to_string(),
                expected_type: "&str".to_string(),
                url: url.to_string(),
            })?;

        message.push_str(&format!(
            "BMC: system_lockdown={system_lockdown}, racadm={racadm}."
        ));

        let is_bmc_locked = system_lockdown == enabled && racadm == disabled;
        let is_bmc_unlocked = system_lockdown == disabled && racadm == enabled;

        Ok(Status {
            message,
            status: if is_bmc_locked {
                StatusInternal::Enabled
            } else if is_bmc_unlocked {
                StatusInternal::Disabled
            } else {
                StatusInternal::Partial
            },
        })
    }

    async fn setup_serial_console(&self) -> Result<(), RedfishError> {
        self.delete_job_queue().await?;

        self.setup_bmc_remote_access().await?;

        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset, // requires reboot to apply
        };
        let serial_console = dell::BiosSerialAttrs {
            serial_comm: dell::SerialCommSettings::OnConRedir,
            serial_port_address: dell::SerialPortSettings::Com1,
            ext_serial_connector: dell::SerialPortExtSettings::Serial1,
            fail_safe_baud: "115200".to_string(),
            con_term_type: dell::SerialPortTermSettings::Vt100Vt220,
            redir_after_boot: EnabledDisabled::Enabled,
        };
        let set_serial_attrs = dell::SetBiosSerialAttrs {
            redfish_settings_apply_time: apply_time,
            attributes: serial_console,
        };

        let url = format!("Systems/{}/Bios/Settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_serial_attrs)
            .await
            .map(|_status_code| ())
    }

    async fn serial_console_status(&self) -> Result<Status, RedfishError> {
        let Status {
            status: remote_access_status,
            message: remote_access_message,
        } = self.bmc_remote_access_status().await?;
        let Status {
            status: bios_serial_status,
            message: bios_serial_message,
        } = self.bios_serial_console_status().await?;

        let final_status = {
            use StatusInternal::*;
            match (remote_access_status, bios_serial_status) {
                (Enabled, Enabled) => Enabled,
                (Disabled, Disabled) => Disabled,
                _ => Partial,
            }
        };
        Ok(Status {
            status: final_status,
            message: format!("BMC: {remote_access_message}. BIOS: {bios_serial_message}."),
        })
    }

    async fn get_boot_options(&self) -> Result<BootOptions, RedfishError> {
        self.s.get_boot_options().await
    }

    async fn get_boot_option(&self, option_id: &str) -> Result<BootOption, RedfishError> {
        self.s.get_boot_option(option_id).await
    }

    async fn boot_once(&self, target: Boot) -> Result<(), RedfishError> {
        match target {
            Boot::Pxe => self.set_boot_first(dell::BootDevices::PXE, true).await,
            Boot::HardDisk => self.set_boot_first(dell::BootDevices::HDD, true).await,
            Boot::UefiHttp => Err(RedfishError::NotSupported(
                "No Dell UefiHttp implementation".to_string(),
            )),
        }
    }

    async fn boot_first(&self, target: Boot) -> Result<(), RedfishError> {
        match target {
            Boot::Pxe => self.set_boot_first(dell::BootDevices::PXE, false).await,
            Boot::HardDisk => self.set_boot_first(dell::BootDevices::HDD, false).await,
            Boot::UefiHttp => Err(RedfishError::NotSupported(
                "No Dell UefiHttp implementation".to_string(),
            )),
        }
    }

    async fn clear_tpm(&self) -> Result<(), RedfishError> {
        self.delete_job_queue().await?;

        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset,
        };
        let tpm = dell::BiosTpmAttrs {
            tpm_security: OnOff::On,
            tpm2_hierarchy: dell::Tpm2HierarchySettings::Clear,
        };
        let set_tpm_clear = dell::SetBiosTpmAttrs {
            redfish_settings_apply_time: apply_time,
            attributes: tpm,
        };
        let url = format!("Systems/{}/Bios/Settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_tpm_clear)
            .await
            .map(|_status_code| ())
    }

    async fn pending(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        self.s.pending().await
    }

    async fn clear_pending(&self) -> Result<(), RedfishError> {
        self.delete_job_queue().await
    }

    async fn pcie_devices(&self) -> Result<Vec<PCIeDevice>, RedfishError> {
        self.s.pcie_devices().await
    }

    async fn update_firmware(
        &self,
        firmware: tokio::fs::File,
    ) -> Result<crate::model::task::Task, RedfishError> {
        self.s.update_firmware(firmware).await
    }

    /// update_firmware_multipart returns a string with the task ID
    async fn update_firmware_multipart(
        &self,
        filename: &Path,
        reboot: bool,
        timeout: Duration,
        _component_type: ComponentType,
    ) -> Result<String, RedfishError> {
        let firmware = File::open(&filename)
            .await
            .map_err(|e| RedfishError::FileError(format!("Could not open file: {e}")))?;

        let parameters = serde_json::to_string(&UpdateParameters::new(reboot)).map_err(|e| {
            RedfishError::JsonSerializeError {
                url: "".to_string(),
                object_debug: "".to_string(),
                source: e,
            }
        })?;

        let (_status_code, loc, _body) = self
            .s
            .client
            .req_update_firmware_multipart(
                filename,
                firmware,
                parameters,
                "UpdateService/MultipartUpload",
                false,
                timeout,
            )
            .await?;

        let loc = match loc {
            None => "Unknown".to_string(),
            Some(x) => x,
        };

        // iDRAC returns the full endpoint, we just want the task ID
        Ok(loc.replace("/redfish/v1/TaskService/Tasks/", ""))
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
        let Some(port) = port else {
            return Err(RedfishError::GenericError {
                error: "Port is missing for Dell.".to_string(),
            });
        };
        let url = format!(
            "Chassis/{}/NetworkAdapters/{}/NetworkDeviceFunctions/{}",
            chassis_id, id, port
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    async fn get_network_device_functions(
        &self,
        chassis_id: &str,
    ) -> Result<Vec<String>, RedfishError> {
        self.s.get_network_device_functions(chassis_id).await
    }

    async fn get_chassis_all(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_chassis_all().await
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
        // The uefi password cant be changed if the host is in lockdown
        if self.is_lockdown().await? {
            return Err(RedfishError::Lockdown);
        }

        self.s
            .change_bios_password(UEFI_PASSWORD_NAME, current_uefi_password, new_uefi_password)
            .await?;

        Ok(Some(self.create_bios_config_job().await?))
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
        self.s.bmc_reset_to_defaults().await
    }

    async fn get_job_state(&self, job_id: &str) -> Result<JobState, RedfishError> {
        let url = format!("Managers/iDRAC.Embedded.1/Jobs/{}", job_id);
        let (_status_code, body): (_, HashMap<String, serde_json::Value>) =
            self.s.client.get(&url).await?;
        let job_state_key = "JobState";
        let job_state_value = body
            .get(job_state_key)
            .ok_or_else(|| RedfishError::MissingKey {
                key: job_state_key.to_string(),
                url: url.to_string(),
            })?
            .as_str()
            .ok_or_else(|| RedfishError::InvalidKeyType {
                key: job_state_key.to_string(),
                expected_type: "&str".to_string(),
                url: url.to_string(),
            })?;

        let job_state = match JobState::from_str(job_state_value) {
            JobState::Scheduled => {
                let message_key = "Message";
                let message_value = body
                    .get(message_key)
                    .ok_or_else(|| RedfishError::MissingKey {
                        key: message_key.to_string(),
                        url: url.to_string(),
                    })?
                    .as_str()
                    .ok_or_else(|| RedfishError::InvalidKeyType {
                        key: message_key.to_string(),
                        expected_type: "&str".to_string(),
                        url: url.to_string(),
                    })?;
                match message_value {
                    /* Example JSON response body for a job that is Scheduled but will never complete: the job remains stuck in a Scheduled state indefinitely.
                    {
                        "@odata.context": "/redfish/v1/$metadata#DellJob.DellJob",
                        "@odata.id": "/redfish/v1/Managers/iDRAC.Embedded.1/Oem/Dell/Jobs/JID_510613515077",
                        "@odata.type": "#DellJob.v1_5_0.DellJob",
                        "ActualRunningStartTime": null,
                        "ActualRunningStopTime": null,
                        "CompletionTime": null,
                        "Description": "Job Instance",
                        "EndTime": "TIME_NA",
                        "Id": "JID_510613515077",
                        "JobState": "Scheduled",
                        "JobType": "RAIDConfiguration",
                        "Message": "Job processing initialization failure.",
                        "MessageArgs": [],
                        "MessageArgs@odata.count": 0,
                        "MessageId": "PR30",
                        "Name": "Configure: BOSS.SL.16-1",
                        "PercentComplete": 1,
                        "StartTime": "2025-06-27T16:55:51",
                        "TargetSettingsURI": null
                    }
                    */
                    "Job processing initialization failure." => JobState::ScheduledWithErrors,
                    _ => JobState::Scheduled,
                }
            }
            state => state,
        };

        Ok(job_state)
    }

    async fn get_collection(&self, id: ODataId) -> Result<Collection, RedfishError> {
        self.s.get_collection(id).await
    }

    async fn get_resource(&self, id: ODataId) -> Result<Resource, RedfishError> {
        self.s.get_resource(id).await
    }

    // set_boot_order_dpu_first configures the boot order on the Dell to set the HTTP boot
    // option that corresponds to the primary DPU as the first boot option in the list.
    async fn set_boot_order_dpu_first(
        &self,
        boot_interface_mac: &str,
    ) -> Result<Option<String>, RedfishError> {
        let expected_boot_option_name: String = self
            .get_expected_dpu_boot_option_name(boot_interface_mac)
            .await?;
        let boot_order = self.get_boot_order().await?;
        for (idx, boot_option) in boot_order.iter().enumerate() {
            if boot_option.display_name == expected_boot_option_name {
                if idx == 0 {
                    // Dells will not generate a bios config job below if the boot orders already configured correctly
                    tracing::info!(
                        "NO-OP: DPU ({boot_interface_mac}) will already be the first netboot option ({expected_boot_option_name}) after reboot"
                    );
                    return Ok(None);
                }

                let url = format!("Systems/{}", self.s.system_id());
                let body = HashMap::from([(
                    "Boot",
                    HashMap::from([("BootOrder", vec![boot_option.id.clone()])]),
                )]);

                let job_id = match self.s.client.patch(&url, body).await? {
                    (_, Some(headers)) => {
                        self.parse_job_id_from_response_headers(&url, headers).await
                    }
                    (_, None) => Err(RedfishError::NoHeader),
                }?;
                return Ok(Some(job_id));
            }
        }

        return Err(RedfishError::MissingBootOption(expected_boot_option_name));
    }

    async fn clear_uefi_password(
        &self,
        current_uefi_password: &str,
    ) -> Result<Option<String>, RedfishError> {
        let job_id = self.clear_uefi_password(current_uefi_password).await?;
        Ok(Some(job_id))
    }

    async fn lockdown_bmc(&self, target: crate::EnabledDisabled) -> Result<(), RedfishError> {
        use EnabledDisabled::*;

        // XE9680's can't PXE boot for some reason
        let system = self.s.get_system().await?;
        let entry = match system.model.as_deref() {
            Some("PowerEdge XE9680") => dell::BootDevices::UefiHttp,
            _ => dell::BootDevices::PXE,
        };

        match target {
            Enabled => self.enable_bmc_lockdown(entry).await,
            Disabled => self.disable_bmc_lockdown(entry).await,
        }
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
            HashMap::from([("BootSeqRetry".to_string(), "Enabled".into())]);
        self.set_bios(attrs).await
    }

    async fn is_infinite_boot_enabled(&self) -> Result<Option<bool>, RedfishError> {
        let bios = self.bios().await?;
        let bios_attributes = match bios.get("Attributes") {
            Some(attributes) => attributes,
            None => {
                return Err(RedfishError::MissingKey {
                    key: "Attributes".to_owned(),
                    url: format!("Systems/{}/Bios", self.s.system_id()),
                })
            }
        };

        let infinite_boot_status = bios_attributes
            .get("BootSeqRetry")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RedfishError::MissingKey {
                key: "BootSeqRetry".to_string(),
                url: "Bios attributes".to_string(),
            })?;

        Ok(Some(
            infinite_boot_status == EnabledDisabled::Enabled.to_string(),
        ))
    }

    async fn set_host_rshim(&self, enabled: EnabledDisabled) -> Result<(), RedfishError> {
        self.s.set_host_rshim(enabled).await
    }

    async fn get_host_rshim(&self) -> Result<Option<EnabledDisabled>, RedfishError> {
        self.s.get_host_rshim().await
    }

    async fn set_idrac_lockdown(&self, enabled: EnabledDisabled) -> Result<(), RedfishError> {
        self.set_idrac_lockdown(enabled).await
    }

    async fn get_boss_controller(&self) -> Result<Option<String>, RedfishError> {
        self.get_boss_controller().await
    }

    async fn decommission_storage_controller(
        &self,
        controller_id: &str,
    ) -> Result<Option<String>, RedfishError> {
        Ok(Some(self.decommission_controller(controller_id).await?))
    }

    async fn create_storage_volume(
        &self,
        controller_id: &str,
        volume_name: &str,
        raid_type: &str,
    ) -> Result<Option<String>, RedfishError> {
        let drives = self.get_storage_drives(controller_id).await?;
        Ok(Some(
            self.create_storage_volume(controller_id, volume_name, raid_type, drives)
                .await?,
        ))
    }

    async fn is_boot_order_setup(&self, boot_interface_mac: &str) -> Result<bool, RedfishError> {
        let (expected, actual) = self
            .get_expected_and_actual_first_boot_option(boot_interface_mac)
            .await?;
        Ok(expected.is_some() && expected == actual)
    }

    async fn is_bios_setup(&self, boot_interface_mac: Option<&str>) -> Result<bool, RedfishError> {
        let diffs = self.diff_bios_bmc_attr(boot_interface_mac).await?;
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
    pub fn new(s: RedfishStandard) -> Result<Bmc, RedfishError> {
        Ok(Bmc { s })
    }

    /// Check BIOS and BMC attributes and return differences
    async fn diff_bios_bmc_attr(
        &self,
        boot_interface_mac: Option<&str>,
    ) -> Result<Vec<MachineSetupDiff>, RedfishError> {
        let mut diffs = vec![];

        let bios = self.s.bios_attributes().await?;
        let nic_slot = match boot_interface_mac {
            Some(mac) => self.dpu_nic_slot(mac).await?,
            None => "".to_string(),
        };

        let mut expected_attrs = self.machine_setup_attrs(&nic_slot).await?;

        expected_attrs.tpm2_hierarchy = dell::Tpm2HierarchySettings::Enabled;

        macro_rules! diff {
            ($key:literal, $exp:expr, $act:ty) => {
                let key = $key;
                let exp = $exp;
                let Some(act_v) = bios.get(key) else {
                    return Err(RedfishError::MissingKey {
                        key: key.to_string(),
                        url: "bios".to_string(),
                    });
                };
                let act =
                    <$act>::deserialize(act_v).map_err(|e| RedfishError::JsonDeserializeError {
                        url: "bios".to_string(),
                        body: act_v.to_string(),
                        source: e,
                    })?;
                if exp != act {
                    diffs.push(MachineSetupDiff {
                        key: key.to_string(),
                        expected: exp.to_string(),
                        actual: act.to_string(),
                    });
                }
            };
        }

        diff!(
            "InBandManageabilityInterface",
            expected_attrs.in_band_manageability_interface,
            EnabledDisabled
        );
        diff!(
            "UefiVariableAccess",
            expected_attrs.uefi_variable_access,
            dell::UefiVariableAccessSettings
        );
        diff!(
            "SerialComm",
            expected_attrs.serial_comm,
            dell::SerialCommSettings
        );
        diff!(
            "SerialPortAddress",
            expected_attrs.serial_port_address,
            dell::SerialPortSettings
        );
        diff!("FailSafeBaud", expected_attrs.fail_safe_baud, String);
        diff!(
            "ConTermType",
            expected_attrs.con_term_type,
            dell::SerialPortTermSettings
        );
        diff!(
            "RedirAfterBoot",
            expected_attrs.redir_after_boot,
            EnabledDisabled
        );
        diff!(
            "SriovGlobalEnable",
            expected_attrs.sriov_global_enable,
            EnabledDisabled
        );
        diff!("TpmSecurity", expected_attrs.tpm_security, OnOff);
        diff!(
            "Tpm2Hierarchy",
            expected_attrs.tpm2_hierarchy,
            dell::Tpm2HierarchySettings
        );
        diff!(
            "Tpm2Algorithm",
            expected_attrs.tpm2_algorithm,
            dell::Tpm2Algorithm
        );
        diff!(
            "HttpDev1EnDis",
            expected_attrs.http_device_1_enabled_disabled,
            EnabledDisabled
        );
        diff!(
            "PxeDev1EnDis",
            expected_attrs.pxe_device_1_enabled_disabled,
            EnabledDisabled
        );
        diff!(
            "HttpDev1Interface",
            expected_attrs.http_device_1_interface,
            String
        );

        let manager_attrs = self.manager_dell_oem_attributes().await?;
        let expected = HashMap::from([
            ("WebServer.1.HostHeaderCheck", "Disabled"),
            ("IPMILan.1.Enable", "Enabled"),
            ("OS-BMC.1.AdminState", "Disabled"),
        ]);
        for (key, exp) in expected {
            let Some(act) = manager_attrs.get(key) else {
                return Err(RedfishError::MissingKey {
                    key: key.to_string(),
                    url: "Managers/{manager_id}/Oem/Dell/DellAttributes/{manager_id}".to_string(),
                });
            };
            if act != exp {
                diffs.push(MachineSetupDiff {
                    key: key.to_string(),
                    expected: exp.to_string(),
                    actual: act.to_string(),
                });
            }
        }

        let bmc_remote_access = self.bmc_remote_access_status().await?;
        if !bmc_remote_access.is_fully_enabled() {
            diffs.push(MachineSetupDiff {
                key: "bmc_remote_access".to_string(),
                expected: "Enabled".to_string(),
                actual: bmc_remote_access.status.to_string(),
            });
        }

        Ok(diffs)
    }

    async fn perform_ac_power_cycle(&self) -> Result<(), RedfishError> {
        self.clear_pending().await?;

        // Set PowerCycleRequest in BIOS settings
        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset,
        };

        let mut attributes = HashMap::new();
        attributes.insert(
            "PowerCycleRequest".to_string(),
            serde_json::Value::String("FullPowerCycle".to_string()),
        );

        let set_attrs = dell::GenericSetBiosAttrs {
            redfish_settings_apply_time: apply_time,
            attributes,
        };

        let url = format!("Systems/{}/Bios/Settings", self.s.system_id());
        let result = self.s.client.patch(&url, set_attrs).await;

        // Handle intermittent 400 errors for read-only attributes
        if let Err(RedfishError::HTTPErrorCode {
            status_code,
            response_body,
            ..
        }) = &result
        {
            if status_code.as_u16() == 400 && response_body.contains("read-only") {
                return Err(RedfishError::GenericError {
                    error: "Failed to set PowerCycleRequest BIOS attribute due to read-only dependencies. Please reboot the machine and try again.".to_string(),
                });
            }
        }
        result?;

        // Apply the setting based on current power state
        let current_power_state = self.s.get_power_state().await?;
        match current_power_state {
            PowerState::Off => self.s.power(SystemPowerControl::On).await,
            _ => self.s.power(SystemPowerControl::GracefulRestart).await,
        }
    }

    // No changes can be applied if there are pending jobs
    async fn delete_job_queue(&self) -> Result<(), RedfishError> {
        // The queue can't be cleared if system lockdown is enabled
        if self.is_lockdown().await? {
            return Err(RedfishError::Lockdown);
        }

        let url = format!(
            "Managers/{}/Oem/Dell/DellJobService/Actions/DellJobService.DeleteJobQueue",
            self.s.manager_id()
        );
        let mut body = HashMap::new();
        body.insert("JobID", "JID_CLEARALL".to_string());
        self.s.client.post(&url, body).await.map(|_resp| ())
    }

    // Is system lockdown enabled?
    async fn is_lockdown(&self) -> Result<bool, RedfishError> {
        let (attrs, url) = self.manager_attributes().await?;

        let key = "Lockdown.1.SystemLockdown";
        let system_lockdown = attrs
            .get(key)
            .ok_or_else(|| RedfishError::MissingKey {
                key: key.to_string(),
                url: url.to_string(),
            })?
            .as_str()
            .ok_or_else(|| RedfishError::InvalidKeyType {
                key: key.to_string(),
                expected_type: "&str".to_string(),
                url: url.to_string(),
            })?;

        let enabled = EnabledDisabled::Enabled.to_string();
        Ok(system_lockdown == enabled)
    }

    async fn set_boot_first(
        &self,
        entry: dell::BootDevices,
        once: bool,
    ) -> Result<(), RedfishError> {
        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset,
        };
        let boot_entry = dell::ServerBoot {
            first_boot_device: entry,
            boot_once: if once {
                EnabledDisabled::Enabled
            } else {
                EnabledDisabled::Disabled
            },
        };
        let boot = dell::ServerBootAttrs {
            server_boot: boot_entry,
        };
        let set_boot = dell::SetFirstBootDevice {
            redfish_settings_apply_time: apply_time,
            attributes: boot,
        };
        let url = format!("Managers/{}/Attributes", self.s.manager_id());
        self.s
            .client
            .patch(&url, set_boot)
            .await
            .map(|_status_code| ())
    }

    async fn set_idrac_lockdown(&self, enabled: EnabledDisabled) -> Result<(), RedfishError> {
        let manager_id: &str = self.s.manager_id();
        let url = format!("Managers/{manager_id}/Attributes/");

        let mut lockdown = HashMap::new();
        lockdown.insert("Lockdown.1.SystemLockdown", enabled.to_string());

        let mut attributes = HashMap::new();
        attributes.insert("Attributes", lockdown);

        self.s
            .client
            .patch(&url, attributes)
            .await
            .map(|_status_code| ())
    }

    async fn enable_bmc_lockdown(&self, entry: dell::BootDevices) -> Result<(), RedfishError> {
        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset,
        };

        // First change all settings except lockdown, because that applies immediately
        // and prevents the other settings being applied.
        let boot_entry = dell::ServerBoot {
            first_boot_device: entry,
            boot_once: EnabledDisabled::Disabled,
        };
        let lockdown = dell::BmcLockdown {
            system_lockdown: None,
            racadm_enable: Some(EnabledDisabled::Disabled),
            server_boot: Some(boot_entry),
        };
        let set_bmc_lockdown = dell::SetBmcLockdown {
            redfish_settings_apply_time: apply_time,
            attributes: lockdown,
        };
        let manager_id = self.s.manager_id();
        let url = format!("Managers/{manager_id}/Oem/Dell/DellAttributes/{manager_id}");
        self.s
            .client
            .patch(&url, set_bmc_lockdown)
            .await
            .map(|_status_code| ())?;

        // Now lockdown
        let lockdown = dell::BmcLockdown {
            system_lockdown: Some(EnabledDisabled::Enabled),
            racadm_enable: None,
            server_boot: None,
        };
        let set_bmc_lockdown = dell::SetBmcLockdown {
            redfish_settings_apply_time: apply_time,
            attributes: lockdown,
        };
        self.s
            .client
            .patch(&url, set_bmc_lockdown)
            .await
            .map(|_status_code| ())
    }

    async fn disable_bios_lockdown(&self) -> Result<(), RedfishError> {
        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset, // requires reboot to apply
        };
        let lockdown = dell::BiosLockdownAttrs {
            in_band_manageability_interface: EnabledDisabled::Enabled,
            uefi_variable_access: dell::UefiVariableAccessSettings::Standard,
        };
        let set_lockdown_attrs = dell::SetBiosLockdownAttrs {
            redfish_settings_apply_time: apply_time,
            attributes: lockdown,
        };
        let url = format!("Systems/{}/Bios/Settings/", self.s.system_id());
        // Sometimes, these settings are read only.  Ignore those errors trying to set them.
        let ret = self
            .s
            .client
            .patch(&url, set_lockdown_attrs)
            .await
            .map(|_status_code| ());
        if let Err(RedfishError::HTTPErrorCode {
            url: _,
            status_code,
            response_body,
        }) = &ret
        {
            if status_code.as_u16() == 400 && response_body.contains("read-only") {
                return Ok(());
            }
        }
        ret
    }

    async fn disable_bmc_lockdown(&self, entry: dell::BootDevices) -> Result<(), RedfishError> {
        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::Immediate, // bmc settings don't require reboot
        };
        let boot_entry = dell::ServerBoot {
            first_boot_device: entry,
            boot_once: EnabledDisabled::Disabled,
        };
        let lockdown = dell::BmcLockdown {
            system_lockdown: Some(EnabledDisabled::Disabled),
            racadm_enable: Some(EnabledDisabled::Enabled),
            server_boot: Some(boot_entry),
        };
        let set_bmc_lockdown = dell::SetBmcLockdown {
            redfish_settings_apply_time: apply_time,
            attributes: lockdown,
        };
        let manager_id = self.s.manager_id();
        let url = format!("Managers/{manager_id}/Oem/Dell/DellAttributes/{manager_id}");
        self.s
            .client
            .patch(&url, set_bmc_lockdown)
            .await
            .map(|_status_code| ())
    }

    async fn setup_bmc_remote_access(&self) -> Result<(), RedfishError> {
        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::Immediate,
        };
        let serial_redirect = dell::SerialRedirection {
            enable: EnabledDisabled::Enabled,
        };
        let ipmi_sol_settings = dell::IpmiSol {
            enable: EnabledDisabled::Enabled,
            baud_rate: "115200".to_string(),
            min_privilege: "Administrator".to_string(),
        };
        let remote_access = dell::BmcRemoteAccess {
            ssh_enable: EnabledDisabled::Enabled,
            serial_redirection: serial_redirect,
            ipmi_lan_enable: EnabledDisabled::Enabled,
            ipmi_sol: ipmi_sol_settings,
        };
        let set_remote_access = dell::SetBmcRemoteAccess {
            redfish_settings_apply_time: apply_time,
            attributes: remote_access,
        };
        let url = format!("Managers/{}/Attributes", self.s.manager_id());
        self.s
            .client
            .patch(&url, set_remote_access)
            .await
            .map(|_status_code| ())
    }

    async fn bmc_remote_access_status(&self) -> Result<Status, RedfishError> {
        let (attrs, _) = self.manager_attributes().await?;
        let expected = vec![
            // "any" means any value counts as correctly disabled
            ("SerialRedirection.1.Enable", "Enabled", "Disabled"),
            ("IPMISOL.1.BaudRate", "115200", "any"),
            ("IPMISOL.1.Enable", "Enabled", "Disabled"),
            ("IPMISOL.1.MinPrivilege", "Administrator", "any"),
            ("SSH.1.Enable", "Enabled", "Disabled"),
            ("IPMILan.1.Enable", "Enabled", "Disabled"),
        ];

        // url is for error messages only
        let manager_id = self.s.manager_id();
        let url = &format!("Managers/{manager_id}/Oem/Dell/DellAttributes/{manager_id}");

        let mut message = String::new();
        let mut enabled = true;
        let mut disabled = true;
        for (key, val_enabled, val_disabled) in expected {
            let val_current = attrs
                .get(key)
                .ok_or_else(|| RedfishError::MissingKey {
                    key: key.to_string(),
                    url: url.to_string(),
                })?
                .as_str()
                .ok_or_else(|| RedfishError::InvalidKeyType {
                    key: key.to_string(),
                    expected_type: "&str".to_string(),
                    url: url.to_string(),
                })?;
            message.push_str(&format!("{key}={val_current} "));
            if val_current != val_enabled {
                enabled = false;
            }
            if val_current != val_disabled && val_disabled != "any" {
                disabled = false;
            }
        }

        Ok(Status {
            message,
            status: match (enabled, disabled) {
                (true, _) => StatusInternal::Enabled,
                (_, true) => StatusInternal::Disabled,
                _ => StatusInternal::Partial,
            },
        })
    }

    async fn bios_serial_console_status(&self) -> Result<Status, RedfishError> {
        let mut message = String::new();

        // Start with true, then check every value to see whether it means things are not setup
        // correctly, and set the value to false.
        // Note that there are three results: Enabled, Disabled, and Partial, so enabled and
        // disabled can both be false by the end. They cannot both be true.
        let mut enabled = true;
        let mut disabled = true;

        let url = &format!("Systems/{}/Bios", self.s.system_id());
        let (_status_code, bios): (_, dell::Bios) = self.s.client.get(url).await?;
        let bios = bios.attributes;

        let val = bios.serial_comm;
        message.push_str(&format!(
            "serial_comm={} ",
            val.as_ref().unwrap_or(&"unknown".to_string())
        ));
        if let Some(x) = &val {
            match x.parse().map_err(|err| RedfishError::InvalidValue {
                err,
                url: url.to_string(),
                field: "serial_comm".to_string(),
            })? {
                dell::SerialCommSettings::OnConRedir | dell::SerialCommSettings::OnConRedirAuto => {
                    // enabled
                    disabled = false;
                }
                dell::SerialCommSettings::Off => {
                    // disabled
                    enabled = false;
                }
                _ => {
                    // someone messed with it manually
                    enabled = false;
                    disabled = false;
                }
            }
        }

        let val = bios.redir_after_boot;
        message.push_str(&format!(
            "redir_after_boot={} ",
            val.as_ref().unwrap_or(&"unknown".to_string())
        ));
        if let Some(x) = &val {
            match x.parse().map_err(|err| RedfishError::InvalidValue {
                err,
                url: url.to_string(),
                field: "redir_after_boot".to_string(),
            })? {
                EnabledDisabled::Enabled => {
                    disabled = false;
                }
                EnabledDisabled::Disabled => {
                    enabled = false;
                }
            }
        }

        // All of these need a specific value for serial console access to work.
        // Any other value counts as correctly disabled.

        let val = bios.serial_port_address;
        message.push_str(&format!(
            "serial_port_address={} ",
            val.as_ref().unwrap_or(&"unknown".to_string())
        ));
        if let Some(x) = &val {
            if *x != dell::SerialPortSettings::Com1.to_string() {
                enabled = false;
            }
        }

        let val = bios.ext_serial_connector;
        message.push_str(&format!(
            "ext_serial_connector={} ",
            val.as_ref().unwrap_or(&"unknown".to_string())
        ));
        if let Some(x) = &val {
            if *x != dell::SerialPortExtSettings::Serial1.to_string() {
                enabled = false;
            }
        }

        let val = bios.fail_safe_baud;
        message.push_str(&format!(
            "fail_safe_baud={} ",
            val.as_ref().unwrap_or(&"unknown".to_string())
        ));
        if let Some(x) = &val {
            if x != "115200" {
                enabled = false;
            }
        }

        let val = bios.con_term_type;
        message.push_str(&format!(
            "con_term_type={} ",
            val.as_ref().unwrap_or(&"unknown".to_string())
        ));
        if let Some(x) = &val {
            if *x != dell::SerialPortTermSettings::Vt100Vt220.to_string() {
                enabled = false;
            }
        }

        Ok(Status {
            message,
            status: match (enabled, disabled) {
                (true, _) => StatusInternal::Enabled,
                (_, true) => StatusInternal::Disabled,
                _ => StatusInternal::Partial,
            },
        })
    }

    // dell stores the sel as part of the manager
    async fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError> {
        let manager_id = self.s.manager_id();
        let url = format!("Managers/{manager_id}/LogServices/Sel/Entries");
        let (_status_code, log_entry_collection): (_, LogEntryCollection) =
            self.s.client.get(&url).await?;
        let log_entries = log_entry_collection.members;
        Ok(log_entries)
    }

    // Second value in tuple is URL we used to fetch attributes, for diagnostics
    async fn manager_attributes(
        &self,
    ) -> Result<(serde_json::Map<String, serde_json::Value>, String), RedfishError> {
        let manager_id = self.s.manager_id();
        let url = &format!("Managers/{manager_id}/Oem/Dell/DellAttributes/{manager_id}");
        let (_status_code, mut body): (_, HashMap<String, serde_json::Value>) =
            self.s.client.get(url).await?;
        let key = "Attributes";
        let v = match body.remove(key).ok_or_else(|| RedfishError::MissingKey {
            key: key.to_string(),
            url: url.to_string(),
        })? {
            serde_json::Value::Object(obj) => obj,
            _ => {
                return Err(RedfishError::InvalidKeyType {
                    key: key.to_string(),
                    expected_type: "Object".to_string(),
                    url: url.to_string(),
                });
            }
        };
        Ok((v, url.to_string()))
    }

    /// Extra Dell-specific attributes we need to set that are not BIOS attributes
    async fn machine_setup_oem(&self) -> Result<(), RedfishError> {
        let manager_id = self.s.manager_id();
        let url = format!("Managers/{manager_id}/Oem/Dell/DellAttributes/{manager_id}");

        let mut attributes = HashMap::new();
        // racadm set idrac.webserver.HostHeaderCheck 0
        attributes.insert("WebServer.1.HostHeaderCheck", "Disabled".to_string());
        // racadm set iDRAC.IPMILan.Enable 1
        attributes.insert("IPMILan.1.Enable", "Enabled".to_string());
        attributes.insert("OS-BMC.1.AdminState", "Disabled".to_string());

        let body = HashMap::from([("Attributes", attributes)]);
        self.s.client.patch(&url, body).await?;
        Ok(())
    }

    async fn manager_dell_oem_attributes(&self) -> Result<serde_json::Value, RedfishError> {
        let manager_id = self.s.manager_id();
        let url = format!("Managers/{manager_id}/Oem/Dell/DellAttributes/{manager_id}");
        let (_status_code, mut body): (_, HashMap<String, serde_json::Value>) =
            self.s.client.get(&url).await?;
        body.remove("Attributes")
            .ok_or_else(|| RedfishError::MissingKey {
                key: "Attributes".to_string(),
                url,
            })
    }

    // TPM is enabled by default so we never call this.
    #[allow(dead_code)]
    async fn enable_tpm(&self) -> Result<(), RedfishError> {
        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset, // requires reboot to apply
        };
        let tpm = dell::BiosTpmAttrs {
            tpm_security: OnOff::On,
            tpm2_hierarchy: dell::Tpm2HierarchySettings::Enabled,
        };
        let set_tpm_enabled = dell::SetBiosTpmAttrs {
            redfish_settings_apply_time: apply_time,
            attributes: tpm,
        };
        let url = format!("Systems/{}/Bios/Settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_tpm_enabled)
            .await
            .map(|_status_code| ())
    }

    // Dell supports disabling the TPM. Why would we do this?
    // Lenovo does not support disabling TPM2.0
    #[allow(dead_code)]
    async fn disable_tpm(&self) -> Result<(), RedfishError> {
        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset, // requires reboot to apply
        };
        let tpm = dell::BiosTpmAttrs {
            tpm_security: OnOff::Off,
            tpm2_hierarchy: dell::Tpm2HierarchySettings::Disabled,
        };
        let set_tpm_disabled = dell::SetBiosTpmAttrs {
            redfish_settings_apply_time: apply_time,
            attributes: tpm,
        };
        let url = format!("Systems/{}/Bios/Settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_tpm_disabled)
            .await
            .map(|_status_code| ())
    }

    pub async fn create_bios_config_job(&self) -> Result<String, RedfishError> {
        let url = "Managers/iDRAC.Embedded.1/Jobs";

        let mut arg = HashMap::new();
        arg.insert(
            "TargetSettingsURI",
            "/redfish/v1/Systems/System.Embedded.1/Bios/Settings".to_string(),
        );

        match self.s.client.post(url, arg).await? {
            (_, Some(headers)) => self.parse_job_id_from_response_headers(url, headers).await,
            (_, None) => Err(RedfishError::NoHeader),
        }
    }

    async fn machine_setup_attrs(
        &self,
        nic_slot: &str,
    ) -> Result<dell::MachineBiosAttrs, RedfishError> {
        let curr_bios_attributes = self.s.bios_attributes().await?;
        let curr_enabled_boot_options = match curr_bios_attributes.get("SetBootOrderEn") {
            Some(enabled_boot_options) => enabled_boot_options.as_str().unwrap_or_default(),
            None => {
                return Err(RedfishError::MissingKey {
                    key: "SetBootOrderEn".to_owned(),
                    url: format!("Systems/{}/Bios", self.s.system_id()),
                });
            }
        };

        // We want to disable all boot options other than HTTP Device 1.
        let boot_options_to_disable_arr: Vec<&str> = curr_enabled_boot_options
            .split(",")
            .filter(|boot_option| *boot_option != "NIC.HttpDevice.1-1")
            .collect();
        let boot_options_to_disable_str = boot_options_to_disable_arr.join(",");

        Ok(dell::MachineBiosAttrs {
            in_band_manageability_interface: EnabledDisabled::Disabled,
            uefi_variable_access: dell::UefiVariableAccessSettings::Standard,
            serial_comm: dell::SerialCommSettings::OnConRedir,
            serial_port_address: dell::SerialPortSettings::Com1,
            fail_safe_baud: "115200".to_string(),
            con_term_type: dell::SerialPortTermSettings::Vt100Vt220,
            redir_after_boot: EnabledDisabled::Enabled,
            sriov_global_enable: EnabledDisabled::Enabled,
            tpm_security: OnOff::On,
            tpm2_hierarchy: dell::Tpm2HierarchySettings::Clear,
            tpm2_algorithm: dell::Tpm2Algorithm::SHA256,
            http_device_1_enabled_disabled: EnabledDisabled::Enabled,
            pxe_device_1_enabled_disabled: EnabledDisabled::Disabled,
            boot_mode: "Uefi".to_string(),
            http_device_1_interface: nic_slot.to_string(),
            set_boot_order_en: nic_slot.to_string(),
            http_device_1_tls_mode: dell::TlsMode::None,
            set_boot_order_dis: boot_options_to_disable_str,
        })
    }

    /// Dells endpoint to change the UEFI password has a bug for updating it once it is set.
    /// Use the ImportSystemConfiguration endpoint as a hack to clear the UEFI password instead.
    /// Detailed here: https://github.com/dell/iDRAC-Redfish-Scripting/issues/308
    async fn clear_uefi_password(
        &self,
        current_uefi_password: &str,
    ) -> Result<String, RedfishError> {
        let system_configuration = SystemConfiguration {
            shutdown_type: "Forced".to_string(),
            share_parameters: ShareParameters {
                target: "BIOS".to_string(),
            },
            import_buffer: format!(
                r##"<SystemConfiguration><Component FQDD="BIOS.Setup.1-1"><!-- <Attribute Name="OldSysPassword"></Attribute>--><!-- <Attribute Name="NewSysPassword"></Attribute>--><Attribute Name="OldSetupPassword">{current_uefi_password}</Attribute><Attribute Name="NewSetupPassword"></Attribute></Component></SystemConfiguration>"##
            ),
        };

        self.import_system_configuration(system_configuration).await
    }

    async fn parse_job_id_from_response_headers(
        &self,
        url: &str,
        resp_headers: HeaderMap,
    ) -> Result<String, RedfishError> {
        let key = "location";
        Ok(resp_headers
            .get(key)
            .ok_or_else(|| RedfishError::MissingKey {
                key: key.to_string(),
                url: url.to_string(),
            })?
            .to_str()
            .map_err(|e| RedfishError::InvalidValue {
                url: url.to_string(),
                field: key.to_string(),
                err: InvalidValueError(e.to_string()),
            })?
            .split('/')
            .next_back()
            .ok_or_else(|| RedfishError::InvalidValue {
                url: url.to_string(),
                field: key.to_string(),
                err: InvalidValueError("unable to parse job_id from location string".to_string()),
            })?
            .to_string())
    }

    /// import_system_configuration returns the job ID for importing this sytem configuration
    async fn import_system_configuration(
        &self,
        system_configuration: SystemConfiguration,
    ) -> Result<String, RedfishError> {
        let url = "Managers/iDRAC.Embedded.1/Actions/Oem/EID_674_Manager.ImportSystemConfiguration";
        let (_status_code, _resp_body, resp_headers): (
            _,
            Option<HashMap<String, serde_json::Value>>,
            Option<HeaderMap>,
        ) = self
            .s
            .client
            .req(
                Method::POST,
                url,
                Some(system_configuration),
                None,
                None,
                Vec::new(),
            )
            .await?;

        match resp_headers {
            Some(headers) => self.parse_job_id_from_response_headers(url, headers).await,
            None => Err(RedfishError::NoHeader),
        }
    }

    async fn get_dpu_nw_device_function(
        &self,
        boot_interface_mac_address: &str,
    ) -> Result<NetworkDeviceFunction, RedfishError> {
        let chassis = self.get_chassis(self.s.system_id()).await?;
        let na_id = match chassis.network_adapters {
            Some(id) => id,
            None => {
                let chassis_odata_url = chassis
                    .odata
                    .map(|o| o.odata_id)
                    .unwrap_or_else(|| "empty_odata_id_url".to_string());
                return Err(RedfishError::MissingKey {
                    key: "network_adapters".to_string(),
                    url: chassis_odata_url,
                });
            }
        };

        let rc_nw_adapter: ResourceCollection<NetworkAdapter> = self
            .s
            .get_collection(na_id)
            .await
            .and_then(|r| r.try_get())?;

        // Get nw_device_functions
        for nw_adapter in rc_nw_adapter.members {
            let nw_dev_func_oid = match nw_adapter.network_device_functions {
                Some(x) => x,
                None => {
                    // TODO debug
                    continue;
                }
            };

            let rc_nw_func: ResourceCollection<NetworkDeviceFunction> = self
                .get_collection(nw_dev_func_oid)
                .await
                .and_then(|r| r.try_get())?;

            for nw_dev_func in rc_nw_func.members {
                if let Some(ref ethernet_info) = nw_dev_func.ethernet {
                    if let Some(ref mac) = ethernet_info.mac_address {
                        let standardized_mac = mac.to_lowercase();
                        if standardized_mac == boot_interface_mac_address.to_lowercase() {
                            return Ok(nw_dev_func);
                        }
                    }
                }
            }
        }

        Err(RedfishError::GenericError {
            error: format!(
                "could not find network device function for {boot_interface_mac_address}"
            ),
        })
    }

    async fn get_dell_nic_info(
        &self,
        mac_address: &str,
    ) -> Result<serde_json::Map<String, Value>, RedfishError> {
        let nw_device_function = self.get_dpu_nw_device_function(mac_address).await?;

        let oem = nw_device_function
            .oem
            .ok_or_else(|| RedfishError::GenericError {
                error: "OEM information is missing".to_string(),
            })?;

        let oem_dell = oem.get("Dell").ok_or_else(|| RedfishError::GenericError {
            error: "Dell OEM information is missing".to_string(),
        })?;

        let oem_dell_map = oem_dell
            .as_object()
            .ok_or_else(|| RedfishError::GenericError {
                error: "Dell OEM information is not a valid object".to_string(),
            })?;

        let dell_nic_map = oem_dell_map
            .get("DellNIC")
            .and_then(|dell_nic| dell_nic.as_object())
            .ok_or_else(|| RedfishError::GenericError {
                error: "DellNIC information is not a valid object or is missing".to_string(),
            })?;

        Ok(dell_nic_map.to_owned())
    }

    // Returns a string like "NIC.Slot.5-1"
    async fn dpu_nic_slot(&self, mac_address: &str) -> Result<String, RedfishError> {
        let dell_nic_info = self.get_dell_nic_info(mac_address).await?;

        let nic_slot = dell_nic_info
            .get("Id")
            .and_then(|id| id.as_str())
            .ok_or_else(|| RedfishError::GenericError {
                error: "NIC slot ID is missing or not a valid string".to_string(),
            })?
            .to_string();

        Ok(nic_slot)
    }

    async fn get_boss_controller(&self) -> Result<Option<String>, RedfishError> {
        let url = "Systems/System.Embedded.1/Storage";
        let (_status_code, storage_collection): (_, StorageCollection) =
            self.s.client.get(url).await?;
        for controller in storage_collection.members {
            if controller.odata_id.contains("BOSS") {
                let boss_controller_id =
                    controller.odata_id.split('/').next_back().ok_or_else(|| {
                        RedfishError::InvalidValue {
                            url: url.to_string(),
                            field: "odata_id".to_string(),
                            err: InvalidValueError(format!(
                                "unable to parse boss_controller_id from {}",
                                controller.odata_id
                            )),
                        }
                    })?;
                return Ok(Some(boss_controller_id.to_string()));
            }
        }

        Ok(None)
    }

    async fn decommission_controller(&self, controller_id: &str) -> Result<String, RedfishError> {
        // wait for the lifecycle controller status to become Ready before decomissioning the boss controller
        // https://github.com/dell/idrac-Redfish-Scripting/issues/323
        self.lifecycle_controller_is_ready().await?;

        let url: String = format!("Systems/System.Embedded.1/Storage/{controller_id}/Actions/Oem/DellStorage.ControllerDrivesDecommission");
        let mut arg = HashMap::new();
        arg.insert("@Redfish.OperationApplyTime", "Immediate");

        match self.s.client.post(&url, arg).await? {
            (_, Some(headers)) => self.parse_job_id_from_response_headers(&url, headers).await,
            (_, None) => Err(RedfishError::NoHeader),
        }
    }

    async fn get_storage_drives(&self, controller_id: &str) -> Result<Value, RedfishError> {
        let url = format!("Systems/System.Embedded.1/Storage/{controller_id}");
        let (_status_code, body): (_, HashMap<String, serde_json::Value>) =
            self.s.client.get(&url).await?;

        let key = "Drives";
        body.get(key)
            .ok_or_else(|| RedfishError::MissingKey {
                key: key.to_string(),
                url: url.to_string(),
            })
            .cloned()
    }

    async fn create_storage_volume(
        &self,
        controller_id: &str,
        volume_name: &str,
        raid_type: &str,
        drive_info: Value,
    ) -> Result<String, RedfishError> {
        if volume_name.len() > 15 || volume_name.is_empty() {
            return Err(RedfishError::GenericError {
                error: format!(
                    "invalid volume name ({volume_name}); must be between 1 and 15 characters long"
                ),
            });
        }

        // wait for the lifecycle controller status to become Ready
        self.lifecycle_controller_is_ready().await?;

        let url: String = format!("Systems/System.Embedded.1/Storage/{controller_id}/Volumes");
        let mut arg = HashMap::new();

        arg.insert("Name", Value::String(volume_name.to_string()));
        arg.insert("RAIDType", Value::String(raid_type.to_string()));
        arg.insert("Drives", drive_info);

        match self.s.client.post(&url, arg).await? {
            (_, Some(headers)) => self.parse_job_id_from_response_headers(&url, headers).await,
            (_, None) => Err(RedfishError::NoHeader),
        }
    }

    async fn get_lifecycle_controller_status(&self) -> Result<String, RedfishError> {
        let url = format!(
            "Dell/Managers/{}/DellLCService/Actions/DellLCService.GetRemoteServicesAPIStatus",
            self.s.manager_id()
        );
        let arg: HashMap<&'static str, Value> = HashMap::new();
        let (_status_code, resp_body, _resp_headers): (
            _,
            Option<HashMap<String, serde_json::Value>>,
            Option<HeaderMap>,
        ) = self
            .s
            .client
            .req(Method::POST, &url, Some(arg), None, None, Vec::new())
            .await?;

        let lc_status = match resp_body.unwrap_or_default().get("LCStatus") {
            Some(status) => status.as_str().unwrap_or_default().to_string(),
            None => todo!(),
        };

        Ok(lc_status)
    }

    async fn lifecycle_controller_is_ready(&self) -> Result<(), RedfishError> {
        let lc_status = self.get_lifecycle_controller_status().await?;
        if lc_status == "Ready" {
            return Ok(());
        }

        Err(RedfishError::GenericError { error: format!("the lifecycle controller is not ready to accept provisioning requests; lc_status: {lc_status}") })
    }

    // get_expected_dpu_boot_option_name assumes that assumes that the HTTP Device One boot option has been enabled
    // and points to the NIC for the boot interface MAC address. In the future, we can relax the string matching if
    // we configure other HTTP devices and just match on the NIC's device description.
    async fn get_expected_dpu_boot_option_name(
        &self,
        boot_interface_mac: &str,
    ) -> Result<String, RedfishError> {
        let dell_nic_info = self.get_dell_nic_info(boot_interface_mac).await?;

        let device_description = dell_nic_info
            .get("DeviceDescription")
            .and_then(|device_description| device_description.as_str())
            .ok_or_else(|| RedfishError::GenericError {
                error: format!("the NIC Device Description for {boot_interface_mac} is missing or not a valid string").to_string(),
            })?
            .to_string();

        Ok(format!("HTTP Device 1: {device_description}",))
    }

    async fn get_boot_order(&self) -> Result<Vec<BootOption>, RedfishError> {
        let boot_options = self.get_boot_options().await?;
        let mut boot_order: Vec<BootOption> = Vec::new();
        for boot_option in boot_options.members.iter() {
            let id = boot_option.odata_id_get()?;
            let boot_option = self.get_boot_option(id).await?;
            boot_order.push(boot_option)
        }

        Ok(boot_order)
    }

    // get_expected_and_actual_first_boot_option assumes that the HTTP Device One boot option has been enabled
    // and points to the NIC for the boot interface MAC address. In the future, we can relax the string matching if
    // we configure other HTTP devices and just match on the NIC's device description.
    async fn get_expected_and_actual_first_boot_option(
        &self,
        boot_interface_mac: &str,
    ) -> Result<(Option<String>, Option<String>), RedfishError> {
        let expected_first_boot_option = Some(
            self.get_expected_dpu_boot_option_name(boot_interface_mac)
                .await?,
        );
        let boot_order = self.get_boot_order().await?;
        let actual_first_boot_option = boot_order.first().map(|opt| opt.display_name.clone());

        Ok((expected_first_boot_option, actual_first_boot_option))
    }
}

// UpdateParameters is what is sent for a multipart firmware upload's metadata.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct UpdateParameters {
    targets: Vec<String>,
    #[serde(rename = "@Redfish.OperationApplyTime")]
    pub apply_time: String,
    oem: Empty,
}

// The BMC expects to have a {} in its JSON, even though it doesn't seem to do anything with it.  Their implementation must be... interesting.
#[derive(Serialize)]
struct Empty {}

impl UpdateParameters {
    pub fn new(reboot_immediate: bool) -> UpdateParameters {
        let apply_time = match reboot_immediate {
            true => "Immediate",
            false => "OnReset",
        }
        .to_string();
        UpdateParameters {
            targets: vec![],
            apply_time,
            oem: Empty {},
        }
    }
}
