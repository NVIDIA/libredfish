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
use reqwest::{
    header::{HeaderMap, HeaderName, IF_MATCH, IF_NONE_MATCH},
    Method,
};
use serde::Serialize;
use std::{collections::HashMap, path::Path, time::Duration, vec};
use tokio::fs::File;
use tracing::{debug, error, info, warn};
use version_compare::Version;

use crate::{
    model::{
        account_service::ManagerAccount,
        boot::{BootSourceOverrideEnabled, BootSourceOverrideTarget},
        certificate::Certificate,
        chassis::{Assembly, Chassis, NetworkAdapter},
        component_integrity::ComponentIntegrities,
        network_device_function::NetworkDeviceFunction,
        oem::{
            nvidia_dpu::NicMode,
            nvidia_viking::{
                BootDevices::{self},
                *,
            },
        },
        power::Power,
        resource::IsResource,
        secure_boot::SecureBoot,
        sel::{LogEntry, LogEntryCollection},
        sensor::{GPUSensors, Sensor},
        service_root::{RedfishVendor, ServiceRoot},
        software_inventory::SoftwareInventory,
        storage::Drives,
        task::Task,
        thermal::Thermal,
        update_service::{ComponentType, TransferProtocolType, UpdateService},
        BootOption, ComputerSystem, EnableDisable, Manager, ManagerResetType,
    },
    standard::RedfishStandard,
    BiosProfileType, Boot, BootOptions, Collection,
    EnabledDisabled::{self, Disabled, Enabled},
    JobState, MachineSetupDiff, MachineSetupStatus, ODataId, PCIeDevice, PowerState, Redfish,
    RedfishError, Resource, RoleId, Status, StatusInternal, SystemPowerControl,
};

