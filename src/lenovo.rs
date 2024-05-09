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

use reqwest::header::HeaderMap;
use reqwest::Method;
use tracing::debug;

use crate::model::account_service::ManagerAccount;
use crate::model::service_root::ServiceRoot;
use crate::model::task::Task;
use crate::model::Manager;
use crate::model::{secure_boot::SecureBoot, ComputerSystem};
use crate::EnabledDisabled::Enabled;
use crate::RoleId;
use crate::{
    model::{
        chassis::{Chassis, NetworkAdapter},
        network_device_function::NetworkDeviceFunction,
        oem::lenovo,
        power::Power,
        sel::{LogEntry, LogEntryCollection},
        software_inventory::SoftwareInventory,
        thermal::Thermal,
        BootOption,
    },
    network::REDFISH_ENDPOINT,
    standard::RedfishStandard,
    Boot, BootOptions, EnabledDisabled, PCIeDevice, PowerState, Redfish, RedfishError, Status,
    StatusInternal, SystemPowerControl,
};

const UEFI_PASSWORD_NAME: &str = "UefiAdminPassword";

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

    async fn change_password(&self, user: &str, new: &str) -> Result<(), RedfishError> {
        self.s.change_password(user, new).await
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

    async fn get_thermal_metrics(&self) -> Result<Thermal, RedfishError> {
        self.s.get_thermal_metrics().await
    }

    async fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError> {
        self.get_system_event_log().await
    }

    async fn bios(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        self.s.bios().await
    }

    async fn machine_setup(&self) -> Result<(), RedfishError> {
        self.setup_serial_console().await?;
        self.clear_tpm().await?;
        self.boot_first(Boot::Pxe).await?;
        self.set_virt_enable().await?;
        self.set_uefi_boot_only().await?;
        // always do system lockdown last
        self.lockdown(Enabled).await
    }

    /// Redfish equivalent of `accseccfg -pew 0 -pe 0 -chgnew off -rc 0 -ci 0 -lf 0`
    async fn set_machine_password_policy(&self) -> Result<(), RedfishError> {
        use serde_json::Value;
        let mut body = HashMap::from([
            (
                "AccountLockoutThreshold".to_string(),
                Value::Number(0.into()),
            ), // -lf 0
            (
                "AccountLockoutDuration".to_string(),
                // 60 secs is the shortest Lenovo allows. The docs say 0 disables it, but my
                // test Lenovo rejects 0.
                Value::Number(60.into()),
            ),
        ]);
        let lenovo = Value::Object(serde_json::Map::from_iter(vec![
            (
                "PasswordExpirationPeriodDays".to_string(),
                Value::Number(0.into()),
            ), // -pe 0
            (
                "PasswordChangeOnFirstAccess".to_string(),
                Value::Bool(false),
            ), // -chgnew off
            (
                "MinimumPasswordChangeIntervalHours".to_string(),
                Value::Number(0.into()),
            ), // -ci 0
            (
                "MinimumPasswordReuseCycle".to_string(),
                Value::Number(0.into()),
            ), // -rc 0
            (
                "PasswordExpirationWarningPeriod".to_string(),
                Value::Number(0.into()),
            ), // -pew 0
        ]));
        let mut oem = serde_json::Map::new();
        oem.insert("Lenovo".to_string(), lenovo);
        body.insert("Oem".to_string(), serde_json::Value::Object(oem));

        self.s
            .client
            .patch("AccountService", body)
            .await
            .map(|_status_code| ())
    }

    async fn lockdown(&self, target: EnabledDisabled) -> Result<(), RedfishError> {
        use EnabledDisabled::*;
        match target {
            Enabled => self.enable_lockdown().await,
            Disabled => self.disable_lockdown().await,
        }
    }

    async fn lockdown_status(&self) -> Result<Status, RedfishError> {
        let kcs = self.get_kcs_lenovo().await?;
        let firmware_rollback = self.get_firmware_rollback_lenovo().await?;
        let eth_usb = self.get_ethernet_over_usb().await?;
        let front_usb = self.get_front_panel_usb_lenovo().await?;

        let message = format!(
            "kcs={kcs}, firmware_rollback={firmware_rollback}, ethernet_over_usb={eth_usb}, front_panel_usb={}/{}",
            front_usb.fp_mode, front_usb.port_switching_to,
        );

        let is_locked = !kcs
            && !eth_usb
            && firmware_rollback == EnabledDisabled::Disabled
            && front_usb.fp_mode == lenovo::FrontPanelUSBMode::Server;

        let is_unlocked = kcs
            && eth_usb
            && firmware_rollback == EnabledDisabled::Enabled
            && front_usb.fp_mode == lenovo::FrontPanelUSBMode::Shared
            && front_usb.port_switching_to == lenovo::PortSwitchingMode::Server;

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

    async fn setup_serial_console(&self) -> Result<(), RedfishError> {
        let mut body = HashMap::new();
        body.insert(
            "Attributes",
            HashMap::from([
                (
                    "DevicesandIOPorts_COMPort1",
                    EnabledDisabled::Enabled.to_string(),
                ),
                (
                    "DevicesandIOPorts_ConsoleRedirection",
                    "Enabled".to_string(), // not an EnabledDisabled, can be "Auto"
                ),
                (
                    "DevicesandIOPorts_SPRedirection",
                    EnabledDisabled::Enabled.to_string(),
                ),
                (
                    "DevicesandIOPorts_SerialPortSharing",
                    EnabledDisabled::Enabled.to_string(),
                ),
                (
                    "DevicesandIOPorts_COMPortActiveAfterBoot",
                    EnabledDisabled::Enabled.to_string(),
                ),
                (
                    "DevicesandIOPorts_SerialPortAccessMode",
                    "Shared".to_string(),
                ),
            ]),
        );
        let url = format!("Systems/{}/Bios/Pending", self.s.system_id());
        self.s.client.patch(&url, body).await.map(|_status_code| ())
    }

    async fn serial_console_status(&self) -> Result<Status, RedfishError> {
        let bios = self.bios().await?;
        let attrs = bios
            .get("Attributes")
            .ok_or_else(|| RedfishError::MissingKey {
                key: "Attributes".to_string(),
                url: format!("Systems/{}/Bios", self.s.system_id()),
            })?
            .as_object()
            .ok_or_else(|| RedfishError::InvalidKeyType {
                key: "Attributes".to_string(),
                expected_type: "Object".to_string(),
                url: format!("Systems/{}/Bios", self.s.system_id()),
            })?;

        let expected = vec![
            // "any" means any value counts as correctly disabled
            ("DevicesandIOPorts_COMPort1", "Enabled", "any"),
            ("DevicesandIOPorts_ConsoleRedirection", "Enabled", "Auto"),
            ("DevicesandIOPorts_SPRedirection", "Enabled", "Disabled"),
            ("DevicesandIOPorts_SerialPortSharing", "Enabled", "Disabled"),
            (
                "DevicesandIOPorts_COMPortActiveAfterBoot",
                "Enabled",
                "Disabled",
            ),
            (
                "DevicesandIOPorts_SerialPortAccessMode",
                "Shared",
                "Disabled",
            ),
        ];
        let mut message = String::new();
        let mut enabled = true;
        let mut disabled = true;
        let url = format!("Systems/{}/Bios", self.s.system_id()); // url for debug only
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

    async fn get_boot_options(&self) -> Result<BootOptions, RedfishError> {
        self.s.get_boot_options().await
    }

    async fn get_boot_option(&self, option_id: &str) -> Result<BootOption, RedfishError> {
        self.s.get_boot_option(option_id).await
    }

    async fn boot_once(&self, target: Boot) -> Result<(), RedfishError> {
        match target {
            Boot::Pxe => self.set_boot_override(lenovo::BootSource::Pxe).await,
            Boot::HardDisk => self.set_boot_override(lenovo::BootSource::Hdd).await,
            Boot::UefiHttp => Err(RedfishError::NotSupported(
                "No Lenovo UefiHttp implementation".to_string(),
            )),
        }
    }

    async fn boot_first(&self, target: Boot) -> Result<(), RedfishError> {
        match target {
            Boot::Pxe => self.set_boot_first(lenovo::BootOptionName::Network).await,
            Boot::HardDisk => self.set_boot_first(lenovo::BootOptionName::HardDisk).await,
            Boot::UefiHttp => Err(RedfishError::NotSupported(
                "No Lenovo UefiHttp implementation".to_string(),
            )),
        }
    }

    async fn clear_tpm(&self) -> Result<(), RedfishError> {
        let mut body = HashMap::new();
        body.insert(
            "Attributes",
            HashMap::from([("TrustedComputingGroup_DeviceOperation", "Clear")]),
        );
        let url = format!("Systems/{}/Bios/Pending", self.s.system_id());
        self.s.client.patch(&url, body).await.map(|_status_code| ())
    }

    async fn pending(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        let url = format!("Systems/{}/Bios/Pending", self.s.system_id());
        self.s.pending_with_url(&url).await
    }

    async fn clear_pending(&self) -> Result<(), RedfishError> {
        let url = format!("Systems/{}/Bios/Pending", self.s.system_id());
        self.s.clear_pending_with_url(&url).await
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

    async fn update_firmware_multipart(
        &self,
        filename: &Path,
        reboot: bool,
    ) -> Result<String, RedfishError> {
        self.s.update_firmware_multipart(filename, reboot).await
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
    ) -> Result<NetworkDeviceFunction, RedfishError> {
        self.s.get_network_device_function(chassis_id, id).await
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
    ) -> Result<(), RedfishError> {
        self.s
            .change_bios_password(UEFI_PASSWORD_NAME, current_uefi_password, new_uefi_password)
            .await
    }

    async fn change_boot_order(&self, boot_array: Vec<String>) -> Result<(), RedfishError> {
        let body = HashMap::from([("Boot", HashMap::from([("BootOrder", boot_array)]))]);
        let url = format!("Systems/{}/Pending", self.s.system_id());
        // BMC takes longer to respond to this one, so override timeout
        let timeout = Duration::from_secs(10);
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
                Some(body),
                Some(timeout),
                None,
                Vec::new(),
            )
            .await?;
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
}

impl Bmc {
    /// Lock a Lenovo server to make it ready for tenants
    async fn enable_lockdown(&self) -> Result<(), RedfishError> {
        self.set_kcs_lenovo(false).await.map_err(|e| {
            debug!("Failed disabling 'IPMI over KCS Access'");
            e
        })?;
        self.set_firmware_rollback_lenovo(EnabledDisabled::Disabled)
            .await
            .map_err(|e| {
                debug!("Failed changing 'Prevent System Firmware Down-Level'");
                e
            })?;
        self.set_ethernet_over_usb(false).await.map_err(|e| {
            debug!("Failed disabling Ethernet over USB");
            e
        })?;
        self.set_front_panel_usb_lenovo(
            lenovo::FrontPanelUSBMode::Server,
            lenovo::PortSwitchingMode::Server,
        )
        .await
        .map_err(|e| {
            debug!("Failed locking front panel USB to host-only.");
            e
        })?;
        Ok(())
    }

    /// Unlock a Lenovo server, restoring defaults
    pub async fn disable_lockdown(&self) -> Result<(), RedfishError> {
        self.set_kcs_lenovo(true).await.map_err(|e| {
            debug!("Failed enabling 'IPMI over KCS Access'");
            e
        })?;
        self.set_firmware_rollback_lenovo(EnabledDisabled::Enabled)
            .await
            .map_err(|e| {
                debug!("Failed changing 'Prevent System Firmware Down-Level'");
                e
            })?;
        self.set_ethernet_over_usb(true).await.map_err(|e| {
            debug!("Failed disabling Ethernet over USB");
            e
        })?;
        self.set_front_panel_usb_lenovo(
            lenovo::FrontPanelUSBMode::Shared,
            lenovo::PortSwitchingMode::Server,
        )
        .await
        .map_err(|e| {
            debug!("Failed unlocking front panel USB to shared mode.");
            e
        })?;
        Ok(())
    }

    async fn set_kcs_lenovo(&self, is_allowed: bool) -> Result<(), RedfishError> {
        let body = HashMap::from([(
            "Oem",
            HashMap::from([("Lenovo", HashMap::from([("KCSEnabled", is_allowed)]))]),
        )]);
        let url = format!("Managers/{}", self.s.manager_id());
        self.s.client.patch(&url, body).await.map(|_status_code| ())
    }

    async fn get_kcs_lenovo(&self) -> Result<bool, RedfishError> {
        let url = format!("Managers/{}", self.s.manager_id());
        let (_, body): (_, HashMap<String, serde_json::Value>) = self.s.client.get(&url).await?;

        let key = "Oem";
        let oem_obj = body
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
            })?;

        let key = "Lenovo";
        let lenovo_obj = oem_obj
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
            })?;

        let key = "KCSEnabled";
        let is_kcs_enabled = lenovo_obj
            .get(key)
            .ok_or_else(|| RedfishError::MissingKey {
                key: key.to_string(),
                url: url.to_string(),
            })?
            .as_bool()
            .ok_or_else(|| RedfishError::InvalidKeyType {
                key: key.to_string(),
                expected_type: "bool".to_string(),
                url: url.to_string(),
            })?;

        Ok(is_kcs_enabled)
    }

    async fn set_firmware_rollback_lenovo(&self, set: EnabledDisabled) -> Result<(), RedfishError> {
        let body = HashMap::from([(
            "Configurator",
            HashMap::from([("FWRollback", set.to_string())]),
        )]);
        let url = format!("Managers/{}/Oem/Lenovo/Security", self.s.manager_id());
        self.s.client.patch(&url, body).await.map(|_status_code| ())
    }

    async fn get_firmware_rollback_lenovo(&self) -> Result<EnabledDisabled, RedfishError> {
        let url = format!("Managers/{}/Oem/Lenovo/Security", self.s.manager_id());
        let (_, body): (_, HashMap<String, serde_json::Value>) = self.s.client.get(&url).await?;

        let key = "Configurator";
        let configurator = body
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
            })?;

        let key = "FWRollback";
        let fw_rollback = configurator
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

        let fw_typed = fw_rollback
            .parse()
            .map_err(|_| RedfishError::InvalidKeyType {
                key: key.to_string(),
                expected_type: "EnabledDisabled".to_string(),
                url: url.to_string(),
            })?;
        Ok(fw_typed)
    }

    async fn set_front_panel_usb_lenovo(
        &self,
        mode: lenovo::FrontPanelUSBMode,
        owner: lenovo::PortSwitchingMode,
    ) -> Result<(), RedfishError> {
        let mut body = HashMap::new();
        body.insert(
            "Oem",
            HashMap::from([(
                "Lenovo",
                HashMap::from([(
                    "FrontPanelUSB",
                    HashMap::from([
                        ("FPMode", mode.to_string()),
                        ("PortSwitchingTo", owner.to_string()),
                    ]),
                )]),
            )]),
        );
        let url = format!("Systems/{}", self.s.system_id());
        self.s.client.patch(&url, body).await.map(|_status_code| ())
    }

    async fn get_front_panel_usb_lenovo(&self) -> Result<lenovo::FrontPanelUSB, RedfishError> {
        let url = format!("Systems/{}", self.s.system_id());
        let (_, body): (_, HashMap<String, serde_json::Value>) = self.s.client.get(&url).await?;

        let key = "Oem";
        let oem_obj = body
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
            })?;

        let key = "Lenovo";
        let lenovo_obj = oem_obj
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
            })?;

        let key = "FrontPanelUSB";
        let fp_usb_val = lenovo_obj
            .get(key)
            .ok_or_else(|| RedfishError::MissingKey {
                key: key.to_string(),
                url: url.to_string(),
            })?;
        let fp_usb = serde_json::from_value(fp_usb_val.clone()).map_err(|err| {
            RedfishError::JsonDeserializeError {
                url,
                body: format!("{fp_usb_val:?}"),
                source: err,
            }
        })?;

        Ok(fp_usb)
    }

    async fn set_ethernet_over_usb(&self, is_allowed: bool) -> Result<(), RedfishError> {
        let body = HashMap::from([("InterfaceEnabled", is_allowed)]);
        let url = format!("Managers/{}/EthernetInterfaces/ToHost", self.s.manager_id());
        self.s.client.patch(&url, body).await.map(|_status_code| ())
    }

    async fn get_ethernet_over_usb(&self) -> Result<bool, RedfishError> {
        let url = format!("Managers/{}/EthernetInterfaces/ToHost", self.s.manager_id());
        let (_, body): (_, HashMap<String, serde_json::Value>) = self.s.client.get(&url).await?;

        let key = "InterfaceEnabled";
        let is_allowed = body
            .get(key)
            .ok_or_else(|| RedfishError::MissingKey {
                key: key.to_string(),
                url: url.to_string(),
            })?
            .as_bool()
            .ok_or_else(|| RedfishError::InvalidKeyType {
                key: key.to_string(),
                expected_type: "bool".to_string(),
                url: url.to_string(),
            })?;
        Ok(is_allowed)
    }

    async fn set_virt_enable(&self) -> Result<(), RedfishError> {
        let mut body = HashMap::new();
        body.insert(
            "Attributes",
            HashMap::from([("Processors_IntelVirtualizationTechnology", "Enabled")]),
        );
        let url = format!("Systems/{}/Bios/Pending", self.s.system_id());
        self.s.client.patch(&url, body).await.map(|_status_code| ())
    }

    /// Set so that we only UEFI IPv4 HTTP boot, and we retry that.
    ///
    /// Disable PXE Boot
    /// Disable LegacyBIOS Mode
    /// Set Bootmode to UEFI
    /// Enable IPv4 HTTP Boot
    /// Disable IPv4 PXE Boot
    /// Disable IPv6 PXE Boot
    /// Enable Infinite Boot Mode
    async fn set_uefi_boot_only(&self) -> Result<(), RedfishError> {
        let mut body = HashMap::new();
        body.insert(
            "Attributes",
            HashMap::from([
                ("LegacyBIOS_NonOnboardPXE", "Disabled"),
                ("LegacyBIOS_LegacyBIOS", "Disabled"),
                ("BootModes_SystemBootMode", "UEFIMode"),
                ("NetworkStackSettings_IPv4HTTPSupport", "Enabled"),
                ("NetworkStackSettings_IPv4PXESupport", "Disabled"),
                ("NetworkStackSettings_IPv6PXESupport", "Disabled"),
                ("BootModes_InfiniteBootRetry", "Enabled"),
            ]),
        );
        let url = format!("Systems/{}/Bios/Pending", self.s.system_id());
        self.s.client.patch(&url, body).await.map(|_status_code| ())
    }

    async fn set_boot_override(&self, target: lenovo::BootSource) -> Result<(), RedfishError> {
        let target_str = &target.to_string();
        let body = HashMap::from([(
            "Boot",
            HashMap::from([
                ("BootSourceOverrideEnabled", "Once"),
                ("BootSourceOverrideTarget", target_str),
            ]),
        )]);
        let url = format!("Systems/{}", self.s.system_id());
        self.s.client.patch(&url, body).await.map(|_status_code| ())
    }

    // name: The name of the device you want to make the first boot choice.
    //
    // Note that _within_ the type you choose you could also give the order. e.g for "Network"
    // see Systems/1/Oem/Lenovo/BootSettings/BootOrder.NetworkBootOrder
    // and for "HardDisk" see Systems/1/Oem/Lenovo/BootSettings/BootOrder.HardDiskBootOrder
    async fn set_boot_first(&self, name: lenovo::BootOptionName) -> Result<(), RedfishError> {
        let boot_array = match self.get_boot_options_ids_with_first(name).await? {
            None => {
                return Err(RedfishError::MissingBootOption(name.to_string()));
            }
            Some(b) => b,
        };

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
        with_name: lenovo::BootOptionName,
    ) -> Result<Option<Vec<String>>, RedfishError> {
        let with_name_str = with_name.to_string();
        let mut with_name_match = None; // the ID of the option matching with_name
        let mut ordered = Vec::new(); // the final boot options
        let boot_options = self.s.get_boot_options().await?;
        for member in boot_options.members {
            let url = member
                .odata_id
                .replace(&format!("/{REDFISH_ENDPOINT}/"), "");
            let b: BootOption = self.s.client.get(&url).await?.1;
            if b.name == with_name_str {
                with_name_match = Some(b.id);
            } else {
                ordered.push(b.id);
            }
        }
        match with_name_match {
            None => Ok(None),
            Some(with_name_id) => {
                ordered.insert(0, with_name_id);
                Ok(Some(ordered))
            }
        }
    }

    // lenovo stores the sel as part of the system
    async fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError> {
        let url = format!("Systems/{}/LogServices/SEL/Entries", self.s.system_id());
        let (_status_code, log_entry_collection): (_, LogEntryCollection) =
            self.s.client.get(&url).await?;
        let log_entries = log_entry_collection.members;
        Ok(log_entries)
    }
}
