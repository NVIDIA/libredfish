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

use reqwest::header::{HeaderMap, HeaderName, IF_MATCH};
use reqwest::Method;
use version_compare::Version;

use crate::model::account_service::ManagerAccount;
use crate::EnabledDisabled::Enabled;
use crate::{
    model::{
        boot::{BootSourceOverrideEnabled, BootSourceOverrideTarget},
        chassis::{Chassis, NetworkAdapter},
        network_device_function::NetworkDeviceFunction,
        oem::nvidia_viking,
        oem::nvidia_viking::{BootDevices, BootDevices::Pxe},
        power::Power,
        secure_boot::SecureBoot,
        sel::{LogEntry, LogEntryCollection},
        service_root::ServiceRoot,
        software_inventory::SoftwareInventory,
        system::PCIeDevices,
        task::Task,
        thermal::Thermal,
        BootOption, ComputerSystem,
        EnableDisable::Enable,
        Manager,
    },
    network::REDFISH_ENDPOINT,
    standard::RedfishStandard,
    Boot, BootOptions, EnabledDisabled, PCIeDevice, PowerState, Redfish, RedfishError, Status,
    StatusInternal, SystemPowerControl,
};
use crate::{MachineSetupDiff, MachineSetupStatus, JobState, RoleId};

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
        self.set_virt_enable().await?;
        self.set_uefi_nic_boot().await?;
        self.set_boot_order(Pxe).await?;
        Ok(())
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

        let virt = self.get_virt_enabled().await?;
        if !virt.is_enabled() {
            diffs.push(MachineSetupDiff {
                key: "virt".to_string(),
                expected: "Enabled".to_string(),
                actual: virt.to_string(),
            });
        }

        let url = format!("Systems/{}/Bios/SD/", self.s.system_id());
        let uefi: nvidia_viking::SetUefiHttpAttributes = self.s.client.get(&url).await?.1;
        let needed = [
            ("Ipv4Http", uefi.attributes.ipv4_http),
            ("Ipv4Pxe", uefi.attributes.ipv4_pxe),
            ("Ipv6Http", uefi.attributes.ipv6_http),
            ("Ipv6Pxe", uefi.attributes.ipv6_pxe),
        ];
        for (name, val) in needed {
            if !val.is_enabled() {
                diffs.push(MachineSetupDiff {
                    key: name.to_string(),
                    expected: "Enabled".to_string(),
                    actual: val.to_string(),
                });
            }
        }

        // TODO: Many BootOptions have Alias="Pxe". This probably isn't doing what we want.
        // see get_boot_options_ids_with_first
        let boot_first = self.s.get_first_boot_option().await?;
        if boot_first.alias != Some(Pxe.to_string()) {
            diffs.push(MachineSetupDiff {
                key: "boot_first".to_string(),
                expected: Pxe.to_string(),
                actual: format!("{:?}", boot_first.alias.as_deref().unwrap_or("_missing_")),
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
        use serde_json::Value;
        let body = HashMap::from([
            ("AccountLockoutThreshold", Value::Number(0.into())),
            ("AccountLockoutDuration", Value::Number(0.into())),
            ("AccountLockoutCounterResetAfter", Value::Number(0.into())),
            ("AccountLockoutCounterResetEnabled", Value::Bool(false)),
            ("AuthFailureLoggingThreshold", Value::Number(0.into())),
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
            Enabled => self.enable_lockdown().await,
            Disabled => self.disable_lockdown().await,
        }
    }

    async fn lockdown_status(&self) -> Result<Status, RedfishError> {
        let url = &format!("Systems/{}/Bios", self.s.system_id());
        let (_status_code, bios): (_, nvidia_viking::Bios) = self.s.client.get(url).await?;
        let bios = bios.attributes;
        let message = format!(
            "ipmi_kcs_disable={}, redfish_enable={}.",
            bios.kcs_interface_disable, bios.redfish_enable
        );
        // todo: fix this once dgx viking team adds support
        Ok(Status {
            message,
            status: if bios.kcs_interface_disable == "Deny All"
            /*&& bios.redfish_enable == Disabled */
            {
                StatusInternal::Enabled
            } else if bios.kcs_interface_disable == "Allow All" && bios.redfish_enable == Enabled {
                StatusInternal::Disabled
            } else {
                StatusInternal::Partial
            },
        })
    }

    async fn setup_serial_console(&self) -> Result<(), RedfishError> {
        let serial_console = nvidia_viking::BiosSerialConsoleAttributes {
            acpi_spcr_baud_rate: "115200".to_string(),
            baud_rate0: "115200".to_string(),
            acpi_spcr_console_redirection_enable: true,
            acpi_spcr_flow_control: "None".to_string(),
            acpi_spcr_port: "COM0".to_string(),
            acpi_spcr_terminal_type: "VT-UTF8".to_string(),
            console_redirection_enable0: true,
            terminal_type0: "ANSI".to_string(),
        };
        let set_serial_attrs = nvidia_viking::SetBiosSerialConsoleAttributes {
            attributes: serial_console,
        };
        let url = format!("Systems/{}/Bios/SD/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_serial_attrs)
            .await
            .map(|_status_code| ())

        // TODO: need to figure out from viking team on patching this:
        // let bmc_serial = nvidia_viking::BmcSerialConsoleAttributes {
        //    bit_rate: "115200".to_string(),
        //    data_bits: "8".to_string(),
        //    flow_control: "None".to_string(),
        //    interface_enabled: true,
        //    parity: "None".to_string(),
        //    stop_bits: "1".to_string(),
        //};
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
        // TODO: possibly remove this redundant matching, the enum is based on the bmc capabilities
        match target {
            Boot::Pxe => self.set_boot_order(BootDevices::Pxe).await,
            Boot::HardDisk => self.set_boot_order(BootDevices::Hdd).await,
            Boot::UefiHttp => self.set_boot_order(BootDevices::UefiHttp).await,
        }
    }

    async fn clear_tpm(&self) -> Result<(), RedfishError> {
        let tpm = nvidia_viking::TpmAttributes {
            tpm_support: Enable,
            tpm_operation: "TPM Clear".to_string(),
        };
        let set_tpm_attrs = nvidia_viking::SetTpmAttributes { attributes: tpm };
        let url = format!("Systems/{}/Bios/SD/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_tpm_attrs)
            .await
            .map(|_status_code| ())
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
        let mut out = Vec::new();

        // viking has pcie devices on the daughterboard that requires enumerating all chassis
        // the structure of pcie devices reported is also different from other vendors
        let chassis_all = self.s.get_chassis_all().await?;
        for chassis_id in chassis_all {
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
                            .clone()
                            .unwrap()
                            .state
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

    async fn update_firmware(&self, firmware: tokio::fs::File) -> Result<Task, RedfishError> {
        self.s.update_firmware(firmware).await
    }

    async fn update_firmware_multipart(
        &self,
        filename: &Path,
        reboot: bool,
        timeout: Duration,
    ) -> Result<String, RedfishError> {
        self.s
            .update_firmware_multipart(filename, reboot, timeout)
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
    ) -> Result<Option<String>, RedfishError> {
        self.s
            .change_bios_password(UEFI_PASSWORD_NAME, current_uefi_password, new_uefi_password)
            .await
    }

    async fn change_boot_order(&self, boot_array: Vec<String>) -> Result<(), RedfishError> {
        let data = HashMap::from([("Boot", HashMap::from([("BootOrder", boot_array)]))]);
        let url = format!("Systems/{}/SD", self.s.system_id());
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
                Some(data),
                Some(timeout),
                None,
                headers,
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

    async fn get_job_state(&self, job_id: &str) -> Result<JobState, RedfishError> {
        self.s.get_job_state(job_id).await
    }
}

impl Bmc {
    async fn check_firmware_version(
        &self,
        firmware_id: String,
        minimum_version: String,
    ) -> Result<(), RedfishError> {
        let firmware = self.get_firmware(&firmware_id).await?;
        if let Some(version) = firmware.version {
            let current = Version::from(&version);
            let minimum = Version::from(&minimum_version);
            if current < minimum {
                return Err(RedfishError::NotSupported(format!(
                    "{firmware_id} {version} < {minimum_version}"
                )));
            }
            return Ok(());
        }
        Err(RedfishError::NotSupported(format!(
            "{firmware_id} unknown version < {minimum_version}"
        )))
    }

    async fn enable_lockdown(&self) -> Result<(), RedfishError> {
        // assuming that the viking bmc does not modify the suffixes
        self.check_firmware_version("HostBIOS_0".to_string(), "1.01.03".to_string())
            .await?;
        self.check_firmware_version("HostBMC_0".to_string(), "23.11.09".to_string())
            .await?;

        let lockdown_attrs = nvidia_viking::BiosLockdownAttributes {
            kcs_interface_disable: "Deny All".to_string(),
            redfish_enable: Enabled, // todo: this should be disabled for the virtual usb nic, not yet implemented by dgx team
        };
        let set_lockdown = nvidia_viking::SetBiosLockdownAttributes {
            attributes: lockdown_attrs,
        };
        let url = format!("Systems/{}/Bios/SD/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_lockdown)
            .await
            .map(|_status_code| ())
    }

    async fn disable_lockdown(&self) -> Result<(), RedfishError> {
        let lockdown_attrs = nvidia_viking::BiosLockdownAttributes {
            kcs_interface_disable: "Allow All".to_string(),
            redfish_enable: Enabled,
        };
        let set_lockdown = nvidia_viking::SetBiosLockdownAttributes {
            attributes: lockdown_attrs,
        };
        let url = format!("Systems/{}/Bios/SD/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_lockdown)
            .await
            .map(|_status_code| ())
    }

    async fn set_virt_enable(&self) -> Result<(), RedfishError> {
        let virt_attrs = nvidia_viking::VirtAttributes {
            sriov_enable: Enable,
            vtd_support: Enable,
        };
        let set_virt_attrs = nvidia_viking::SetVirtAttributes {
            attributes: virt_attrs,
        };
        let url = format!("Systems/{}/Bios/SD/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_virt_attrs)
            .await
            .map(|_status_code| ())
    }

    async fn get_virt_enabled(&self) -> Result<EnabledDisabled, RedfishError> {
        let url = format!("Systems/{}/Bios/SD/", self.s.system_id());
        let virt: nvidia_viking::SetVirtAttributes = self.s.client.get(&url).await?.1;
        if virt.attributes.sriov_enable.is_enabled() && virt.attributes.vtd_support.is_enabled() {
            Ok(EnabledDisabled::Enabled)
        } else {
            Ok(EnabledDisabled::Disabled)
        }
    }

    async fn set_uefi_nic_boot(&self) -> Result<(), RedfishError> {
        let uefi_nic_boot = nvidia_viking::UefiHttpAttributes {
            ipv4_http: Enabled,
            ipv4_pxe: Enabled,
            ipv6_http: Enabled,
            ipv6_pxe: Enabled,
        };
        let set_uefi_nic_boot = nvidia_viking::SetUefiHttpAttributes {
            attributes: uefi_nic_boot,
        };
        let url = format!("Systems/{}/Bios/SD/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_uefi_nic_boot)
            .await
            .map(|_status_code| ())
    }

    async fn bios_serial_console_status(&self) -> Result<Status, RedfishError> {
        let mut message = String::new();

        let mut enabled = true;
        let mut disabled = true;

        let url = &format!("Systems/{}/Bios", self.s.system_id());
        let (_status_code, bios): (_, nvidia_viking::Bios) = self.s.client.get(url).await?;
        let bios = bios.attributes;

        let val = bios.acpi_spcr_console_redirection_enable;
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

        let val = bios.console_redirection_enable0;
        message.push_str(&format!("console_redirection_enable0={val} "));
        match val {
            true => {
                disabled = false;
            }
            false => {
                enabled = false;
            }
        }

        // All of these need a specific value for serial console access to work.
        // Any other value counts as correctly disabled.

        let val = bios.acpi_spcr_port;
        message.push_str(&format!("acpi_spcr_port={val} "));
        if &val != "COM0" {
            enabled = false;
        }

        let val = bios.acpi_spcr_flow_control;
        message.push_str(&format!("acpi_spcr_flow_control={val} "));
        if &val != "None" {
            enabled = false;
        }

        let val = bios.acpi_spcr_baud_rate;
        message.push_str(&format!("acpi_spcr_baud_rate={val} "));
        if &val != "115200" {
            enabled = false;
        }

        let val = bios.baud_rate0;
        message.push_str(&format!("baud_rate0={val} "));
        if &val != "115200" {
            enabled = false;
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
            if b.alias.is_some() && b.alias.unwrap() == with_name_str {
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
}