const UEFI_PASSWORD_NAME: &str = "AdminPassword";

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

    /*
        https://docs.nvidia.com/dgx/dgxh100-user-guide/redfish-api-supp.html
        curl -k -u <bmc-user>:<password> --request PATCH 'https://<bmc-ip-address>/redfish/v1/AccountService/Accounts/2' --header 'If-Match: *'  --header 'Content-Type: application/json' --data-raw '{ "Password" : "<password>" }'
    */
    async fn change_password_by_id(
        &self,
        account_id: &str,
        new_pass: &str,
    ) -> Result<(), RedfishError> {
        let url = format!("AccountService/Accounts/{}", account_id);
        let mut data = HashMap::new();
        data.insert("Password", new_pass);
        self.s.client.patch_with_if_match(&url, data).await
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

    fn ac_powercycle_supported_by_power(&self) -> bool {
        false
    }

    async fn bmc_reset(&self) -> Result<(), RedfishError> {
        self.s
            .reset_manager(
                ManagerResetType::ForceRestart,
                Some(vec![(IF_MATCH, "*".to_string())]),
            )
            .await
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
        let mut output = vec![];
        for chassis_id in self
            .get_chassis_all()
            .await?
            .iter()
            // TODO: proper filtering for which chassis contains a gpu
            .filter(|c| c.starts_with("HGX_GPU"))
        {
            if let Some(sensor_ids) = self.get_chassis(chassis_id.as_str()).await?.sensors {
                output.push(GPUSensors {
                    gpu_id: chassis_id.to_string(),
                    sensors: self
                        .get_collection(sensor_ids)
                        .await
                        .and_then(|c| c.try_get::<Sensor>())?
                        .members,
                });
            }
        }

        Ok(output)
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
        self.clear_nvram().await
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
        self.set_bios_attributes().await
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
        // TODO: these values are wrong.
        // Setting to (0,0,0,false,0) causes account lockout. So set them to less harmful values
        let body = HashMap::from([
            ("AccountLockoutThreshold", Value::Number(4.into())),
            ("AccountLockoutDuration", Value::Number(20.into())),
            ("AccountLockoutCounterResetAfter", Value::Number(20.into())),
            ("AccountLockoutCounterResetEnabled", Value::Bool(true)),
            ("AuthFailureLoggingThreshold", Value::Number(2.into())),
        ]);
        return self
            .s
            .client
            .patch_with_if_match("AccountService", body)
            .await;
    }

    async fn lockdown(&self, target: EnabledDisabled) -> Result<(), RedfishError> {
        use EnabledDisabled::*;
        match target {
            Enabled => self.enable_lockdown().await,
            Disabled => self.disable_lockdown().await,
        }
    }

    // TODO: This needs rework
    async fn lockdown_status(&self) -> Result<Status, RedfishError> {
        let bios = self.get_bios().await?;
        let bios = bios.attributes;
        // TODO: This is definitely not correct
        let (message, status) = match (bios.kcs_interface_disable, bios.redfish_enable) {
            (None, None) => ("missing".to_string(), StatusInternal::Disabled),
            (None, Some(rf)) => (format!("redfish_enable={}.", rf), StatusInternal::Partial),
            (Some(kcs), None) => (
                format!("ipmi_kcs_disable={}.", kcs),
                StatusInternal::Partial,
            ),
            (Some(kcs), Some(rf)) => {
                let status = if kcs == *KCS_INTERFACE_DISABLE_DENY_ALL
                /*&& bios.redfish_enable == Disabled */
                {
                    StatusInternal::Enabled
                } else if kcs == KCS_INTERFACE_DISABLE_ALLOW_ALL && rf == EnabledDisabled::Enabled {
                    StatusInternal::Disabled
                } else {
                    StatusInternal::Partial
                };
                (
                    format!("ipmi_kcs_disable={}, redfish_enable={}.", kcs, rf),
                    status,
                )
            }
        };
        // todo: fix this once dgx viking team adds support
        Ok(Status { message, status })
    }

    async fn setup_serial_console(&self) -> Result<(), RedfishError> {
        let serial_console = BiosAttributes {
            acpi_spcr_baud_rate: DEFAULT_ACPI_SPCR_BAUD_RATE.to_string().into(),
            baud_rate0: DEFAULT_BAUD_RATE0.to_string().into(),
            acpi_spcr_console_redirection_enable: DEFAULT_ACPI_SPCR_CONSOLE_REDIRECTION_ENABLE
                .into(),
            acpi_spcr_flow_control: DEFAULT_ACPI_SPCR_FLOW_CONTROL.to_string().into(),
            acpi_spcr_port: DEFAULT_ACPI_SPCR_PORT.to_string().into(),
            acpi_spcr_terminal_type: DEFAULT_ACPI_SPCR_TERMINAL_TYPE.to_string().into(),
            console_redirection_enable0: DEFAULT_CONSOLE_REDIRECTION_ENABLE0.into(),
            terminal_type0: DEFAULT_TERMINAL_TYPE0.to_string().into(),
            ..Default::default()
        };
        // TODO: need to figure out from viking team on patching this:
        // let bmc_serial = nvidia_viking::BmcSerialConsoleAttributes {
        //    bit_rate: "115200".to_string(),
        //    data_bits: "8".to_string(),
        //    flow_control: "None".to_string(),
        //    interface_enabled: true,
        //    parity: "None".to_string(),
        //    stop_bits: "1".to_string(),
        //};

        let set_serial_attrs = SetBiosAttributes {
            attributes: serial_console,
        };
        return self.patch_bios_attributes(set_serial_attrs).await;
    }

    async fn serial_console_status(&self) -> Result<Status, RedfishError> {
        self.bios_serial_console_status().await
        // TODO: add bmc serial console status
    }

    async fn get_boot_options(&self) -> Result<BootOptions, RedfishError> {
        self.s.get_boot_options().await
    }

    async fn get_boot_option(&self, option_id: &str) -> Result<BootOption, RedfishError> {
        self.s.get_boot_option(option_id).await
    }

    async fn boot_once(&self, target: Boot) -> Result<(), RedfishError> {
        match target {
            Boot::Pxe => {
                self.set_boot_override(
                    BootSourceOverrideTarget::Pxe,
                    BootSourceOverrideEnabled::Once,
                )
                .await
            }
            Boot::HardDisk => {
                self.set_boot_override(
                    BootSourceOverrideTarget::Hdd,
                    BootSourceOverrideEnabled::Once,
                )
                .await
            }
            Boot::UefiHttp => {
                self.set_boot_override(
                    BootSourceOverrideTarget::UefiHttp,
                    BootSourceOverrideEnabled::Once,
                )
                .await
            }
        }
    }

    async fn boot_first(&self, target: Boot) -> Result<(), RedfishError> {
        match target {
            Boot::Pxe => self.set_boot_order(BootDevices::Pxe).await,
            Boot::HardDisk => self.set_boot_order(BootDevices::Hdd).await,
            Boot::UefiHttp => self.set_boot_order(BootDevices::UefiHttp).await,
        }
    }

    async fn clear_tpm(&self) -> Result<(), RedfishError> {
        let tpm = BiosAttributes {
            tpm_operation: Some(DEFAULT_TPM_OPERATION.to_string()),
            tpm_support: Some(DEFAULT_TPM_SUPPORT),
            ..Default::default()
        };

        let set_tpm_attrs = SetBiosAttributes { attributes: tpm };
        return self.patch_bios_attributes(set_tpm_attrs).await;
    }

    async fn pending(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        let url = format!("Systems/{}/Bios/SD", self.s.system_id());
        self.s.pending_with_url(&url).await
    }

    async fn clear_pending(&self) -> Result<(), RedfishError> {
        // TODO: check with viking team, unsupported
        Ok(())
    }

    async fn pcie_devices(&self) -> Result<Vec<PCIeDevice>, RedfishError> {
        let chassis = self
            .s
            .get_chassis_all()
            .await?
            .into_iter()
            .filter(|chassis| {
                chassis.starts_with("HGX_GPU_SXM") || chassis.starts_with("HGX_NVSwitch")
            })
            .collect();
        self.s.pcie_devices_for_chassis(chassis).await
    }

    async fn update_firmware(&self, firmware: tokio::fs::File) -> Result<Task, RedfishError> {
        self.s.update_firmware(firmware).await
    }

    /// update_firmware_multipart returns a string with the task ID
    async fn update_firmware_multipart(
        &self,
        filename: &Path,
        _reboot: bool,
        timeout: Duration,
        component_type: ComponentType,
    ) -> Result<String, RedfishError> {
        let firmware = File::open(&filename)
            .await
            .map_err(|e| RedfishError::FileError(format!("Could not open file: {e}")))?;

        let parameters =
            serde_json::to_string(&UpdateParameters::new(component_type)).map_err(|e| {
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
                "UpdateService/upload", // Viking does "upload" instead of "MultiPartUpload"
                false,
                timeout,
            )
            .await?;

        let loc = match loc {
            None => "Unknown".to_string(),
            Some(x) => x,
        };

        // It returns the full endpoint, we just want the task ID
        Ok(loc.replace("/redfish/v1/TaskService/Tasks/", ""))
    }

    async fn get_tasks(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_tasks().await
    }

    async fn get_task(&self, id: &str) -> Result<crate::model::task::Task, RedfishError> {
        self.s.get_task(id).await
    }

    async fn get_update_service(&self) -> Result<UpdateService, RedfishError> {
        self.s.get_update_service().await
    }

    async fn get_firmware(&self, id: &str) -> Result<SoftwareInventory, RedfishError> {
        self.s.get_firmware(id).await
    }

    async fn get_software_inventories(&self) -> Result<Vec<String>, RedfishError> {
        self.s
            .get_members_with_timout(
                "UpdateService/FirmwareInventory",
                Some(Duration::from_secs(180)),
            )
            .await
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
        let mut data = HashMap::new();
        data.insert("SecureBootEnable", true);
        let url = format!("Systems/{}/SecureBoot", self.s.system_id());
        return self.s.client.patch_with_if_match(&url, data).await;
    }

    async fn disable_secure_boot(&self) -> Result<(), RedfishError> {
        let mut data = HashMap::new();
        data.insert("SecureBootEnable", false);
        let url = format!("Systems/{}/SecureBoot", self.s.system_id());
        return self.s.client.patch_with_if_match(&url, data).await;
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
        self.s
            .change_bios_password(UEFI_PASSWORD_NAME, current_uefi_password, new_uefi_password)
            .await
    }

    async fn change_boot_order(&self, boot_array: Vec<String>) -> Result<(), RedfishError> {
        self.change_boot_order_with_etag(boot_array, None).await
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

    async fn get_base_mac_address(&self) -> Result<Option<String>, RedfishError> {
        self.s.get_base_mac_address().await
    }

    //
    // Details of changing boot order in DGX H100 can be found at
    // https://docs.nvidia.com/dgx/dgxh100-user-guide/redfish-api-supp.html#modifying-the-boot-order-on-dgx-h100-using-redfish.

    async fn set_boot_order_dpu_first(
        &self,
        address: &str,
    ) -> Result<Option<String>, RedfishError> {
        let mut system: ComputerSystem = self.s.get_system().await?;
        let mac_address = address.replace(':', "").to_uppercase();

        debug!("Using DPU with mac_address {}", mac_address);

        // Get all boot options
        let all_boot_options: Vec<BootOption> = match system.boot.boot_options {
            None => {
                return Err(RedfishError::MissingKey {
                    key: "boot.boot_options".to_string(),
                    url: system.odata.odata_id.to_string(),
                });
            }
            Some(boot_options_id) => self
                .get_collection(boot_options_id)
                .await
                .and_then(|t1| t1.try_get::<BootOption>())
                .iter()
                .flat_map(move |x1| x1.members.clone())
                .collect::<Vec<BootOption>>(),
        };

        // We should use system_settings.settings_object if it exits for updating boot order
        if let Some(red_settings) = system.redfish_settings {
            if let Some(settings_object_id) = red_settings.settings_object {
                system = self
                    .get_resource(settings_object_id)
                    .await
                    .and_then(|t| t.try_get())?;
            }
        }

        debug!("Current boot order {}", system.boot.boot_order.join(","));
        let mut new_boot_order = system.boot.boot_order.clone();

        // find the Ipv4 uefihttp boot option available for the mac_address and move it to the front of new_boot_order.
        let boot_options_for_dpu = all_boot_options
            .clone()
            .into_iter()
            .filter_map(|v| {
                let path = v
                    .uefi_device_path
                    .clone()
                    .unwrap_or_default()
                    .to_uppercase();
                if path.contains(mac_address.as_str())
                    && path.contains("IPV4")
                    && v.alias
                        .clone()
                        .unwrap_or("".to_string())
                        .to_uppercase()
                        .contains("UEFIHTTP")
                {
                    Some(v)
                } else {
                    None
                }
            })
            .collect::<Vec<BootOption>>();
        debug!(
            "{} boot options available for dpu {}",
            boot_options_for_dpu.len(),
            mac_address
        );
        debug!("{all_boot_options:?}");
        debug!(
            "boot options for mac {} are {:?}",
            mac_address, boot_options_for_dpu
        );

        let mut selected_boot_option = match boot_options_for_dpu.first() {
            Some(x) => x.to_owned(),
            None => {
                return Err(RedfishError::GenericError {
                    error: format!(
                        "no IPv4 Uefi Http boot option found for mac address {mac_address}; current boot options:\n {all_boot_options:?}",
                    ),
                })
            }
        };

        // For some reason collection doesn't include @odata.etag property which is required for PATCH'ing as per the doc
        if selected_boot_option.odata.odata_etag.is_none() {
            selected_boot_option = self
                .get_resource(selected_boot_option.odata.clone().odata_id.into())
                .await
                .and_then(|t2| t2.try_get())?;
            if selected_boot_option.odata.odata_etag.is_none() {
                return Err(RedfishError::MissingKey {
                    key: "@odata.etag".to_string(),
                    url: selected_boot_option.odata_id(),
                });
            };
        };

        // reorder new_boot
        let index = match new_boot_order
            .iter()
            .position(|x| *x == selected_boot_option.boot_option_reference.as_ref())
        {
            Some(u) => u,
            None => {
                return Err(RedfishError::GenericError {
                    error: format!(
                        "Boot option {} is not found in boot order list {}",
                        selected_boot_option.boot_option_reference,
                        new_boot_order.join(",")
                    ),
                })
            }
        };
        new_boot_order.remove(index);
        new_boot_order.insert(0, selected_boot_option.boot_option_reference.clone());
        debug!("current boot order is {:?}", system.boot.boot_order.clone());
        debug!("new boot order is {new_boot_order:?}");
        debug!(
            "new boot order etag {}",
            selected_boot_option
                .odata
                .odata_etag
                .clone()
                .unwrap_or_default()
        );

        self.change_boot_order_with_etag(new_boot_order, selected_boot_option.odata.odata_etag)
            .await?;
        Ok(None)
    }

    async fn clear_uefi_password(
        &self,
        current_uefi_password: &str,
    ) -> Result<Option<String>, RedfishError> {
        self.change_uefi_password(current_uefi_password, "").await
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

    /***
         curl -k -u admin:admin
         --request POST
         --location 'https://<bmcip>/redfish/v1/UpdateService/Actions/Oem/NvidiaUpdateService.ClearNVRAM' \
         --header 'Content-Type: application/json' \
         --data '{
         "Targets": ["/redfish/v1/UpdateService/FirmwareInventory/HostBIOS_0"]
         }'
    ***/
    async fn clear_nvram(&self) -> Result<(), RedfishError> {
        let data = HashMap::from([(
            "Targets",
            vec!["/redfish/v1/UpdateService/FirmwareInventory/HostBIOS_0".to_string()],
        )]);

        self.s
            .client
            .post(
                "UpdateService/Actions/Oem/NvidiaUpdateService.ClearNVRAM",
                data,
            )
            .await
            .map(|_status_code| Ok(()))?
    }

    async fn get_nic_mode(&self) -> Result<Option<NicMode>, RedfishError> {
        self.s.get_nic_mode().await
    }

    async fn set_nic_mode(&self, mode: NicMode) -> Result<(), RedfishError> {
        self.s.set_nic_mode(mode).await
    }

    async fn enable_infinite_boot(&self) -> Result<(), RedfishError> {
        let attrs = BiosAttributes {
            nvidia_infiniteboot: DEFAULT_NVIDIA_INFINITEBOOT.into(),
            ..Default::default()
        };
        let set_attrs = SetBiosAttributes { attributes: attrs };
        self.patch_bios_attributes(set_attrs).await
    }

    async fn is_infinite_boot_enabled(&self) -> Result<Option<bool>, RedfishError> {
        let bios = self.get_bios().await?;
        match bios.attributes.nvidia_infiniteboot {
            Some(is_infinite_boot_enabled) => Ok(Some(
                is_infinite_boot_enabled == DEFAULT_NVIDIA_INFINITEBOOT,
            )),
            None => Ok(None),
        }
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
        // Get the current values
        let bios = self.get_bios().await?;

        let sc = self.serial_console_status().await?;
        if !sc.is_fully_enabled() {
            diffs.push(MachineSetupDiff {
                key: "serial_console".to_string(),
                expected: "Enabled".to_string(),
                actual: sc.status.to_string(),
            });
        }

        let virt = self.get_virt_enabled().await?;
        if !virt.is_enabled() {
            diffs.push(MachineSetupDiff {
                key: "virt".to_string(),
                expected: "Enabled".to_string(),
                actual: virt.to_string(),
            });
        }

        let enabled_disabled_attributes_needed = [
            ("Ipv4Http", bios.attributes.ipv4_http, DEFAULT_IPV4_HTTP),
            ("Ipv4Pxe", bios.attributes.ipv4_pxe, DEFAULT_IPV4_PXE),
            ("Ipv6Http", bios.attributes.ipv6_http, DEFAULT_IPV6_HTTP),
            ("Ipv6Pxe", bios.attributes.ipv6_pxe, DEFAULT_IPV6_PXE),
        ];

        for (bios_attribute_name, current_value, expected_value) in
            enabled_disabled_attributes_needed
        {
            if let Some(current_val) = current_value {
                if current_val != expected_value {
                    diffs.push(MachineSetupDiff {
                        key: bios_attribute_name.to_string(),
                        expected: expected_value.to_string(),
                        actual: current_val.to_string(),
                    });
                }
            }
        }

        let enable_disable_attributes_needed = [(
            "NvidiaInfiniteboot",
            bios.attributes.nvidia_infiniteboot,
            DEFAULT_NVIDIA_INFINITEBOOT,
        )];
        for (name, current_value, expected_value) in enable_disable_attributes_needed {
            if let Some(current_val) = current_value {
                if current_val != expected_value {
                    diffs.push(MachineSetupDiff {
                        key: name.to_string(),
                        expected: expected_value.to_string(),
                        actual: current_val.to_string(),
                    });
                }
            }
        }

        Ok(diffs)
    }

    async fn get_expected_and_actual_first_boot_option(
        &self,
        boot_interface_mac: &str,
    ) -> Result<(Option<String>, Option<String>), RedfishError> {
        let system = self.s.get_system().await?;
        let mac_address = boot_interface_mac.replace(':', "").to_uppercase();

        // Get all boot options
        let all_boot_options: Vec<BootOption> = match system.boot.boot_options {
            None => {
                return Err(RedfishError::MissingKey {
                    key: "boot.boot_options".to_string(),
                    url: system.odata.odata_id.to_string(),
                });
            }
            Some(boot_options_id) => self
                .get_collection(boot_options_id)
                .await
                .and_then(|t1| t1.try_get::<BootOption>())
                .iter()
                .flat_map(move |x1| x1.members.clone())
                .collect::<Vec<BootOption>>(),
        };

        // find the Ipv4 uefihttp boot option available for the mac_address
        let boot_options_for_dpu = all_boot_options
            .clone()
            .into_iter()
            .filter_map(|v| {
                let path = v
                    .uefi_device_path
                    .clone()
                    .unwrap_or_default()
                    .to_uppercase();
                if path.contains(mac_address.as_str())
                    && path.contains("IPV4")
                    && v.alias
                        .clone()
                        .unwrap_or("".to_string())
                        .to_uppercase()
                        .contains("UEFIHTTP")
                {
                    Some(v)
                } else {
                    None
                }
            })
            .collect::<Vec<BootOption>>();

        let expected_first_boot_option = boot_options_for_dpu
            .first()
            .map(|opt| opt.display_name.clone());

        // Get actual first boot option
        let actual_first_boot_ref = system.boot.boot_order.first().cloned();
        let actual_first_boot_option = actual_first_boot_ref.and_then(|boot_ref| {
            all_boot_options
                .iter()
                .find(|opt| opt.boot_option_reference.as_ref() == boot_ref)
                .map(|opt| opt.display_name.clone())
        });

        Ok((expected_first_boot_option, actual_first_boot_option))
    }

    async fn check_firmware_version(
        &self,
        firmware_id: String,
        minimum_version: String,
        recommended_version: String,
    ) -> Result<(), RedfishError> {
        let firmware = self.get_firmware(&firmware_id).await?;
        if let Some(version) = firmware.version {
            let current = Version::from(&version);
            info!("{firmware_id} is {version} ");
            let minimum = Version::from(&minimum_version);
            let recommended = Version::from(&recommended_version);
            if current < minimum {
                error!("{firmware_id} is below minimum version. {version} < {minimum_version}");
                return Err(RedfishError::NotSupported(format!(
                    "{firmware_id} {version} < {minimum_version}"
                )));
            }
            if current < recommended {
                warn!(
                    "{firmware_id} is below recommended version. {version} < {recommended_version}"
                );
            }
            return Ok(());
        }
        Err(RedfishError::NotSupported(format!(
            "{firmware_id} unknown version < {minimum_version}"
        )))
    }

    async fn enable_lockdown(&self) -> Result<(), RedfishError> {
        // assuming that the viking bmc does not modify the suffixes
        self.check_firmware_version(
            "HostBIOS_0".to_string(),
            MINIMUM_BIOS_VERSION.to_string(),
            RECOMMENDED_BIOS_VERSION.to_string(),
        )
        .await?;
        self.check_firmware_version(
            "HostBMC_0".to_string(),
            MINIMUM_BMC_FW_VERSION.to_string(),
            RECOMMENDED_BMC_FW_VERSION.to_string(),
        )
        .await?;

        let lockdown_attrs = BiosAttributes {
            kcs_interface_disable: DEFAULT_KCS_INTERFACE_DISABLE.to_string().into(),
            redfish_enable: Disabled.into(), // todo: this should be disabled for the virtual usb nic, not yet implemented by dgx team
            ..Default::default()
        };
        let set_lockdown = SetBiosAttributes {
            attributes: lockdown_attrs,
        };
        self.patch_bios_attributes(set_lockdown).await
    }

    async fn disable_lockdown(&self) -> Result<(), RedfishError> {
        let lockdown_attrs = BiosAttributes {
            kcs_interface_disable: KCS_INTERFACE_DISABLE_ALLOW_ALL.to_string().into(),
            redfish_enable: Enabled.into(),
            ..Default::default()
        };
        let set_lockdown = SetBiosAttributes {
            attributes: lockdown_attrs,
        };
        self.patch_bios_attributes(set_lockdown).await
    }
    async fn get_virt_enabled(&self) -> Result<EnabledDisabled, RedfishError> {
        let bios = self.get_bios().await?;
        // We are told if the attribute is missing it is Enabled by default
        if bios
            .attributes
            .sriov_enable
            .unwrap_or(EnableDisable::Enable)
            == DEFAULT_SRIOV_ENABLE
            && bios.attributes.vtd_support.unwrap_or(EnableDisable::Enable) == DEFAULT_VTD_SUPPORT
        {
            Ok(EnabledDisabled::Enabled)
        } else {
            Ok(EnabledDisabled::Disabled)
        }
    }

    async fn bios_serial_console_status(&self) -> Result<Status, RedfishError> {
        let mut message = String::new();

        let mut enabled = true;
        let mut disabled = true;

        let bios = self.get_bios().await?;
        let bios = bios.attributes;

        if let Some(val) = bios.acpi_spcr_console_redirection_enable {
            message.push_str(&format!("acpi_spcr_console_redirection_enable={val} "));
            match val {
                true => {
                    // enabled
                    disabled = false;
                }
                false => {
                    // disabled
                    enabled = false;
                }
            }
        }
        if let Some(val) = bios.console_redirection_enable0 {
            message.push_str(&format!("console_redirection_enable0={val} "));
            match val {
                true => {
                    disabled = false;
                }
                false => {
                    enabled = false;
                }
            }
        }
        // All of these need a specific value for serial console access to work.
        // Any other value counts as correctly disabled.

        if let Some(val) = &bios.acpi_spcr_port {
            message.push_str(&format!("acpi_spcr_port={val} "));
            if val != DEFAULT_ACPI_SPCR_PORT {
                enabled = false;
            }
        }
        if let Some(val) = &bios.acpi_spcr_flow_control {
            message.push_str(&format!("acpi_spcr_flow_control={val} "));
            if val != DEFAULT_ACPI_SPCR_FLOW_CONTROL {
                enabled = false;
            }
        }
        if let Some(val) = &bios.acpi_spcr_baud_rate {
            message.push_str(&format!("acpi_spcr_baud_rate={val} "));
            if val != DEFAULT_ACPI_SPCR_BAUD_RATE {
                enabled = false;
            }
        }

        if let Some(val) = &bios.baud_rate0 {
            message.push_str(&format!("baud_rate0={val} "));
            if val != DEFAULT_BAUD_RATE0 {
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
        let with_name_str = device.to_string();
        let mut ordered = Vec::new(); // the final boot options
        let boot_options = self.s.get_system().await?.boot.boot_order;
        for member in boot_options {
            let member_url = member.replace("Boot", "");
            let b: BootOption = self.s.get_boot_option(member_url.as_str()).await?;
            // dgx has alias entries for each BootOption that matches BootDevices enum
            //
            // TODO: Many BootOptions have Alias="Pxe". This probably isn't doing what we want.
            //
            if b.alias.as_deref() == Some(&with_name_str) {
                ordered.insert(0, format!("Boot{}", b.id).to_string());
                continue;
            }
            ordered.push(format!("Boot{}", b.id).to_string());
        }
        Ok(Some(ordered))
    }

    async fn set_boot_override(
        &self,
        override_target: BootSourceOverrideTarget,
        override_enabled: BootSourceOverrideEnabled,
    ) -> Result<(), RedfishError> {
        let mut boot_data: HashMap<String, String> = HashMap::new();
        boot_data.insert("BootSourceOverrideMode".to_string(), "UEFI".to_string());
        boot_data.insert(
            "BootSourceOverrideEnabled".to_string(),
            format!("{}", override_enabled),
        );
        boot_data.insert(
            "BootSourceOverrideTarget".to_string(),
            format!("{}", override_target),
        );
        let data = HashMap::from([("Boot", boot_data)]);
        let url = format!("Systems/{}/SD ", self.s.system_id());
        let (_, body): (_, HashMap<String, serde_json::Value>) = self.s.client.get(&url).await?;
        let key = "@odata.etag";
        let etag = body
            .get(key)
            .ok_or_else(|| RedfishError::MissingKey {
                key: key.to_string(),
                url: url.to_string(),
            })?
            .as_str()
            .ok_or_else(|| RedfishError::InvalidKeyType {
                key: key.to_string(),
                expected_type: "Object".to_string(),
                url: url.to_string(),
            })?;

        let headers: Vec<(HeaderName, String)> = vec![(IF_MATCH, etag.to_string())];
        let timeout = Duration::from_secs(60);
        let (_status_code, _resp_body, _resp_headers): (
            _,
            Option<HashMap<String, serde_json::Value>>,
            Option<HeaderMap>,
        ) = self
            .s
            .client
            .req(
                Method::PATCH,
                &url,
                Some(data),
                Some(timeout),
                None,
                headers,
            )
            .await?;
        Ok(())
    }

    // nvidia dgx stores the sel as part of the manager
    async fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError> {
        let manager_id = self.s.manager_id();
        let url = format!("Managers/{manager_id}/LogServices/SEL/Entries");
        let (_status_code, log_entry_collection): (_, LogEntryCollection) =
            self.s.client.get(&url).await?;
        let log_entries = log_entry_collection.members;
        Ok(log_entries)
    }

    async fn change_boot_order_with_etag(
        &self,
        boot_array: Vec<String>,
        oetag: Option<String>,
    ) -> Result<(), RedfishError> {
        let data = HashMap::from([("Boot", HashMap::from([("BootOrder", boot_array)]))]);
        let url = format!("Systems/{}/SD", self.s.system_id());
        let etag = match oetag {
            Some(x) => x,
            None => {
                let (_, body): (_, HashMap<String, serde_json::Value>) =
                    self.s.client.get(&url).await?;
                let key = "@odata.etag";
                let t = body
                    .get(key)
                    .ok_or_else(|| RedfishError::MissingKey {
                        key: key.to_string(),
                        url: url.to_string(),
                    })?
                    .as_str()
                    .ok_or_else(|| RedfishError::InvalidKeyType {
                        key: key.to_string(),
                        expected_type: "Object".to_string(),
                        url: url.to_string(),
                    })?;
                t.to_string()
            }
        };

        let headers: Vec<(HeaderName, String)> = vec![(IF_NONE_MATCH, etag.to_string())];
        let timeout = Duration::from_secs(60);
        let (_status_code, _resp_body, _resp_headers): (
            _,
            Option<HashMap<String, serde_json::Value>>,
            Option<HeaderMap>,
        ) = self
            .s
            .client
            .req(
                Method::PATCH,
                &url,
                Some(data),
                Some(timeout),
                None,
                headers,
            )
            .await?;
        Ok(())
    }
    ///
    /// Returns current BIOS attributes that are used/modified
    ///
    async fn get_bios(&self) -> Result<Bios, RedfishError> {
        let url = &format!("Systems/{}/Bios", self.s.system_id());
        let (_status_code, bios): (_, Bios) = self.s.client.get(url).await?;
        Ok(bios)
    }

    async fn set_bios_attributes(&self) -> Result<(), RedfishError> {
        let url = &format!("Systems/{}/Bios", self.s.system_id());
        let (_status_code, bios): (_, Bios) = self.s.client.get(url).await?;
        let current_values = bios.attributes;

        let new_values = BiosAttributes {
            acpi_spcr_baud_rate: current_values
                .acpi_spcr_baud_rate
                .and(DEFAULT_ACPI_SPCR_BAUD_RATE.to_string().into()),
            baud_rate0: current_values
                .baud_rate0
                .and(DEFAULT_BAUD_RATE0.to_string().into()),
            acpi_spcr_console_redirection_enable: current_values
                .acpi_spcr_console_redirection_enable
                .and(DEFAULT_ACPI_SPCR_CONSOLE_REDIRECTION_ENABLE.into()),
            acpi_spcr_flow_control: current_values
                .acpi_spcr_flow_control
                .and(DEFAULT_ACPI_SPCR_FLOW_CONTROL.to_string().into()),
            acpi_spcr_port: current_values
                .acpi_spcr_port
                .and(DEFAULT_ACPI_SPCR_PORT.to_string().into()),
            acpi_spcr_terminal_type: current_values
                .acpi_spcr_terminal_type
                .and(DEFAULT_ACPI_SPCR_TERMINAL_TYPE.to_string().into()),
            console_redirection_enable0: current_values
                .console_redirection_enable0
                .and(DEFAULT_ACPI_SPCR_CONSOLE_REDIRECTION_ENABLE.into()),
            terminal_type0: current_values
                .terminal_type0
                .and(DEFAULT_TERMINAL_TYPE0.to_string().into()),
            tpm_support: current_values.tpm_support.and(DEFAULT_TPM_SUPPORT.into()),
            kcs_interface_disable: None,
            tpm_operation: current_values
                .tpm_operation
                .and(DEFAULT_TPM_OPERATION.to_string().into()),
            sriov_enable: current_values.sriov_enable.and(DEFAULT_SRIOV_ENABLE.into()),
            vtd_support: current_values.vtd_support.and(DEFAULT_VTD_SUPPORT.into()),
            ipv4_http: current_values.ipv4_http.and(DEFAULT_IPV4_HTTP.into()),
            ipv4_pxe: current_values.ipv4_pxe.and(DEFAULT_IPV4_PXE.into()),
            ipv6_http: current_values.ipv6_http.and(DEFAULT_IPV6_HTTP.into()),
            ipv6_pxe: current_values.ipv6_pxe.and(DEFAULT_IPV6_PXE.into()),
            redfish_enable: None,
            nvidia_infiniteboot: current_values
                .nvidia_infiniteboot
                .and(DEFAULT_NVIDIA_INFINITEBOOT.into()),
        };

        self.patch_bios_attributes(SetBiosAttributes {
            attributes: new_values,
        })
        .await
    }

    async fn patch_bios_attributes<B>(&self, data: B) -> Result<(), RedfishError>
    where
        B: Serialize + ::std::fmt::Debug,
    {
        let url = format!("Systems/{}/Bios/SD", self.s.system_id());
        self.s.client.patch_with_if_match(&url, data).await
    }
}

// UpdateParameters is what is sent for a multipart firmware upload's metadata.
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct UpdateParameters {
    #[serde(skip_serializing_if = "Option::is_none")]
    targets: Option<Vec<String>>,
}

impl UpdateParameters {
    pub fn new(component: ComponentType) -> UpdateParameters {
        let targets = match component {
            ComponentType::Unknown => None,
            _ => Some(vec![match component {
                ComponentType::BMC => {
                    "/redfish/v1/UpdateService/FirmwareInventory/HostBMC_0".to_string()
                }
                ComponentType::UEFI => {
                    "/redfish/v1/UpdateService/FirmwareInventory/HostBIOS_0".to_string()
                }
                ComponentType::EROTBMC => {
                    "/redfish/v1/UpdateService/FirmwareInventory/EROT_BMC_0".to_string()
                }
                ComponentType::EROTBIOS => {
                    "/redfish/v1/UpdateService/FirmwareInventory/EROT_BIOS_0".to_string()
                }
                ComponentType::CPLDMID => {
                    "/redfish/v1/UpdateService/FirmwareInventory/CPLDMID_0".to_string()
                }
                ComponentType::CPLDMB => {
                    "/redfish/v1/UpdateService/FirmwareInventory/CPLDMB_0".to_string()
                }
                ComponentType::PSU { num } => {
                    format!("/redfish/v1/UpdateService/FirmwareInventory/PSU_{num}")
                }
                ComponentType::PCIeSwitch { num } => {
                    format!("/redfish/v1/UpdateService/FirmwareInventory/PCIeSwitch_{num}")
                }
                ComponentType::PCIeRetimer { num } => {
                    format!("/redfish/v1/UpdateService/FirmwareInventory/PCIeRetimer_{num}")
                }
                ComponentType::HGXBMC => {
                    "/redfish/v1/UpdateService/FirmwareInventory/HGX_FW_BMC_0".to_string()
                }
                ComponentType::Unknown | ComponentType::CPLDPDB => "unreachable".to_string(),
            }]),
        };
        UpdateParameters { targets }
    }
}
