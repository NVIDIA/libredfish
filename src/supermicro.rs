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

use crate::{
    model::{
        account_service::ManagerAccount,
        boot,
        chassis::{Chassis, NetworkAdapter},
        network_device_function::NetworkDeviceFunction,
        oem::supermicro::{self, FixedBootOrder},
        power::Power,
        secure_boot::SecureBoot,
        sel::LogEntry,
        sensor::GPUSensors,
        service_root::ServiceRoot,
        software_inventory::SoftwareInventory,
        task::Task,
        thermal::Thermal,
        update_service::{ComponentType, TransferProtocolType, UpdateService},
        BootOption, ComputerSystem, EnableDisable, InvalidValueError, Manager,
    },
    standard::RedfishStandard,
    Boot, BootOptions, Collection, EnabledDisabled, MachineSetupDiff, MachineSetupStatus, JobState,
    ODataId, PCIeDevice, PowerState, Redfish, RedfishError, Resource, RoleId, Status,
    StatusInternal, SystemPowerControl,
};

const MELLANOX_UEFI_HTTP4: &str = "UEFI HTTP IPv4 Mellanox Network Adapter";
const HARD_DISK: &str = "UEFI Hard Disk";
const NETWORK: &str = "UEFI Network";

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

    async fn change_username(&self, old_name: &str, new_name: &str) -> Result<(), RedfishError> {
        self.s.change_username(old_name, new_name).await
    }

    async fn change_password(
        &self,
        username: &str,
        new_password: &str,
    ) -> Result<(), RedfishError> {
        self.s.change_password(username, new_password).await
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

    async fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError> {
        self.s.get_system_event_log().await
    }

    async fn bios(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        self.s.bios().await
    }

    /// Note that you can't use this for initial setup unless you reboot and run it twice.
    /// `boot_first` won't find the Mellanox HTTP device. `uefi_nic_boot_attrs` enables it,
    /// but it won't show until after reboot so that step will fail on first time through.
    async fn machine_setup(&self, _boot_interface_mac: Option<&str>) -> Result<(), RedfishError> {
        self.setup_serial_console().await?;

        let bios_attrs = self.machine_setup_attrs().await?;
        let mut attrs = HashMap::new();
        attrs.extend(bios_attrs);
        let body = HashMap::from([("Attributes", attrs)]);
        let url = format!("Systems/{}/Bios", self.s.system_id());
        self.s
            .client
            .patch(&url, body)
            .await
            .map(|_status_code| ())?;

        self.boot_first(Boot::Pxe).await
    }

    async fn machine_setup_status(&self) -> Result<MachineSetupStatus, RedfishError> {
        let mut diffs = vec![];

        let sc = self.serial_console_status().await?;
        if !sc.is_fully_enabled() {
            diffs.push(MachineSetupDiff {
                key: "serial_console".to_string(),
                expected: "Enabled".to_string(),
                actual: sc.status.to_string(),
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

        // Supermicro has an Oem/FixedBootOrder separate from regular boot order
        // Must boot from network
        let fbo = self.get_boot_order().await?;
        let actual = fbo.fixed_boot_order.first();
        if actual.map(|s| s.as_str()) != Some(NETWORK) {
            diffs.push(MachineSetupDiff {
                key: "boot_order".to_string(),
                expected: NETWORK.to_string(),
                actual: format!("{actual:?}"),
            });
        }
        // The DPU should be the first NIC we try
        let boot_first = self.s.get_first_boot_option().await?;
        if !boot_first.display_name.contains(MELLANOX_UEFI_HTTP4) {
            diffs.push(MachineSetupDiff {
                key: "boot_first".to_string(),
                expected: MELLANOX_UEFI_HTTP4.to_string(),
                actual: boot_first.display_name,
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

    async fn set_machine_password_policy(&self) -> Result<(), RedfishError> {
        use serde_json::Value::Number;
        let body = HashMap::from([
            ("AccountLockoutThreshold", Number(0.into())),
            ("AccountLockoutDuration", Number(0.into())),
            ("AccountLockoutCounterResetAfter", Number(0.into())),
        ]);
        self.s
            .client
            .patch("AccountService", body)
            .await
            .map(|_status_code| ())
    }

    async fn lockdown(&self, target: EnabledDisabled) -> Result<(), RedfishError> {
        use EnabledDisabled::*;
        match target {
            Enabled => {
                self.set_host_interfaces(Disabled).await?;
                self.set_kcs_privilege(supermicro::Privilege::Callback)
                    .await?;
                self.set_syslockdown(Enabled).await?; // Lock last
            }
            Disabled => {
                self.set_syslockdown(Disabled).await?; // Unlock first
                self.set_kcs_privilege(supermicro::Privilege::Administrator)
                    .await?;
                self.set_host_interfaces(Enabled).await?;
            }
        }
        Ok(())
    }

    async fn lockdown_status(&self) -> Result<Status, RedfishError> {
        let is_hi_on = self.is_host_interface_enabled().await?;
        let kcs_privilege = self.get_kcs_privilege().await?;
        let is_syslockdown = self.get_syslockdown().await?;
        let message = format!("SysLockdownEnabled={is_syslockdown}, kcs_privilege={kcs_privilege}, host_interface_enabled={is_hi_on}");
        let is_locked =
            is_syslockdown && kcs_privilege == supermicro::Privilege::Callback && !is_hi_on;
        let is_unlocked =
            !is_syslockdown && kcs_privilege == supermicro::Privilege::Administrator && is_hi_on;
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

    /// On Supermicro this does nothing. Serial Console is on by default and can't be disabled
    /// or enabled via redfish. The properties under Systems/1, key SerialConsole are read only.
    async fn setup_serial_console(&self) -> Result<(), RedfishError> {
        Ok(())
    }

    async fn serial_console_status(&self) -> Result<Status, RedfishError> {
        let s_interface = self.s.get_serial_interface().await?;
        let system = self.s.get_system().await?;
        let Some(sr) = &system.serial_console else {
            return Err(RedfishError::NotSupported(
                "No SerialConsole in System object. Maybe it's in Manager and you have old firmware?".to_string(),
            ));
        };
        let is_enabled = sr.ssh.service_enabled
            && sr.max_concurrent_sessions != 0
            && s_interface.is_supermicro_default();
        let status = if is_enabled {
            StatusInternal::Enabled
        } else {
            StatusInternal::Disabled
        };
        Ok(Status {
            message: String::new(),
            status,
        })
    }

    async fn get_boot_options(&self) -> Result<BootOptions, RedfishError> {
        self.s.get_boot_options().await
    }

    async fn get_boot_option(&self, option_id: &str) -> Result<BootOption, RedfishError> {
        self.s.get_boot_option(option_id).await
    }

    /// Boot from this device once then go back to the normal boot order
    async fn boot_once(&self, target: Boot) -> Result<(), RedfishError> {
        if target == Boot::Pxe || target == Boot::UefiHttp {
            let _ = self.set_mellanox_first().await;
        }
        self.set_boot_override(target, true).await
    }

    /// Set which device we should boot from first.
    async fn boot_first(&self, target: Boot) -> Result<(), RedfishError> {
        let _ = self.set_mellanox_first().await;
        self.set_boot_order(target).await
    }

    /// Supermicro BMC does not appear to have this.
    /// TODO: Verify that this really clear the TPM.
    async fn clear_tpm(&self) -> Result<(), RedfishError> {
        let bios_attrs = self.s.bios_attributes().await?;
        let Some(attrs_map) = bios_attrs.as_object() else {
            return Err(RedfishError::InvalidKeyType {
                key: "Attributes".to_string(),
                expected_type: "Map".to_string(),
                url: String::new(),
            });
        };

        // Yes the BIOS attribute to clear the TPM is called "PendingOperation<something>"
        let Some(name) = attrs_map.keys().find(|k| k.starts_with("PendingOperation")) else {
            return Err(RedfishError::NotSupported(
                "Cannot clear_tpm, PendingOperation BIOS attr missing".to_string(),
            ));
        };

        let body = HashMap::from([("Attributes", HashMap::from([(name, "TPM Clear")]))]);
        let url = format!("Systems/{}/Bios", self.s.system_id());
        self.s.client.patch(&url, body).await.map(|_status_code| ())
    }

    async fn pending(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        let url = format!("Systems/{}/Bios/SD", self.s.system_id());
        // Supermicro doesn't include the Attributes key if there are no pending changes
        self.s
            .pending_attributes(&url)
            .await
            .map(|m| {
                m.into_iter()
                    .collect::<HashMap<String, serde_json::Value>>()
            })
            .or_else(|err| match err {
                RedfishError::MissingKey { .. } => Ok(HashMap::new()),
                err => Err(err),
            })
    }

    // TODO: This resets the pending Bios changes to their default values,
    // but DOES NOT CLEAR THEM. We don't know how to do that, or if Supermicro supports it at all.
    async fn clear_pending(&self) -> Result<(), RedfishError> {
        let url = format!("Systems/{}/Bios/SD", self.s.system_id());
        self.s.clear_pending_with_url(&url).await
    }

    async fn pcie_devices(&self) -> Result<Vec<PCIeDevice>, RedfishError> {
        let Some(chassis_id) = self.get_chassis_all().await?.into_iter().next().take() else {
            return Err(RedfishError::NoContent);
        };
        let url = format!("Chassis/{chassis_id}/PCIeDevices");
        let device_ids = self.s.get_members(&url).await?;
        let mut out = Vec::with_capacity(device_ids.len());
        for device_id in device_ids {
            out.push(self.get_pcie_device(&chassis_id, &device_id).await?);
        }
        Ok(out)
    }

    async fn update_firmware(
        &self,
        firmware: tokio::fs::File,
    ) -> Result<crate::model::task::Task, RedfishError> {
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

    async fn get_update_service(&self) -> Result<UpdateService, RedfishError> {
        self.s.get_update_service().await
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
        self.s
            .change_uefi_password(current_uefi_password, new_uefi_password)
            .await
    }

    async fn change_boot_order(&self, boot_array: Vec<String>) -> Result<(), RedfishError> {
        let body = HashMap::from([("Boot", HashMap::from([("BootOrder", boot_array)]))]);
        let url = format!("Systems/{}", self.s.system_id());
        self.s.client.patch(&url, body).await.map(|_status_code| ())
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
        mac_address: Option<&str>,
    ) -> Result<(), RedfishError> {
        self.s.set_boot_order_dpu_first(mac_address).await
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
        self.set_syslockdown(target).await
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
    async fn machine_setup_attrs(&self) -> Result<Vec<(String, serde_json::Value)>, RedfishError> {
        let mut bios_keys = self.bios_attributes_name_map().await?;
        let mut bios_attrs: Vec<(String, serde_json::Value)> = vec![];

        macro_rules! add_keys {
            ($name:literal, $value:expr) => {
                for real_key in bios_keys.remove($name).unwrap_or(vec![]) {
                    bios_attrs.push((real_key, $value.into()));
                }
            };
        }
        add_keys!("QuietBoot", false);
        add_keys!("Re-tryBoot", "EFI Boot");
        add_keys!("CSMSupport", "Disabled");
        add_keys!("SecureBootEnable", false);

        // Trusted Computing / Provision Support / TXT Support
        add_keys!("TXTSupport", EnabledDisabled::Enabled);

        // registries/BiosAttributeRegistry.1.0.0.json/index.json
        add_keys!("DeviceSelect", "TPM 2.0");

        // Attributes to enable CPU virtualization support for faster VMs
        // Not that some are "Enable" and some are "Enabled". Subtle.
        add_keys!("IntelVTforDirectedI/O(VT-d)", EnableDisable::Enable);
        add_keys!("IntelVirtualizationTechnology", EnableDisable::Enable);
        add_keys!("SR-IOVSupport", EnabledDisabled::Enabled);

        // UEFI NIC boot
        add_keys!("IPv4HTTPSupport", EnabledDisabled::Enabled);
        add_keys!("IPv4PXESupport", EnabledDisabled::Enabled);
        add_keys!("IPv6HTTPSupport", EnabledDisabled::Enabled);
        add_keys!("IPv6PXESupport", EnabledDisabled::Disabled);

        Ok(bios_attrs)
    }

    async fn get_kcs_privilege(&self) -> Result<supermicro::Privilege, RedfishError> {
        let url = format!(
            "Managers/{}/Oem/Supermicro/KCSInterface",
            self.s.manager_id()
        );
        let (_, body): (_, HashMap<String, serde_json::Value>) = self.s.client.get(&url).await?;
        let key = "Privilege";
        let p_str = body
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
        p_str.parse().map_err(|_| RedfishError::InvalidKeyType {
            key: key.to_string(),
            expected_type: "oem::supermicro::Privilege".to_string(),
            url: url.to_string(),
        })
    }

    async fn set_kcs_privilege(
        &self,
        privilege: supermicro::Privilege,
    ) -> Result<(), RedfishError> {
        let url = format!(
            "Managers/{}/Oem/Supermicro/KCSInterface",
            self.s.manager_id()
        );
        let body = HashMap::from([("Privilege", privilege.to_string())]);
        self.s.client.patch(&url, body).await.map(|_status_code| ())
    }

    async fn is_host_interface_enabled(&self) -> Result<bool, RedfishError> {
        let url = format!("Managers/{}/HostInterfaces", self.s.manager_id());
        let host_interface_ids = self.s.get_members(&url).await?;
        let num_interfaces = host_interface_ids.len();
        if num_interfaces != 1 {
            return Err(RedfishError::InvalidValue {
                url,
                field: "Members".to_string(),
                err: InvalidValueError(format!(
                    "Expected a single host interface, found {num_interfaces}"
                )),
            });
        }

        let url = format!(
            "Managers/{}/HostInterfaces/{}",
            self.s.manager_id(),
            host_interface_ids[0]
        );
        let (_, body): (_, HashMap<String, serde_json::Value>) = self.s.client.get(&url).await?;
        let key = "InterfaceEnabled";
        body.get(key)
            .ok_or_else(|| RedfishError::MissingKey {
                key: key.to_string(),
                url: url.to_string(),
            })?
            .as_bool()
            .ok_or_else(|| RedfishError::InvalidKeyType {
                key: key.to_string(),
                expected_type: "bool".to_string(),
                url: url.to_string(),
            })
    }

    // The HostInterface allows remote BMC access
    async fn set_host_interfaces(&self, target: EnabledDisabled) -> Result<(), RedfishError> {
        let url = format!("Managers/{}/HostInterfaces", self.s.manager_id());
        // I have only seen exactly one, but you can't be too careful
        let host_iface_ids = self.s.get_members(&url).await?;
        for iface_id in host_iface_ids {
            self.set_host_interface(&iface_id, target).await?;
        }
        Ok(())
    }

    async fn set_host_interface(
        &self,
        iface_id: &str,
        target: EnabledDisabled,
    ) -> Result<(), RedfishError> {
        let url = format!("Managers/{}/HostInterfaces/{iface_id}", self.s.manager_id());
        let body = HashMap::from([("InterfaceEnabled", target == EnabledDisabled::Enabled)]);
        self.s.client.patch(&url, body).await.map(|_status_code| ())
    }

    async fn get_syslockdown(&self) -> Result<bool, RedfishError> {
        let url = format!(
            "Managers/{}/Oem/Supermicro/SysLockdown",
            self.s.manager_id()
        );
        let (_, body): (_, HashMap<String, serde_json::Value>) = self.s.client.get(&url).await?;
        let key = "SysLockdownEnabled";
        body.get(key)
            .ok_or_else(|| RedfishError::MissingKey {
                key: key.to_string(),
                url: url.to_string(),
            })?
            .as_bool()
            .ok_or_else(|| RedfishError::InvalidKeyType {
                key: key.to_string(),
                expected_type: "bool".to_string(),
                url: url.to_string(),
            })
    }

    async fn set_syslockdown(&self, target: EnabledDisabled) -> Result<(), RedfishError> {
        let url = format!(
            "Managers/{}/Oem/Supermicro/SysLockdown",
            self.s.manager_id()
        );
        let body = HashMap::from([("SysLockdownEnabled", target.is_enabled())]);
        self.s.client.patch(&url, body).await.map(|_status_code| ())
    }

    async fn set_boot_override(&self, target: Boot, once: bool) -> Result<(), RedfishError> {
        let url = format!("Systems/{}", self.s.system_id());
        let boot = boot::Boot {
            boot_source_override_target: Some(match target {
                // In UEFI mode Pxe gets converted to UefiBootNext, but it won't accept
                // UefiBootNext directly.
                Boot::Pxe => boot::BootSourceOverrideTarget::Pxe,
                Boot::HardDisk => boot::BootSourceOverrideTarget::Hdd,
                // For this one to appear you have to set boot_source_override_mode to UEFI and
                // reboot, then choose it, then reboot to use it.
                Boot::UefiHttp => boot::BootSourceOverrideTarget::UefiHttp,
            }),
            boot_source_override_enabled: Some(if once {
                boot::BootSourceOverrideEnabled::Once
            } else {
                boot::BootSourceOverrideEnabled::Continuous
            }),
            boot_source_override_mode: Some(boot::BootSourceOverrideMode::UEFI),
            ..Default::default()
        };
        let body = HashMap::from([("Boot", boot)]);
        self.s.client.patch(&url, body).await.map(|_status_code| ())
    }

    async fn get_boot_order(&self) -> Result<FixedBootOrder, RedfishError> {
        let url = format!(
            "Systems/{}/Oem/Supermicro/FixedBootOrder",
            self.s.system_id()
        );
        let (_, fbo) = self.s.client.get(&url).await?;
        Ok(fbo)
    }

    async fn set_boot_order(&self, target: Boot) -> Result<(), RedfishError> {
        let mut fbo = self.get_boot_order().await?;

        // The network name is not consistent because it includes the interface name
        let Some(network) = fbo
            .fixed_boot_order
            .iter()
            .find(|entry| entry.starts_with(NETWORK))
        else {
            return Err(RedfishError::NotSupported(format!(
                "No match for {NETWORK} in top level boot order"
            )));
        };

        // Make our option the first option, the other one second, and everything else (CD/ROM,
        // USB, etc) disabled.
        let mut order = ["Disabled"].repeat(fbo.fixed_boot_order.len());
        match target {
            Boot::Pxe | Boot::UefiHttp => {
                order[0] = network;
                order[1] = HARD_DISK;
            }
            Boot::HardDisk => {
                order[0] = HARD_DISK;
                order[1] = network;
            }
        }

        // Set the DPU to be the first network device to boot from, for faster boots
        if target != Boot::HardDisk {
            let Some(pos) = fbo
                .uefi_network
                .iter()
                .position(|s| s.contains("UEFI HTTP IPv4 Mellanox"))
            else {
                return Err(RedfishError::NotSupported(
                    "No match for 'UEFI HTTP IPv4 Mellanox' in network boot order".to_string(),
                ));
            };
            fbo.uefi_network.swap(0, pos);
        };

        let url = format!(
            "Systems/{}/Oem/Supermicro/FixedBootOrder",
            self.s.system_id()
        );
        let body = HashMap::from([
            ("FixedBootOrder", order),
            (
                "UEFINetwork",
                fbo.uefi_network.iter().map(|s| s.as_ref()).collect(),
            ),
        ]);
        self.s.client.patch(&url, body).await.map(|_status_code| ())
    }

    async fn get_pcie_device(
        &self,
        chassis_id: &str,
        device_id: &str,
    ) -> Result<PCIeDevice, RedfishError> {
        let url = format!("Chassis/{chassis_id}/PCIeDevices/{device_id}");
        let (_, body): (_, PCIeDevice) = self.s.client.get(&url).await?;
        Ok(body)
    }

    /// Set the DPU to be our first netboot device.
    ///
    /// Callers should usually ignore the error and continue. The HTTP adapter
    /// will only appear after IPv4HTTPSupport bios setting is enabled and the host rebooted.
    /// If the Mellanox adapter is not first everything still works, but boot takes a little longer
    /// because it tries the other adapters too.
    async fn set_mellanox_first(&self) -> Result<(), RedfishError> {
        let mut with_name_match = None; // the ID of the option matching with_name
        let mut ordered = Vec::new(); // the final boot options
        let all = self.s.get_boot_options().await?;
        for b in all.members {
            let id = b.odata_id.split('/').last().unwrap();
            let boot_option = self.s.get_boot_option(id).await?;
            if boot_option
                .display_name
                .contains("UEFI HTTP IPv4 Mellanox Network Adapter")
            {
                with_name_match = Some(boot_option.id);
            } else {
                ordered.push(boot_option.id);
            }
        }
        if with_name_match.is_none() {
            // This happens if IPv4HTTPSupport#00F7 is disabled in the bios
            return Err(RedfishError::NotSupported(
                "No match for Mellanox HTTP adapter boot".to_string(),
            ));
        }
        ordered.insert(0, with_name_match.unwrap());
        self.change_boot_order(ordered).await
    }

    // BIOS attribute names by their clean name.
    // e.g.{ QuietBoot -> [QuietBoot#002E]
    //       TXTSupport -> [TXTSupport#0062, TXTSupport#0072] }
    async fn bios_attributes_name_map(&self) -> Result<HashMap<String, Vec<String>>, RedfishError> {
        let bios_attrs = self.s.bios_attributes().await?;
        let Some(attrs_map) = bios_attrs.as_object() else {
            return Err(RedfishError::InvalidKeyType {
                key: "Attributes".to_string(),
                expected_type: "Map".to_string(),
                url: String::new(),
            });
        };
        let mut by_name: HashMap<String, Vec<String>> = HashMap::with_capacity(attrs_map.len());
        for k in attrs_map.keys() {
            let clean_key = k.split('#').next().unwrap().to_string();
            by_name
                .entry(clean_key)
                .and_modify(|e| e.push(k.clone()))
                .or_insert(vec![k.clone()]);
        }
        Ok(by_name)
    }
}
