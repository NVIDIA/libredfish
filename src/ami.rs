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

use crate::{
    model::{
        account_service::ManagerAccount,
        boot::{BootSourceOverrideEnabled, BootSourceOverrideTarget},
        certificate::Certificate,
        chassis::{Assembly, Chassis, NetworkAdapter},
        storage::Drives,
        component_integrity::ComponentIntegrities,
        network_device_function::NetworkDeviceFunction,
        oem::nvidia_dpu::{HostPrivilegeLevel, NicMode},
        power::Power,
        secure_boot::SecureBoot,
        sel::LogEntry,
        sensor::GPUSensors,
        service_root::{RedfishVendor, ServiceRoot},
        software_inventory::SoftwareInventory,
        task::Task,
        thermal::Thermal,
        update_service::{ComponentType, TransferProtocolType, UpdateService},
        BootOption, ComputerSystem, Manager, ManagerResetType,
    },
    jsonmap,
    standard::RedfishStandard,
    BiosProfileType, Boot, BootOptions, Collection, EnabledDisabled, JobState,
    MachineSetupStatus, MachineSetupDiff, ODataId, PCIeDevice, PowerState, Redfish, RedfishError,
    Resource, RoleId, Status, StatusInternal, SystemPowerControl,
};

/// AMI uses BIOS attribute SETUP001 for Administrator Password (UEFI password)
const UEFI_PASSWORD_NAME: &str = "SETUP001";

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
    async fn change_username(&self, old_name: &str, new_name: &str) -> Result<(), RedfishError> {
        self.s.change_username(old_name, new_name).await
    }

    async fn change_password(&self, user: &str, new: &str) -> Result<(), RedfishError> {
        self.s.change_password(user, new).await
    }

    /// AMI BMC requires If-Match header for password changes
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

    async fn get_firmware(&self, id: &str) -> Result<SoftwareInventory, RedfishError> {
        self.s.get_firmware(id).await
    }

    async fn get_software_inventories(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_software_inventories().await
    }

    async fn get_tasks(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_tasks().await
    }

    async fn get_task(&self, id: &str) -> Result<Task, RedfishError> {
        self.s.get_task(id).await
    }

    async fn get_power_state(&self) -> Result<PowerState, RedfishError> {
        self.s.get_power_state().await
    }

    async fn get_service_root(&self) -> Result<ServiceRoot, RedfishError> {
        self.s.get_service_root().await
    }

    async fn get_systems(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_systems().await
    }

    async fn get_system(&self) -> Result<ComputerSystem, RedfishError> {
        self.s.get_system().await
    }

    async fn get_managers(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_managers().await
    }

    async fn get_manager(&self) -> Result<Manager, RedfishError> {
        self.s.get_manager().await
    }

    async fn get_secure_boot(&self) -> Result<SecureBoot, RedfishError> {
        self.s.get_secure_boot().await
    }

    /// AMI BMC requires If-Match header for secure boot changes
    async fn disable_secure_boot(&self) -> Result<(), RedfishError> {
        let mut data = HashMap::new();
        data.insert("SecureBootEnable", false);
        let url = format!("Systems/{}/SecureBoot", self.s.system_id());
        self.s.client.patch_with_if_match(&url, data).await
    }

    /// AMI BMC requires If-Match header for secure boot changes
    async fn enable_secure_boot(&self) -> Result<(), RedfishError> {
        let mut data = HashMap::new();
        data.insert("SecureBootEnable", true);
        let url = format!("Systems/{}/SecureBoot", self.s.system_id());
        self.s.client.patch_with_if_match(&url, data).await
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

    async fn get_power_metrics(&self) -> Result<Power, RedfishError> {
        self.s.get_power_metrics().await
    }

    async fn power(&self, action: SystemPowerControl) -> Result<(), RedfishError> {
        self.s.power(action).await
    }

    /// AMI BMC only supports ForceRestart
    async fn bmc_reset(&self) -> Result<(), RedfishError> {
        self.s
            .reset_manager(ManagerResetType::ForceRestart, None)
            .await
    }

    async fn chassis_reset(
        &self,
        chassis_id: &str,
        reset_type: SystemPowerControl,
    ) -> Result<(), RedfishError> {
        self.s.chassis_reset(chassis_id, reset_type).await
    }

    async fn bmc_reset_to_defaults(&self) -> Result<(), RedfishError> {
        self.s.bmc_reset_to_defaults().await
    }

    async fn get_thermal_metrics(&self) -> Result<Thermal, RedfishError> {
        self.s.get_thermal_metrics().await
    }

    async fn get_gpu_sensors(&self) -> Result<Vec<GPUSensors>, RedfishError> {
        self.s.get_gpu_sensors().await
    }

    async fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError> {
        self.s.get_system_event_log().await
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

    /// Machine setup for AMI BMC.
    ///
    /// Sets up:
    /// 1. Serial console
    /// 2. Clears TPM
    /// 3. BIOS settings
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
        let attrs = self.machine_setup_attrs();
        self.set_bios(attrs).await?;
        Ok(())
    }

    /// Check machine setup status for AMI BMC.
    async fn machine_setup_status(
        &self,
        boot_interface_mac: Option<&str>,
    ) -> Result<MachineSetupStatus, RedfishError> {
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

    async fn is_bios_setup(&self, _boot_interface_mac: Option<&str>) -> Result<bool, RedfishError> {
        let diffs = self.diff_bios_bmc_attr().await?;
        Ok(diffs.is_empty())
    }

    /// AMI BMC requires If-Match header for password policy changes
    async fn set_machine_password_policy(&self) -> Result<(), RedfishError> {
        use serde_json::Value;
        let body = HashMap::from([
            ("AccountLockoutThreshold", Value::Number(0.into())),
            ("AccountLockoutDuration", Value::Number(0.into())),
            ("AccountLockoutCounterResetAfter", Value::Number(0.into())),
        ]);
        self.s
            .client
            .patch_with_if_match("AccountService", body)
            .await
    }

    /// AMI lockdown - controls KCS access, USB support, and Host Interface
    async fn lockdown(&self, target: EnabledDisabled) -> Result<(), RedfishError> {
        use EnabledDisabled::*;
        // (KCSACP, USB000, HostInterface enabled)
        let (kcsacp, usb, hi_enabled) = match target {
            Enabled => ("Deny All", "Disabled", false),
            Disabled => ("Allow All", "Enabled", true),
        };

        // Set BIOS attributes
        self.set_bios(HashMap::from([
            ("KCSACP".to_string(), kcsacp.into()),
            ("USB000".to_string(), usb.into()),
        ]))
        .await?;

        // Set Host Interface
        let hi_body = HashMap::from([("InterfaceEnabled", hi_enabled)]);
        self.s
            .client
            .patch_with_if_match("Managers/Self/HostInterfaces/Self", hi_body)
            .await
    }

    /// AMI lockdown_status - checks KCS access, USB support, and Host Interface
    async fn lockdown_status(&self) -> Result<Status, RedfishError> {
        let bios = self.s.bios().await?;
        let url = format!("Systems/{}/Bios", self.s.system_id());
        let attrs = jsonmap::get_object(&bios, "Attributes", &url)?;
        let kcsacp = jsonmap::get_str(attrs, "KCSACP", "Bios Attributes")?;
        let usb000 = jsonmap::get_str(attrs, "USB000", "Bios Attributes")?;

        let hi_url = "Managers/Self/HostInterfaces/Self";
        let (_status, hi): (_, serde_json::Value) = self.s.client.get(hi_url).await?;
        let hi_enabled = hi
            .get("InterfaceEnabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let message = format!(
            "kcs_access={}, usb_support={}, host_interface={}",
            kcsacp, usb000, hi_enabled
        );

        let is_locked = kcsacp == "Deny All" && usb000 == "Disabled" && !hi_enabled;
        let is_unlocked = kcsacp == "Allow All" && usb000 == "Enabled" && hi_enabled;

        Ok(Status {
            message,
            status: if is_locked {
                StatusInternal::Enabled
            } else if is_unlocked {
                StatusInternal::Disabled
            } else {
                StatusInternal::Partial
            },
        })
    }

    /// Setup serial console for AMI BMC via BIOS attributes.
    async fn setup_serial_console(&self) -> Result<(), RedfishError> {
        use serde_json::Value;

        let attributes: HashMap<String, Value> = HashMap::from([
            ("TER001".to_string(), "Enabled".into()),   // Console Redirection
            ("TER010".to_string(), "Enabled".into()),   // Console Redirection EMS
            ("TER06B".to_string(), "COM1".into()),      // Out-of-Band Mgmt Port
            ("TER0021".to_string(), "115200".into()),   // Bits per second
            ("TER0020".to_string(), "115200".into()),   // Bits per second EMS
            ("TER012".to_string(), "VT100Plus".into()), // Terminal Type
            ("TER011".to_string(), "VT-UTF8".into()),   // Terminal Type EMS
            ("TER05D".to_string(), "None".into()),      // Flow Control
        ]);

        self.set_bios(attributes).await
    }

    /// Check serial console status for AMI BMC.
    async fn serial_console_status(&self) -> Result<Status, RedfishError> {
        let bios = self.bios().await?;
        let url = format!("Systems/{}/Bios", self.s.system_id());
        let attrs = jsonmap::get_object(&bios, "Attributes", &url)?;

        let expected = vec![
            ("TER001", "Enabled", "Disabled"),
            ("TER010", "Enabled", "Disabled"),
            ("TER06B", "COM1", "any"),
            ("TER0021", "115200", "any"),
            ("TER0020", "115200", "any"),
            ("TER012", "VT100Plus", "any"),
            ("TER011", "VT-UTF8", "any"),
            ("TER05D", "None", "any"),
        ];

        let mut message = String::new();
        let mut enabled = true;
        let mut disabled = true;

        for (key, val_enabled, val_disabled) in expected {
            if let Some(val_current) = attrs.get(key).and_then(|v| v.as_str()) {
                message.push_str(&format!("{key}={val_current} "));
                if val_current != val_enabled {
                    enabled = false;
                }
                if val_current != val_disabled && val_disabled != "any" {
                    disabled = false;
                }
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

    async fn get_boot_options(&self) -> Result<BootOptions, RedfishError> {
        self.s.get_boot_options().await
    }

    async fn get_boot_option(&self, option_id: &str) -> Result<BootOption, RedfishError> {
        self.s.get_boot_option(option_id).await
    }

    async fn boot_once(&self, target: Boot) -> Result<(), RedfishError> {
        let override_target = match target {
            Boot::Pxe => BootSourceOverrideTarget::Pxe,
            Boot::HardDisk => BootSourceOverrideTarget::Hdd,
            Boot::UefiHttp => BootSourceOverrideTarget::UefiHttp,
        };
        self.set_boot_override(override_target, BootSourceOverrideEnabled::Once)
            .await
    }

    async fn boot_first(&self, target: Boot) -> Result<(), RedfishError> {
        self.s.boot_first(target).await
    }

    /// AMI BMC requires If-Match header for boot order changes
    async fn change_boot_order(&self, boot_array: Vec<String>) -> Result<(), RedfishError> {
        let body = HashMap::from([("Boot", HashMap::from([("BootOrder", boot_array)]))]);
        let url = format!("Systems/{}/SD", self.s.system_id());
        self.s.client.patch_with_if_match(&url, body).await
    }

    async fn clear_tpm(&self) -> Result<(), RedfishError> {
        self.set_bios(HashMap::from([("TCG006".to_string(), "TPM Clear".into())]))
            .await
    }

    async fn pcie_devices(&self) -> Result<Vec<PCIeDevice>, RedfishError> {
        self.s.pcie_devices().await
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

    async fn bios(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        self.s.bios().await
    }

    /// AMI BMC requires If-Match header for BIOS changes
    async fn set_bios(
        &self,
        values: HashMap<String, serde_json::Value>,
    ) -> Result<(), RedfishError> {
        let url = format!("Systems/{}/Bios/SD", self.s.system_id());
        let body = HashMap::from([("Attributes", values)]);
        self.s.client.patch_with_if_match(&url, body).await
    }

    async fn reset_bios(&self) -> Result<(), RedfishError> {
        self.s.factory_reset_bios().await
    }

    /// AMI uses /Bios/SD for pending settings
    async fn pending(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        let url = format!("Systems/{}/Bios/SD", self.s.system_id());
        self.s.pending_with_url(&url).await
    }

    /// AMI clear_pending - uses /Bios/SD instead of /Bios/Settings
    async fn clear_pending(&self) -> Result<(), RedfishError> {
        let pending_url = format!("Systems/{}/Bios/SD", self.s.system_id());
        let pending_attrs = self.s.pending_attributes(&pending_url).await?;
        let current_attrs = self.s.bios_attributes().await?;

        let reset_attrs: HashMap<_, _> = pending_attrs
            .iter()
            .filter(|(k, v)| current_attrs.get(*k) != Some(v))
            .map(|(k, _)| (k.clone(), current_attrs.get(k).cloned()))
            .collect();

        if reset_attrs.is_empty() {
            return Ok(());
        }

        let body = HashMap::from([("Attributes", reset_attrs)]);
        self.s
            .client
            .patch_with_if_match(&pending_url, body)
            .await
    }

    async fn get_network_device_functions(
        &self,
        chassis_id: &str,
    ) -> Result<Vec<String>, RedfishError> {
        self.s.get_network_device_functions(chassis_id).await
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

    /// AMI uses BIOS attribute SETUP001 for Administrator Password
    async fn change_uefi_password(
        &self,
        current_uefi_password: &str,
        new_uefi_password: &str,
    ) -> Result<Option<String>, RedfishError> {
        self.s
            .change_bios_password(UEFI_PASSWORD_NAME, current_uefi_password, new_uefi_password)
            .await
    }

    async fn clear_uefi_password(
        &self,
        current_uefi_password: &str,
    ) -> Result<Option<String>, RedfishError> {
        self.change_uefi_password(current_uefi_password, "").await
    }

    async fn get_job_state(&self, job_id: &str) -> Result<JobState, RedfishError> {
        self.s.get_job_state(job_id).await
    }

    async fn get_resource(&self, id: ODataId) -> Result<Resource, RedfishError> {
        self.s.get_resource(id).await
    }

    async fn get_collection(&self, id: ODataId) -> Result<Collection, RedfishError> {
        self.s.get_collection(id).await
    }

    /// Set the DPU (identified by MAC address) as the first boot option.
    async fn set_boot_order_dpu_first(
        &self,
        mac_address: &str,
    ) -> Result<Option<String>, RedfishError> {
        let mac = mac_address.to_uppercase();
        let system = self.get_system().await?;

        let boot_options_id =
            system
                .boot
                .boot_options
                .clone()
                .ok_or_else(|| RedfishError::MissingKey {
                    key: "boot.boot_options".to_string(),
                    url: system.odata.odata_id.clone(),
                })?;

        let all_boot_options: Vec<BootOption> = self
            .get_collection(boot_options_id)
            .await
            .and_then(|c| c.try_get::<BootOption>())?
            .members;

        let target = all_boot_options.iter().find(|opt| {
            let display = opt.display_name.to_uppercase();
            display.contains("HTTP") && display.contains("IPV4") && display.contains(&mac)
        });

        let Some(target) = target else {
            let all_names: Vec<_> = all_boot_options
                .iter()
                .map(|b| format!("{}: {}", b.id, b.display_name))
                .collect();
            return Err(RedfishError::MissingBootOption(format!(
                "No HTTP IPv4 boot option found for MAC {mac_address}; available: {:#?}",
                all_names
            )));
        };

        let target_id = target.boot_option_reference.clone();
        let mut boot_order = system.boot.boot_order;

        if boot_order.first() == Some(&target_id) {
            tracing::info!(
                "NO-OP: DPU ({mac_address}) is already first in boot order ({target_id})"
            );
            return Ok(None);
        }

        boot_order.retain(|id| id != &target_id);
        boot_order.insert(0, target_id);
        self.change_boot_order(boot_order).await?;
        Ok(None)
    }

    /// Check if boot order is setup correctly
    async fn is_boot_order_setup(&self, boot_interface_mac: &str) -> Result<bool, RedfishError> {
        let (expected, actual) = self
            .get_expected_and_actual_first_boot_option(boot_interface_mac)
            .await?;
        Ok(expected.is_some() && expected == actual)
    }

    async fn get_update_service(&self) -> Result<UpdateService, RedfishError> {
        self.s.get_update_service().await
    }

    async fn get_base_mac_address(&self) -> Result<Option<String>, RedfishError> {
        self.s.get_base_mac_address().await
    }

    /// AMI lockdown_bmc - BMC-only lockdown (Host Interface only)
    async fn lockdown_bmc(&self, target: EnabledDisabled) -> Result<(), RedfishError> {
        let interface_enabled = target == EnabledDisabled::Disabled;
        let hi_body = HashMap::from([("InterfaceEnabled", interface_enabled)]);
        let hi_url = "Managers/Self/HostInterfaces/Self";
        self.s.client.patch_with_if_match(hi_url, hi_body).await
    }

    async fn is_ipmi_over_lan_enabled(&self) -> Result<bool, RedfishError> {
        self.s.is_ipmi_over_lan_enabled().await
    }

    /// AMI BMC requires If-Match header for network protocol changes
    async fn enable_ipmi_over_lan(&self, target: EnabledDisabled) -> Result<(), RedfishError> {
        let url = format!("Managers/{}/NetworkProtocol", self.s.manager_id());
        let ipmi_data = HashMap::from([("ProtocolEnabled", target.is_enabled())]);
        let data = HashMap::from([("IPMI", ipmi_data)]);
        self.s.client.patch_with_if_match(&url, data).await
    }

    async fn enable_rshim_bmc(&self) -> Result<(), RedfishError> {
        self.s.enable_rshim_bmc().await
    }

    /// AMI clear_nvram - sets RECV000 (Reset NVRAM) to "Enabled"
    async fn clear_nvram(&self) -> Result<(), RedfishError> {
        self.set_bios(HashMap::from([("RECV000".to_string(), "Enabled".into())]))
            .await
    }

    async fn get_nic_mode(&self) -> Result<Option<NicMode>, RedfishError> {
        self.s.get_nic_mode().await
    }

    async fn set_nic_mode(&self, mode: NicMode) -> Result<(), RedfishError> {
        self.s.set_nic_mode(mode).await
    }

    async fn enable_infinite_boot(&self) -> Result<(), RedfishError> {
        self.set_bios(HashMap::from([("EndlessBoot".to_string(), "Enabled".into())]))
            .await
    }

    async fn is_infinite_boot_enabled(&self) -> Result<Option<bool>, RedfishError> {
        let bios = self.s.bios().await?;
        let url = format!("Systems/{}/Bios", self.s.system_id());
        let attrs = jsonmap::get_object(&bios, "Attributes", &url)?;
        let endless_boot = jsonmap::get_str(attrs, "EndlessBoot", "Bios Attributes")?;
        Ok(Some(endless_boot == "Enabled"))
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

    async fn get_component_integrities(&self) -> Result<ComponentIntegrities, RedfishError> {
        self.s.get_component_integrities().await
    }

    async fn get_firmware_for_component(
        &self,
        component_integrity_id: &str,
    ) -> Result<SoftwareInventory, RedfishError> {
        self.s
            .get_firmware_for_component(component_integrity_id)
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

    async fn set_host_privilege_level(&self, level: HostPrivilegeLevel) -> Result<(), RedfishError> {
        self.s.set_host_privilege_level(level).await
    }

    /// AMI doesn't support AC power cycle through standard power action
    fn ac_powercycle_supported_by_power(&self) -> bool {
        false
    }

    async fn set_utc_timezone(&self) -> Result<(), RedfishError> {
        self.s.set_utc_timezone().await
    }
}

impl Bmc {
    /// AMI requires patching to /Systems/{id} (NOT /SD) with If-Match header
    async fn set_boot_override(
        &self,
        override_target: BootSourceOverrideTarget,
        override_enabled: BootSourceOverrideEnabled,
    ) -> Result<(), RedfishError> {
        let boot_data = HashMap::from([
            ("BootSourceOverrideMode".to_string(), "UEFI".to_string()),
            (
                "BootSourceOverrideEnabled".to_string(),
                override_enabled.to_string(),
            ),
            (
                "BootSourceOverrideTarget".to_string(),
                override_target.to_string(),
            ),
        ]);
        let data = HashMap::from([("Boot", boot_data)]);
        let url = format!("Systems/{}", self.s.system_id());
        self.s.client.patch_with_if_match(&url, data).await
    }

    /// Get expected and actual first boot option for checking boot order setup.
    ///
    /// AMI boot option format example:
    /// DisplayName: "[Slot2]UEFI: HTTP IPv4 Nvidia Network Adapter - B8:E9:24:17:6D:72 P1"
    /// BootOptionReference: "Boot0001"
    ///
    async fn get_expected_and_actual_first_boot_option(
        &self,
        boot_interface_mac: &str,
    ) -> Result<(Option<String>, Option<String>), RedfishError> {
        let mac = boot_interface_mac.to_uppercase();
        let system = self.get_system().await?;

        let boot_options_id =
            system
                .boot
                .boot_options
                .clone()
                .ok_or_else(|| RedfishError::MissingKey {
                    key: "boot.boot_options".to_string(),
                    url: system.odata.odata_id.clone(),
                })?;

        let all_boot_options: Vec<BootOption> = self
            .get_collection(boot_options_id)
            .await
            .and_then(|c| c.try_get::<BootOption>())?
            .members;

        // Find expected boot option display name (HTTP IPv4 with matching MAC)
        let expected_first_boot_option = all_boot_options
            .iter()
            .find(|opt| {
                let display = opt.display_name.to_uppercase();
                display.contains("HTTP") && display.contains("IPV4") && display.contains(&mac)
            })
            .map(|opt| opt.display_name.clone());

        let actual_first_boot_option = system
            .boot
            .boot_order
            .first()
            .and_then(|first_ref| {
                all_boot_options
                    .iter()
                    .find(|opt| &opt.boot_option_reference == first_ref)
                    .map(|opt| opt.display_name.clone())
            });

        Ok((expected_first_boot_option, actual_first_boot_option))
    }

    /// Get the BIOS attributes for machine setup.
    fn machine_setup_attrs(&self) -> HashMap<String, serde_json::Value> {
        HashMap::from([
            ("VMXEN".to_string(), "Enable".into()),        // VMX (Intel Virtualization)
            ("PCIS007".to_string(), "Enabled".into()),     // SR-IOV Support
            ("NWSK000".to_string(), "Enabled".into()),     // Network Stack
            ("NWSK001".to_string(), "Disabled".into()),    // IPv4 PXE Support
            ("NWSK006".to_string(), "Enabled".into()),     // IPv4 HTTP Support
            ("NWSK002".to_string(), "Disabled".into()),    // IPv6 PXE Support
            ("NWSK007".to_string(), "Disabled".into()),    // IPv6 HTTP Support
            ("FBO001".to_string(), "UEFI".into()),         // Boot Mode Select
            ("EndlessBoot".to_string(), "Enabled".into()), // Infinite Boot
        ])
    }

    /// Check BIOS/BMC attributes against expected values for machine setup status.
    async fn diff_bios_bmc_attr(&self) -> Result<Vec<MachineSetupDiff>, RedfishError> {
        let mut diffs = vec![];

        // Check serial console status
        let sc = self.serial_console_status().await?;
        if !sc.is_fully_enabled() {
            diffs.push(MachineSetupDiff {
                key: "serial_console".to_string(),
                expected: "Enabled".to_string(),
                actual: sc.status.to_string(),
            });
        }

        // Check BIOS attributes
        let bios = self.s.bios_attributes().await?;
        let expected_attrs = self.machine_setup_attrs();

        for (key, expected) in expected_attrs {
            let Some(actual) = bios.get(&key) else {
                diffs.push(MachineSetupDiff {
                    key: key.to_string(),
                    expected: expected.to_string(),
                    actual: "_missing_".to_string(),
                });
                continue;
            };
            let act = actual.as_str().unwrap_or(&actual.to_string()).to_string();
            let exp = expected.as_str().unwrap_or(&expected.to_string()).to_string();
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
}
