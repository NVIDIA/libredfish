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
use std::{collections::HashMap, path::Path, time::Duration};

use reqwest::{header::HeaderMap, Method};
use serde::{Deserialize, Serialize};
use tokio::fs::File;

use crate::{
    model::{
        account_service::ManagerAccount,
        chassis::{Chassis, NetworkAdapter},
        network_device_function::NetworkDeviceFunction,
        oem::dell::{self, ShareParameters, SystemConfiguration},
        power::Power,
        resource::ResourceCollection,
        secure_boot::SecureBoot,
        sel::{LogEntry, LogEntryCollection},
        sensor::GPUSensors,
        service_root::ServiceRoot,
        software_inventory::SoftwareInventory,
        task::Task,
        thermal::Thermal,
        update_service::{ComponentType, TransferProtocolType, UpdateService},
        BootOption, ComputerSystem, InvalidValueError, Manager, OnOff,
    },
    standard::RedfishStandard,
    Boot, BootOptions, Collection, EnabledDisabled, MachineSetupDiff, MachineSetupStatus, JobState,
    ODataId, PCIeDevice, PowerState, Redfish, RedfishError, Resource, RoleId, Status,
    StatusInternal, SystemPowerControl,
};

const UEFI_PASSWORD_NAME: &str = "SetupPassword";

const MAX_ACCOUNT_ID: u8 = 16;

const MELLANOX_DELL_VENDOR_ID: &str = "15b3";
const MELLANOX_DELL_DPU_DEVICE_IDS: [&str; 5] = [
    "a2df", // BF4 Family integrated network controller [BlueField-4 integrated network controller]
    "a2d9", // MT43162 BlueField-3 Lx integrated ConnectX-7 network controller
    "a2dc", // MT43244 BlueField-3 integrated ConnectX-7 network controller
    "a2d2", // MT416842 BlueField integrated ConnectX-5 network controller
    "a2d6", // MT42822 BlueField-2 integrated ConnectX-6 Dx network controller
];

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
        self.s.power(action).await
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

    async fn bios(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        self.s.bios().await
    }

    async fn get_base_mac_address(&self) -> Result<Option<String>, RedfishError> {
        self.s.get_base_mac_address().await
    }

    async fn machine_setup(&self, boot_interface_mac: Option<&str>) -> Result<(), RedfishError> {
        self.delete_job_queue().await?;

        let apply_time = dell::SetSettingsApplyTime {
            apply_time: dell::RedfishSettingsApplyTime::OnReset, // requires reboot to apply
        };

        // Find the DPU
        let mut has_dpu = true;
        let nic_slot = match self.dpu_nic_slot(boot_interface_mac).await {
            Ok(slot) => slot,
            Err(RedfishError::NoDpu) => {
                has_dpu = false;
                "".to_string()
            }
            Err(err) => {
                return Err(err);
            }
        };

        // dell idrac requires applying all bios settings at once.
        let machine_settings = self.machine_setup_attrs(&nic_slot);
        let set_machine_attrs = dell::SetBiosAttrs {
            redfish_settings_apply_time: apply_time,
            attributes: machine_settings,
        };

        let url = format!("Systems/{}/Bios/Settings/", self.s.system_id());
        self.s.client.patch(&url, set_machine_attrs).await?;

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

    async fn machine_setup_status(&self) -> Result<MachineSetupStatus, RedfishError> {
        let mut diffs = vec![];

        let bios = self.s.bios_attributes().await?;
        let nic_slot = self.dpu_nic_slot(None).await?;
        let expected_attrs = self.machine_setup_attrs(&nic_slot);

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
        match target {
            Enabled => {
                //self.enable_bios_lockdown().await?;
                self.enable_bmc_lockdown(dell::BootDevices::PXE).await
            }
            Disabled => {
                self.disable_bmc_lockdown(dell::BootDevices::PXE).await?;
                // BIOS lockdown blocks impi, ensure it's disabled even though we never set it
                self.disable_bios_lockdown().await
            }
        }
    }

    async fn lockdown_status(&self) -> Result<Status, RedfishError> {
        let mut message = String::new();
        let enabled = EnabledDisabled::Enabled.to_string();
        let disabled = EnabledDisabled::Disabled.to_string();

        // BIOS lockdown
        let url = format!("Systems/{}/Bios", self.s.system_id());
        let (_status_code, bios): (_, dell::Bios) = self.s.client.get(&url).await?;

        let in_band = bios
            .attributes
            .in_band_manageability_interface
            .unwrap_or_default();
        let uefi_var = bios.attributes.uefi_variable_access.unwrap_or_default();
        message.push_str(&format!(
            "BIOS: in_band_manageability_interface={in_band}, uefi_variable_access={uefi_var}. "
        ));

        let is_bios_locked = in_band == disabled
            && uefi_var == dell::UefiVariableAccessSettings::Controlled.to_string();
        let is_bios_unlocked = in_band == enabled
            && uefi_var == dell::UefiVariableAccessSettings::Standard.to_string();

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
            status: if is_bios_locked && is_bmc_locked {
                StatusInternal::Enabled
            } else if is_bios_unlocked && is_bmc_unlocked {
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

    async fn add_secure_boot_certificate(&self, pem_cert: &str) -> Result<Task, RedfishError> {
        self.s.add_secure_boot_certificate(pem_cert).await
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

    async fn get_ports(&self, chassis_id: &str) -> Result<Vec<String>, RedfishError> {
        self.s.get_ports(chassis_id).await
    }

    async fn get_port(
        &self,
        chassis_id: &str,
        id: &str,
    ) -> Result<crate::NetworkPort, RedfishError> {
        self.s.get_port(chassis_id, id).await
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
        let key = "JobState";
        let val = body
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

        Ok(JobState::from_str(val))
    }

    async fn get_collection(&self, id: ODataId) -> Result<Collection, RedfishError> {
        self.s.get_collection(id).await
    }

    async fn get_resource(&self, id: ODataId) -> Result<Resource, RedfishError> {
        self.s.get_resource(id).await
    }

    // machine_setup does this, but Dell requires all attributes to be sent at once so
    // we do not support doing just this part, on a Dell.
    async fn set_boot_order_dpu_first(
        &self,
        _mac_address: Option<&str>,
    ) -> Result<(), RedfishError> {
        Err(RedfishError::UnnecessaryOperation)
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
        match target {
            Enabled => self.enable_bmc_lockdown(dell::BootDevices::PXE).await,
            Disabled => self.disable_bmc_lockdown(dell::BootDevices::PXE).await,
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
}

impl Bmc {
    pub fn new(s: RedfishStandard) -> Result<Bmc, RedfishError> {
        Ok(Bmc { s })
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
        self.s
            .client
            .patch(&url, set_lockdown_attrs)
            .await
            .map(|_status_code| ())
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
        let (_status_code, body): (_, HashMap<String, serde_json::Value>) =
            self.s.client.get(url).await?;
        let key = "Attributes";
        let v = body
            .get(key)
            .ok_or_else(|| RedfishError::MissingKey {
                key: key.to_string(),
                url: url.to_string(),
            })?
            .as_object()
            .ok_or_else(|| RedfishError::InvalidKeyType {
                key: key.to_string(),
                expected_type: "Object".to_string(),
                url: url.to_string(),
            })
            .cloned()?;
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
            (_, Some(headers)) => {
                let key = "location";
                Ok(headers
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
                    .last()
                    .ok_or_else(|| RedfishError::InvalidValue {
                        url: url.to_string(),
                        field: key.to_string(),
                        err: InvalidValueError(
                            "unable to parse job_id from location string".to_string(),
                        ),
                    })?
                    .to_string())
            }
            (_, None) => Err(RedfishError::NoHeader),
        }
    }

    fn machine_setup_attrs(&self, nic_slot: &str) -> dell::MachineBiosAttrs {
        dell::MachineBiosAttrs {
            in_band_manageability_interface: EnabledDisabled::Disabled,
            uefi_variable_access: dell::UefiVariableAccessSettings::Controlled,
            serial_comm: dell::SerialCommSettings::OnConRedir,
            serial_port_address: dell::SerialPortSettings::Com1,
            fail_safe_baud: "115200".to_string(),
            con_term_type: dell::SerialPortTermSettings::Vt100Vt220,
            redir_after_boot: EnabledDisabled::Enabled,
            sriov_global_enable: EnabledDisabled::Enabled,
            tpm_security: OnOff::On,
            tpm2_hierarchy: dell::Tpm2HierarchySettings::Clear,
            http_device_1_enabled_disabled: EnabledDisabled::Enabled,
            pxe_device_1_enabled_disabled: EnabledDisabled::Disabled,
            boot_mode: "Uefi".to_string(),
            http_device_1_interface: nic_slot.to_string(),
            set_boot_order_en: nic_slot.to_string(),
            http_device_1_tls_mode: dell::TlsMode::None,
        }
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
            Some(headers) => {
                let key = "location";
                Ok(headers
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
                    .last()
                    .ok_or_else(|| RedfishError::InvalidValue {
                        url: url.to_string(),
                        field: key.to_string(),
                        err: InvalidValueError(
                            "unable to parse job_id from location string".to_string(),
                        ),
                    })?
                    .to_string())
            }
            None => Err(RedfishError::NoHeader),
        }
    }

    // Returns a string like "NIC.Slot.5-1"
    async fn dpu_nic_slot(&self, mac_address: Option<&str>) -> Result<String, RedfishError> {
        let chassis = self.get_chassis(self.s.system_id()).await?;
        let na_id = match chassis.network_adapters {
            Some(id) => id,
            None => {
                return Err(RedfishError::MissingKey {
                    key: "network_adapters".to_string(),
                    url: chassis.odata.unwrap().odata_id,
                })
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
                if mac_address.is_some() && nw_dev_func.ethernet.is_none() {
                    // can match on a MAC the interface doesn't report
                    continue;
                }
                if mac_address.is_none() && nw_dev_func.oem.is_none() {
                    // The vendor and device ids are in the OEM section
                    continue;
                }
                let oem = nw_dev_func.oem.unwrap();
                let Some(oem_dell) = oem.get("Dell") else {
                    continue;
                };
                let Some(oem_dell_map) = oem_dell.as_object() else {
                    continue;
                };
                let Some(dell_nic) = oem_dell_map.get("DellNIC") else {
                    continue;
                };
                let Some(dell_nic) = dell_nic.as_object() else {
                    continue;
                };
                let Some(nic_slot) = dell_nic
                    .get("Id")
                    .and_then(|id| id.as_str())
                    .map(|id| id.to_string())
                else {
                    continue;
                };
                match mac_address {
                    // Caller wants to match a specific MAC address
                    Some(want_mac) => {
                        if nw_dev_func
                            .ethernet
                            .unwrap()
                            .mac_address
                            .map(|x| x.to_lowercase())
                            .as_deref()
                            == Some(&want_mac.to_lowercase())
                        {
                            // we found a match by MAC address
                            return Ok(nic_slot);
                        }
                    }
                    // Caller wants the first DPU
                    None => {
                        let Some(vendor_id) =
                            dell_nic.get("PCIVendorID").and_then(|vid| vid.as_str())
                        else {
                            continue;
                        };
                        let Some(device_id) =
                            dell_nic.get("PCIDeviceID").and_then(|did| did.as_str())
                        else {
                            continue;
                        };
                        if vendor_id == MELLANOX_DELL_VENDOR_ID
                            && MELLANOX_DELL_DPU_DEVICE_IDS.contains(&device_id)
                        {
                            // we found a match by vendor and device id address
                            return Ok(nic_slot);
                        }
                    }
                }
            }
        }

        Err(RedfishError::NoDpu)
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
