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

use reqwest::StatusCode;
use serde::Serialize;
use tokio::fs::File;

use crate::{
    model::{
        account_service::ManagerAccount,
        boot::{
            self, BootOverride, BootSourceOverrideEnabled, BootSourceOverrideMode,
            BootSourceOverrideTarget,
        },
        certificate::Certificate,
        chassis::{Assembly, Chassis, NetworkAdapter},
        component_integrity::ComponentIntegrities,
        network_device_function::NetworkDeviceFunction,
        oem::{
            nvidia_dpu::{HostPrivilegeLevel, NicMode},
            supermicro::{self, FixedBootOrder},
        },
        power::Power,
        secure_boot::SecureBoot,
        sel::LogEntry,
        sensor::GPUSensors,
        service_root::{RedfishVendor, ServiceRoot},
        software_inventory::SoftwareInventory,
        storage::Drives,
        task::Task,
        thermal::Thermal,
        update_service::{ComponentType, TransferProtocolType, UpdateService},
        BootOption, ComputerSystem, EnableDisable, InvalidValueError, Manager,
    },
    standard::RedfishStandard,
    BiosProfileType, Boot, BootOptions, Collection, EnabledDisabled, JobState, MachineSetupDiff,
    MachineSetupStatus, ODataId, PCIeDevice, PowerState, Redfish, RedfishError, Resource, RoleId,
    Status, StatusInternal, SystemPowerControl,
};

const MELLANOX_UEFI_HTTP_IPV4: &str = "UEFI HTTP IPv4 Mellanox Network Adapter";
const NVIDIA_UEFI_HTTP_IPV4: &str = "UEFI HTTP IPv4 Nvidia Network Adapter";
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
impl Redfish for Bmc {
    fn create_user<'a>(
        &'a self,
        username: &'a str,
        password: &'a str,
        role_id: RoleId,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.create_user(username, password, role_id).await })
    }

    fn delete_user<'a>(
        &'a self,
        username: &'a str,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.delete_user(username).await })
    }

    fn change_username<'a>(
        &'a self,
        old_name: &'a str,
        new_name: &'a str,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.change_username(old_name, new_name).await })
    }

    fn change_password<'a>(
        &'a self,
        username: &'a str,
        new_password: &'a str,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.change_password(username, new_password).await })
    }

    fn change_password_by_id<'a>(
        &'a self,
        account_id: &'a str,
        new_pass: &'a str,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.change_password_by_id(account_id, new_pass).await })
    }

    fn get_accounts<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Vec<ManagerAccount>, RedfishError>> {
        Box::pin(async move { self.s.get_accounts().await })
    }

    fn get_power_state<'a>(&'a self) -> crate::RedfishFuture<'a, Result<PowerState, RedfishError>> {
        Box::pin(async move { self.s.get_power_state().await })
    }

    fn get_power_metrics<'a>(&'a self) -> crate::RedfishFuture<'a, Result<Power, RedfishError>> {
        Box::pin(async move { self.s.get_power_metrics().await })
    }

    fn power<'a>(
        &'a self,
        action: SystemPowerControl,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            if action == SystemPowerControl::ACPowercycle {
                let args: HashMap<String, String> =
                    HashMap::from([("ResetType".to_string(), "ACCycle".to_string())]);
                let url = format!(
                    "Systems/{}/Actions/Oem/OemSystemExtensions.Reset",
                    self.s.system_id()
                );
                return self.s.client.post(&url, args).await.map(|_status_code| ());
            }
            self.s.power(action).await
        })
    }

    fn ac_powercycle_supported_by_power(&self) -> bool {
        true
    }

    fn bmc_reset<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.bmc_reset().await })
    }

    fn chassis_reset<'a>(
        &'a self,
        chassis_id: &'a str,
        reset_type: SystemPowerControl,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.chassis_reset(chassis_id, reset_type).await })
    }

    fn get_thermal_metrics<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Thermal, RedfishError>> {
        Box::pin(async move { self.s.get_thermal_metrics().await })
    }

    fn get_gpu_sensors<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Vec<GPUSensors>, RedfishError>> {
        Box::pin(async move { self.s.get_gpu_sensors().await })
    }

    fn get_system_event_log<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Vec<LogEntry>, RedfishError>> {
        Box::pin(async move { self.s.get_system_event_log().await })
    }

    fn get_bmc_event_log<'a>(
        &'a self,
        from: Option<chrono::DateTime<chrono::Utc>>,
    ) -> crate::RedfishFuture<'a, Result<Vec<LogEntry>, RedfishError>> {
        Box::pin(async move { self.s.get_bmc_event_log(from).await })
    }

    fn get_drives_metrics<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Vec<Drives>, RedfishError>> {
        Box::pin(async move { self.s.get_drives_metrics().await })
    }

    fn bios<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<HashMap<String, serde_json::Value>, RedfishError>> {
        Box::pin(async move { self.s.bios().await })
    }

    fn set_bios<'a>(
        &'a self,
        values: HashMap<String, serde_json::Value>,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.set_bios(values).await })
    }

    fn reset_bios<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.factory_reset_bios().await })
    }

    /// Note that you can't use this for initial setup unless you reboot and run it twice.
    /// `boot_first` won't find the Mellanox HTTP device. `uefi_nic_boot_attrs` enables it,
    /// but it won't show until after reboot so that step will fail on first time through.
    fn machine_setup<'a>(
        &'a self,
        _boot_interface_mac: Option<&'a str>,
        _bios_profiles: &'a HashMap<
            RedfishVendor,
            HashMap<String, HashMap<BiosProfileType, HashMap<String, serde_json::Value>>>,
        >,
        _selected_profile: BiosProfileType,
        _oem_manager_profiles: &'a HashMap<
            RedfishVendor,
            HashMap<String, HashMap<BiosProfileType, HashMap<String, serde_json::Value>>>,
        >,
    ) -> crate::RedfishFuture<'a, Result<Option<String>, RedfishError>> {
        Box::pin(async move {
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
                .map(|_status_code| None)
        })
    }

    fn machine_setup_status<'a>(
        &'a self,
        boot_interface_mac: Option<&'a str>,
    ) -> crate::RedfishFuture<'a, Result<MachineSetupStatus, RedfishError>> {
        Box::pin(async move {
            // Check BIOS and BMC attributes
            let mut diffs = self.diff_bios_bmc_attr().await?;

            // Check the first boot option
            if let Some(mac) = boot_interface_mac {
                let (expected, actual) =
                    self.get_expected_and_actual_first_boot_option(mac).await?;
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
        })
    }

    fn set_machine_password_policy<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
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
        })
    }

    fn lockdown<'a>(
        &'a self,
        target: EnabledDisabled,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            use EnabledDisabled::*;
            match target {
                Enabled => {
                    // Grace-Grace SMCs can't PXE boot if host interface is disabled
                    if !self.is_grace_grace_smc().await? {
                        self.set_host_interfaces(Disabled).await?;
                    }
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
        })
    }

    fn lockdown_status<'a>(&'a self) -> crate::RedfishFuture<'a, Result<Status, RedfishError>> {
        Box::pin(async move {
            let is_hi_on = self.is_host_interface_enabled().await?;
            let kcs_privilege = match self.get_kcs_privilege().await {
                Ok(priviledge) => Ok(Some(priviledge)),
                Err(e) => {
                    // The Grace-Grace Supermicros in our GB200 lab do not seem to support
                    // querying KCS access from the host to its BMC. Use this workaround to
                    // temporarily enable ingesting these servers.
                    if e.not_found() {
                        Ok(None)
                    } else {
                        Err(e)
                    }
                }
            }?;

            let is_syslockdown = self.get_syslockdown().await?;
            let message = format!("SysLockdownEnabled={is_syslockdown}, kcs_privilege={kcs_privilege:#?}, host_interface_enabled={is_hi_on}");

            // Grace-Grace SMCs (ARS-121L-DNR) need host_interface enabled even with lockdown
            let is_grace_grace = self.is_grace_grace_smc().await?;

            let is_locked = is_syslockdown
                && kcs_privilege
                    .clone()
                    .unwrap_or(supermicro::Privilege::Callback)
                    == supermicro::Privilege::Callback
                && (is_grace_grace || !is_hi_on);
            let is_unlocked = !is_syslockdown
                && kcs_privilege.unwrap_or(supermicro::Privilege::Administrator)
                    == supermicro::Privilege::Administrator
                && is_hi_on;
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
        })
    }

    /// On Supermicro this does nothing. Serial Console is on by default and can't be disabled
    /// or enabled via redfish. The properties under Systems/1, key SerialConsole are read only.
    fn setup_serial_console<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { Ok(()) })
    }

    fn serial_console_status<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Status, RedfishError>> {
        Box::pin(async move {
            let s_interface = self.s.get_serial_interface().await?;
            let system = self.s.get_system().await?;
            let Some(sr) = &system.serial_console else {
                return Err(RedfishError::NotSupported(
                "No SerialConsole in System object. Maybe it's in Manager and you have old firmware?".to_string(),
            ));
            };
            let is_enabled = sr.ssh.service_enabled
                && sr.max_concurrent_sessions != Some(0)
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
        })
    }

    fn get_boot_options<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<BootOptions, RedfishError>> {
        Box::pin(async move { self.s.get_boot_options().await })
    }

    fn get_boot_option<'a>(
        &'a self,
        option_id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<BootOption, RedfishError>> {
        Box::pin(async move { self.s.get_boot_option(option_id).await })
    }

    /// Boot from this device once then go back to the normal boot order
    fn boot_once<'a>(&'a self, target: Boot) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            Redfish::set_boot_override(
                self,
                BootOverride {
                    target: match target {
                        Boot::Pxe => BootSourceOverrideTarget::Pxe,
                        Boot::HardDisk => BootSourceOverrideTarget::Hdd,
                        Boot::UefiHttp => BootSourceOverrideTarget::UefiHttp,
                    },
                    enabled: BootSourceOverrideEnabled::Once,
                    mode: None,
                    http_boot_uri: None,
                },
            )
            .await?;
            Ok(())
        })
    }

    /// Set which device we should boot from first.
    fn boot_first<'a>(
        &'a self,
        target: Boot,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            // Try with FixedBootOptions and fallback to BootOptions if fails
            match self.set_boot_order(target).await {
                Err(RedfishError::HTTPErrorCode {
                    status_code: StatusCode::NOT_FOUND,
                    ..
                }) => {
                    Redfish::set_boot_override(
                        self,
                        BootOverride {
                            target: match target {
                                Boot::Pxe => BootSourceOverrideTarget::Pxe,
                                Boot::HardDisk => BootSourceOverrideTarget::Hdd,
                                Boot::UefiHttp => BootSourceOverrideTarget::UefiHttp,
                            },
                            enabled: BootSourceOverrideEnabled::Continuous,
                            mode: None,
                            http_boot_uri: None,
                        },
                    )
                    .await?;
                    Ok(())
                }
                res => res,
            }
        })
    }

    fn set_boot_override<'a>(
        &'a self,
        settings: BootOverride,
    ) -> crate::RedfishFuture<'a, Result<Option<String>, RedfishError>> {
        Box::pin(async move {
            // Supermicro BMCs default to UEFI mode when the caller doesn't specify one.
            let mode = Some(settings.mode.unwrap_or(BootSourceOverrideMode::UEFI));
            let boot = boot::Boot {
                boot_source_override_target: Some(settings.target),
                boot_source_override_enabled: Some(settings.enabled),
                boot_source_override_mode: mode,
                http_boot_uri: settings.http_boot_uri,
                ..Default::default()
            };
            let body = HashMap::from([("Boot", boot)]);
            let url = format!("Systems/{}", self.s.system_id());
            self.s
                .client
                .patch(&url, body)
                .await
                .map(|_status_code| ())?;
            Ok(None)
        })
    }

    /// Supermicro BMC does not appear to have this.
    /// TODO: Verify that this really clear the TPM.
    fn clear_tpm<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
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
        })
    }

    fn pending<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<HashMap<String, serde_json::Value>, RedfishError>> {
        Box::pin(async move {
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
        })
    }

    // TODO: This resets the pending Bios changes to their default values,
    // but DOES NOT CLEAR THEM. We don't know how to do that, or if Supermicro supports it at all.
    fn clear_pending<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            let url = format!("Systems/{}/Bios/SD", self.s.system_id());
            self.s.clear_pending_with_url(&url).await
        })
    }

    fn pcie_devices<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Vec<PCIeDevice>, RedfishError>> {
        Box::pin(async move {
            let Some(chassis_id) = self.get_chassis_all().await?.into_iter().next() else {
                return Err(RedfishError::NoContent);
            };
            let url = format!("Chassis/{chassis_id}/PCIeDevices");
            let device_ids = self.s.get_members(&url).await?;
            let mut out = Vec::with_capacity(device_ids.len());
            for device_id in device_ids {
                out.push(self.get_pcie_device(&chassis_id, &device_id).await?);
            }
            Ok(out)
        })
    }

    fn update_firmware<'a>(
        &'a self,
        firmware: tokio::fs::File,
    ) -> crate::RedfishFuture<'a, Result<crate::model::task::Task, RedfishError>> {
        Box::pin(async move { self.s.update_firmware(firmware).await })
    }

    fn update_firmware_multipart<'a>(
        &'a self,
        filename: &'a Path,
        _reboot: bool,
        timeout: Duration,
        component_type: ComponentType,
    ) -> crate::RedfishFuture<'a, Result<String, RedfishError>> {
        Box::pin(async move {
            let firmware = File::open(&filename)
                .await
                .map_err(|e| RedfishError::FileError(format!("Could not open file: {}", e)))?;

            let update_service = self.s.get_update_service().await?;

            if update_service.multipart_http_push_uri.is_empty() {
                return Err(RedfishError::NotSupported(
                    "Host BMC does not support HTTP multipart push".to_string(),
                ));
            }

            let parameters = serde_json::to_string(&UpdateParameters::new(component_type))
                .map_err(|e| RedfishError::JsonSerializeError {
                    url: "".to_string(),
                    object_debug: "".to_string(),
                    source: e,
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
        })
    }

    fn get_update_service<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<UpdateService, RedfishError>> {
        Box::pin(async move { self.s.get_update_service().await })
    }

    fn get_tasks<'a>(&'a self) -> crate::RedfishFuture<'a, Result<Vec<String>, RedfishError>> {
        Box::pin(async move { self.s.get_tasks().await })
    }

    fn get_task<'a>(
        &'a self,
        id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<crate::model::task::Task, RedfishError>> {
        Box::pin(async move { self.s.get_task(id).await })
    }

    fn get_firmware<'a>(
        &'a self,
        id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<SoftwareInventory, RedfishError>> {
        Box::pin(async move { self.s.get_firmware(id).await })
    }

    fn get_software_inventories<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Vec<String>, RedfishError>> {
        Box::pin(async move { self.s.get_software_inventories().await })
    }

    fn get_system<'a>(&'a self) -> crate::RedfishFuture<'a, Result<ComputerSystem, RedfishError>> {
        Box::pin(async move { self.s.get_system().await })
    }

    fn get_secure_boot_certificates<'a>(
        &'a self,
        database_id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<Vec<String>, RedfishError>> {
        Box::pin(async move { self.s.get_secure_boot_certificates(database_id).await })
    }

    fn get_secure_boot_certificate<'a>(
        &'a self,
        database_id: &'a str,
        certificate_id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<Certificate, RedfishError>> {
        Box::pin(async move {
            self.s
                .get_secure_boot_certificate(database_id, certificate_id)
                .await
        })
    }

    fn add_secure_boot_certificate<'a>(
        &'a self,
        pem_cert: &'a str,
        database_id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<Task, RedfishError>> {
        Box::pin(async move {
            self.s
                .add_secure_boot_certificate(pem_cert, database_id)
                .await
        })
    }

    fn get_secure_boot<'a>(&'a self) -> crate::RedfishFuture<'a, Result<SecureBoot, RedfishError>> {
        Box::pin(async move { self.s.get_secure_boot().await })
    }

    fn enable_secure_boot<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.enable_secure_boot().await })
    }

    fn disable_secure_boot<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.disable_secure_boot().await })
    }

    fn get_network_device_function<'a>(
        &'a self,
        chassis_id: &'a str,
        id: &'a str,
        port: Option<&'a str>,
    ) -> crate::RedfishFuture<'a, Result<NetworkDeviceFunction, RedfishError>> {
        Box::pin(async move {
            self.s
                .get_network_device_function(chassis_id, id, port)
                .await
        })
    }

    fn get_network_device_functions<'a>(
        &'a self,
        chassis_id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<Vec<String>, RedfishError>> {
        Box::pin(async move { self.s.get_network_device_functions(chassis_id).await })
    }

    fn get_chassis_all<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Vec<String>, RedfishError>> {
        Box::pin(async move { self.s.get_chassis_all().await })
    }

    fn get_chassis<'a>(
        &'a self,
        id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<Chassis, RedfishError>> {
        Box::pin(async move { self.s.get_chassis(id).await })
    }

    fn get_chassis_assembly<'a>(
        &'a self,
        chassis_id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<Assembly, RedfishError>> {
        Box::pin(async move { self.s.get_chassis_assembly(chassis_id).await })
    }

    fn get_chassis_network_adapters<'a>(
        &'a self,
        chassis_id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<Vec<String>, RedfishError>> {
        Box::pin(async move { self.s.get_chassis_network_adapters(chassis_id).await })
    }

    fn get_chassis_network_adapter<'a>(
        &'a self,
        chassis_id: &'a str,
        id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<NetworkAdapter, RedfishError>> {
        Box::pin(async move { self.s.get_chassis_network_adapter(chassis_id, id).await })
    }

    fn get_base_network_adapters<'a>(
        &'a self,
        system_id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<Vec<String>, RedfishError>> {
        Box::pin(async move { self.s.get_base_network_adapters(system_id).await })
    }

    fn get_base_network_adapter<'a>(
        &'a self,
        system_id: &'a str,
        id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<NetworkAdapter, RedfishError>> {
        Box::pin(async move { self.s.get_base_network_adapter(system_id, id).await })
    }

    fn get_ports<'a>(
        &'a self,
        chassis_id: &'a str,
        network_adapter: &'a str,
    ) -> crate::RedfishFuture<'a, Result<Vec<String>, RedfishError>> {
        Box::pin(async move { self.s.get_ports(chassis_id, network_adapter).await })
    }

    fn get_port<'a>(
        &'a self,
        chassis_id: &'a str,
        network_adapter: &'a str,
        id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<crate::NetworkPort, RedfishError>> {
        Box::pin(async move { self.s.get_port(chassis_id, network_adapter, id).await })
    }

    fn get_manager_ethernet_interfaces<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Vec<String>, RedfishError>> {
        Box::pin(async move { self.s.get_manager_ethernet_interfaces().await })
    }

    fn get_manager_ethernet_interface<'a>(
        &'a self,
        id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<crate::EthernetInterface, RedfishError>> {
        Box::pin(async move { self.s.get_manager_ethernet_interface(id).await })
    }

    fn get_system_ethernet_interfaces<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Vec<String>, RedfishError>> {
        Box::pin(async move { self.s.get_system_ethernet_interfaces().await })
    }

    fn get_system_ethernet_interface<'a>(
        &'a self,
        id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<crate::EthernetInterface, RedfishError>> {
        Box::pin(async move { self.s.get_system_ethernet_interface(id).await })
    }

    fn change_uefi_password<'a>(
        &'a self,
        current_uefi_password: &'a str,
        new_uefi_password: &'a str,
    ) -> crate::RedfishFuture<'a, Result<Option<String>, RedfishError>> {
        Box::pin(async move {
            self.s
                .change_uefi_password(current_uefi_password, new_uefi_password)
                .await
        })
    }

    fn change_boot_order<'a>(
        &'a self,
        boot_array: Vec<String>,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            let body = HashMap::from([("Boot", HashMap::from([("BootOrder", boot_array)]))]);
            let url = format!("Systems/{}", self.s.system_id());
            self.s.client.patch(&url, body).await.map(|_status_code| ())
        })
    }

    fn get_service_root<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<ServiceRoot, RedfishError>> {
        Box::pin(async move { self.s.get_service_root().await })
    }

    fn get_systems<'a>(&'a self) -> crate::RedfishFuture<'a, Result<Vec<String>, RedfishError>> {
        Box::pin(async move { self.s.get_systems().await })
    }

    fn get_managers<'a>(&'a self) -> crate::RedfishFuture<'a, Result<Vec<String>, RedfishError>> {
        Box::pin(async move { self.s.get_managers().await })
    }

    fn get_manager<'a>(&'a self) -> crate::RedfishFuture<'a, Result<Manager, RedfishError>> {
        Box::pin(async move { self.s.get_manager().await })
    }

    fn bmc_reset_to_defaults<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.bmc_reset_to_defaults().await })
    }

    fn get_job_state<'a>(
        &'a self,
        job_id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<JobState, RedfishError>> {
        Box::pin(async move { self.s.get_job_state(job_id).await })
    }

    fn get_collection<'a>(
        &'a self,
        id: ODataId,
    ) -> crate::RedfishFuture<'a, Result<Collection, RedfishError>> {
        Box::pin(async move { self.s.get_collection(id).await })
    }

    fn get_resource<'a>(
        &'a self,
        id: ODataId,
    ) -> crate::RedfishFuture<'a, Result<Resource, RedfishError>> {
        Box::pin(async move { self.s.get_resource(id).await })
    }

    /// Set the DPU to be our first netboot device.
    /// The HTTP adapter will only appear after IPv4HTTPSupport bios setting is enabled and the host rebooted.
    fn set_boot_order_dpu_first<'a>(
        &'a self,
        mac_address: &'a str,
    ) -> crate::RedfishFuture<'a, Result<Option<String>, RedfishError>> {
        Box::pin(async move {
            match self.set_mellanox_first(mac_address).await {
                Ok(_) => return Ok(None),
                Err(RedfishError::HTTPErrorCode {
                    status_code,
                    response_body,
                    ..
                }) if status_code == reqwest::StatusCode::BAD_REQUEST
                    && response_body.contains("PropertyUnknown")
                    && response_body.contains("BootOrder") =>
                {
                    // Fall back to the following method if we get this error:
                    // HTTP 400 - "The property BootOrder is not in the list of valid properties for the resource"
                }
                Err(e) => return Err(e),
            }

            // Some supermicro models don't support the set_mellanox_first method, so we fall back to this method
            let mut fbo = self.get_boot_order().await?;

            // The network name is not consistent because it includes the interface name.
            // Falls back to 'UEFI Network' if no specific entry is found to enable network boot options.
            let network = fbo
                .fixed_boot_order
                .iter()
                .find(|entry| entry.starts_with(NETWORK))
                .map(|s| s.as_str())
                .unwrap_or(NETWORK);

            // The hard disk name is also not consistent because it includes the device specifics.
            // Falls back to 'UEFI Hard Disk' if no specific entry is found to enable hard disk boot options.
            let hard_disk = fbo
                .fixed_boot_order
                .iter()
                .find(|entry| entry.starts_with(HARD_DISK))
                .map(|s| s.as_str())
                .unwrap_or(HARD_DISK);

            // Make network the first option, hard disk second, and everything else disabled
            let mut order = ["Disabled"].repeat(fbo.fixed_boot_order.len());
            order[0] = network;
            order[1] = hard_disk;

            // Set the DPU to be the first network device to boot from
            let Some(pos) = fbo
                .uefi_network
                .iter()
                .position(|s| s.contains("UEFI HTTP IPv4 Mellanox") && s.contains(mac_address))
                .or_else(|| {
                    fbo.uefi_network.iter().position(|s| {
                        s.contains("UEFI HTTP IPv4 Nvidia") && s.contains(mac_address)
                    })
                })
            else {
                return Err(RedfishError::NotSupported(
                format!("No match for Mellanox/Nvidia HTTP adapter with MAC address {} in network boot order", mac_address)
            ));
            };
            fbo.uefi_network.swap(0, pos);

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
            self.s
                .client
                .patch(&url, body)
                .await
                .map(|_status_code| ())?;
            Ok(None)
        })
    }

    fn clear_uefi_password<'a>(
        &'a self,
        current_uefi_password: &'a str,
    ) -> crate::RedfishFuture<'a, Result<Option<String>, RedfishError>> {
        Box::pin(async move { self.change_uefi_password(current_uefi_password, "").await })
    }

    fn get_base_mac_address<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Option<String>, RedfishError>> {
        Box::pin(async move { self.s.get_base_mac_address().await })
    }

    fn lockdown_bmc<'a>(
        &'a self,
        target: crate::EnabledDisabled,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.set_syslockdown(target).await })
    }

    fn is_ipmi_over_lan_enabled<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<bool, RedfishError>> {
        Box::pin(async move { self.s.is_ipmi_over_lan_enabled().await })
    }

    fn enable_ipmi_over_lan<'a>(
        &'a self,
        target: crate::EnabledDisabled,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.enable_ipmi_over_lan(target).await })
    }

    fn update_firmware_simple_update<'a>(
        &'a self,
        image_uri: &'a str,
        targets: Vec<String>,
        transfer_protocol: TransferProtocolType,
    ) -> crate::RedfishFuture<'a, Result<Task, RedfishError>> {
        Box::pin(async move {
            self.s
                .update_firmware_simple_update(image_uri, targets, transfer_protocol)
                .await
        })
    }

    fn enable_rshim_bmc<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.enable_rshim_bmc().await })
    }

    fn clear_nvram<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.clear_nvram().await })
    }

    fn get_nic_mode<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Option<NicMode>, RedfishError>> {
        Box::pin(async move { self.s.get_nic_mode().await })
    }

    fn set_nic_mode<'a>(
        &'a self,
        mode: NicMode,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.set_nic_mode(mode).await })
    }

    fn enable_infinite_boot<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.enable_infinite_boot().await })
    }

    fn is_infinite_boot_enabled<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Option<bool>, RedfishError>> {
        Box::pin(async move { self.s.is_infinite_boot_enabled().await })
    }

    fn set_host_rshim<'a>(
        &'a self,
        enabled: EnabledDisabled,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.set_host_rshim(enabled).await })
    }

    fn get_host_rshim<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Option<EnabledDisabled>, RedfishError>> {
        Box::pin(async move { self.s.get_host_rshim().await })
    }

    fn set_idrac_lockdown<'a>(
        &'a self,
        enabled: EnabledDisabled,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.set_idrac_lockdown(enabled).await })
    }

    fn get_boss_controller<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Option<String>, RedfishError>> {
        Box::pin(async move { self.s.get_boss_controller().await })
    }

    fn decommission_storage_controller<'a>(
        &'a self,
        controller_id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<Option<String>, RedfishError>> {
        Box::pin(async move { self.s.decommission_storage_controller(controller_id).await })
    }

    fn create_storage_volume<'a>(
        &'a self,
        controller_id: &'a str,
        volume_name: &'a str,
    ) -> crate::RedfishFuture<'a, Result<Option<String>, RedfishError>> {
        Box::pin(async move {
            self.s
                .create_storage_volume(controller_id, volume_name)
                .await
        })
    }

    fn is_boot_order_setup<'a>(
        &'a self,
        boot_interface_mac: &'a str,
    ) -> crate::RedfishFuture<'a, Result<bool, RedfishError>> {
        Box::pin(async move {
            let (expected, actual) = self
                .get_expected_and_actual_first_boot_option(boot_interface_mac)
                .await?;
            Ok(expected.is_some() && expected == actual)
        })
    }

    fn is_bios_setup<'a>(
        &'a self,
        _boot_interface_mac: Option<&'a str>,
    ) -> crate::RedfishFuture<'a, Result<bool, RedfishError>> {
        Box::pin(async move {
            let diffs = self.diff_bios_bmc_attr().await?;
            Ok(diffs.is_empty())
        })
    }

    fn get_component_integrities<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<ComponentIntegrities, RedfishError>> {
        Box::pin(async move { self.s.get_component_integrities().await })
    }

    fn get_firmware_for_component<'a>(
        &'a self,
        componnent_integrity_id: &'a str,
    ) -> crate::RedfishFuture<
        'a,
        Result<crate::model::software_inventory::SoftwareInventory, RedfishError>,
    > {
        Box::pin(async move {
            self.s
                .get_firmware_for_component(componnent_integrity_id)
                .await
        })
    }

    fn get_component_ca_certificate<'a>(
        &'a self,
        url: &'a str,
    ) -> crate::RedfishFuture<
        'a,
        Result<crate::model::component_integrity::CaCertificate, RedfishError>,
    > {
        Box::pin(async move { self.s.get_component_ca_certificate(url).await })
    }

    fn trigger_evidence_collection<'a>(
        &'a self,
        url: &'a str,
        nonce: &'a str,
    ) -> crate::RedfishFuture<'a, Result<Task, RedfishError>> {
        Box::pin(async move { self.s.trigger_evidence_collection(url, nonce).await })
    }

    fn get_evidence<'a>(
        &'a self,
        url: &'a str,
    ) -> crate::RedfishFuture<'a, Result<crate::model::component_integrity::Evidence, RedfishError>>
    {
        Box::pin(async move { self.s.get_evidence(url).await })
    }

    fn set_host_privilege_level<'a>(
        &'a self,
        level: HostPrivilegeLevel,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.set_host_privilege_level(level).await })
    }

    fn set_utc_timezone<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.set_utc_timezone().await })
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
        // Try using standard BootOptions first
        match self.s.get_boot_options().await {
            Ok(all) => {
                // Get actual first boot option
                let actual_first_boot_option = if let Some(first) = all.members.first() {
                    let id = first.odata_id_get()?;
                    Some(self.s.get_boot_option(id).await?.display_name)
                } else {
                    None
                };

                // Find expected boot option
                let mut expected_first_boot_option = None;
                for b in &all.members {
                    let id = b.odata_id_get()?;
                    let boot_option = self.s.get_boot_option(id).await?;

                    if (boot_option.display_name.contains(MELLANOX_UEFI_HTTP_IPV4)
                        || boot_option.display_name.contains(NVIDIA_UEFI_HTTP_IPV4))
                        && boot_option.display_name.contains(boot_interface_mac)
                    {
                        expected_first_boot_option = Some(boot_option.display_name);
                        break;
                    }
                }

                Ok((expected_first_boot_option, actual_first_boot_option))
            }
            Err(RedfishError::HTTPErrorCode {
                status_code,
                response_body,
                ..
            }) if status_code == reqwest::StatusCode::BAD_REQUEST
                && response_body.contains("PropertyUnknown")
                && response_body.contains("BootOrder") =>
            {
                // Fall back to FixedBootOrder for platforms that don't support standard BootOptions
                let fbo = self.get_boot_order().await?;

                // Get actual first boot option (strip prefix like "UEFI Network:", "UEFI Hard Disk:", etc.)
                let actual_first_boot_option = fbo.fixed_boot_order.first().and_then(|entry| {
                    // Find the first colon and take everything after it
                    entry.find(':').map(|idx| entry[idx + 1..].to_string())
                });

                // Find expected boot option in UEFINetwork list
                let expected_first_boot_option = fbo
                    .uefi_network
                    .iter()
                    .find(|entry| {
                        (entry.contains(MELLANOX_UEFI_HTTP_IPV4)
                            || entry.contains(NVIDIA_UEFI_HTTP_IPV4))
                            && entry.contains(boot_interface_mac)
                    })
                    .cloned();

                Ok((expected_first_boot_option, actual_first_boot_option))
            }
            Err(e) => Err(e),
        }
    }

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
        add_keys!("IPv4PXESupport", EnabledDisabled::Disabled);
        add_keys!("IPv6HTTPSupport", EnabledDisabled::Disabled);
        add_keys!("IPv6PXESupport", EnabledDisabled::Disabled);

        // Enable TPM - check current format and use matching enum
        let current_attrs = self.s.bios_attributes().await?;
        let tpm_value = current_attrs
            .as_object()
            .and_then(|attrs| {
                attrs.iter().find(|(key, _)| {
                    key.split('_')
                        .next()
                        .unwrap_or(key)
                        .starts_with("SecurityDeviceSupport")
                })
            })
            .and_then(|(_, value)| value.as_str());

        if let Some(val) = tpm_value {
            if val == EnabledDisabled::Enabled.to_string()
                || val == EnabledDisabled::Disabled.to_string()
            {
                add_keys!("SecurityDeviceSupport", EnabledDisabled::Enabled)
            } else if val == EnableDisable::Enable.to_string()
                || val == EnableDisable::Disable.to_string()
            {
                add_keys!("SecurityDeviceSupport", EnableDisable::Enable)
            } else {
                return Err(RedfishError::GenericError {
                    error: "Unexpected SecurityDeviceSupport value".to_string(),
                });
            }
        } else {
            return Err(RedfishError::GenericError {
                error: "Missing SecurityDeviceSupport value".to_string(),
            });
        }

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
        self.s
            .client
            .patch(&url, body)
            .await
            .or_else(|err| {
                // The Grace-Grace Supermicros in our GB200 lab do not seem to support
                // disabling KCS access from the host to its BMC. Use this workaround to
                // temporarily enable ingesting these servers.
                if err.not_found() {
                    tracing::warn!(
                        "Supermicro was uanble to find {url}: {err}; not returning error to caller"
                    );
                    Ok((StatusCode::OK, None))
                } else {
                    Err(err)
                }
            })
            .map(|_status_code| ())
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

        // The network name is not consistent because it includes the interface name.
        // Falls back to 'UEFI Network' if no specific entry is found to enable network boot options.
        let network = fbo
            .fixed_boot_order
            .iter()
            .find(|entry| entry.starts_with(NETWORK))
            .map(|s| s.as_str())
            .unwrap_or(NETWORK);

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
    async fn set_mellanox_first(&self, boot_interface: &str) -> Result<(), RedfishError> {
        let mut with_name_match = None; // the ID of the option matching with_name
        let mut ordered = Vec::new(); // the final boot options
        let all = self.s.get_boot_options().await?;
        for b in all.members {
            let id = b.odata_id_get()?;
            let boot_option = self.s.get_boot_option(id).await?;

            if (boot_option.display_name.contains(MELLANOX_UEFI_HTTP_IPV4)
                || boot_option.display_name.contains(NVIDIA_UEFI_HTTP_IPV4))
                && boot_option.display_name.contains(boot_interface)
            {
                // Here are the patterns we have seen so far:
                // UEFI HTTP IPv4 Mellanox Network Adapter - A0:88:C2:EA:84:D0(MAC:A088C2EA84D0)
                // UEFI HTTP IPv4 Nvidia Network Adapter - C4:70:BD:F0:40:AA - C470BDF040AA"
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
            let clean_key = k.split('_').next().unwrap().to_string();
            by_name
                .entry(clean_key)
                .and_modify(|e| e.push(k.clone()))
                .or_insert(vec![k.clone()]);
        }
        Ok(by_name)
    }

    // Check if this is a Grace-Grace SMC (ARS-121L-DNR) that needs host_interface enabled
    async fn is_grace_grace_smc(&self) -> Result<bool, RedfishError> {
        Ok(self
            .s
            .get_system()
            .await?
            .model
            .unwrap_or_default()
            .contains("ARS-121L-DNR"))
    }
}

// UpdateParameters is what is sent for a multipart firmware upload's metadata.
#[allow(clippy::type_complexity)]
#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct UpdateParameters {
    targets: Vec<String>,
    #[serde(rename = "@Redfish.OperationApplyTime")]
    pub apply_time: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    oem: Option<HashMap<String, HashMap<String, HashMap<String, bool>>>>,
}

impl UpdateParameters {
    pub fn new(component_type: ComponentType) -> UpdateParameters {
        let target = match component_type {
            ComponentType::UEFI => "/redfish/v1/Systems/1/Bios",
            ComponentType::BMC => "/redfish/v1/Managers/1",
            ComponentType::CPLDMB => "/redfish/v1/UpdateService/FirmwareInventory/CPLD_Motherboard",
            ComponentType::CPLDMID => {
                "/redfish/v1/UpdateService/FirmwareInventory/CPLD_Backplane_1"
            }
            _ => "Unrecognized component type",
        }
        .to_string();

        let oem = match component_type {
            ComponentType::UEFI => Some(HashMap::from([(
                "Supermicro".to_string(),
                HashMap::from([(
                    "BIOS".to_string(),
                    HashMap::from([
                        ("PreserveME".to_string(), true),
                        ("PreserveNVRAM".to_string(), true),
                        ("PreserveSMBIOS".to_string(), true),
                        ("BackupBIOS".to_string(), false),
                    ]),
                )]),
            )])),
            ComponentType::BMC => Some(HashMap::from([(
                "Supermicro".to_string(),
                HashMap::from([(
                    "BMC".to_string(),
                    HashMap::from([
                        ("PreserveCfg".to_string(), true),
                        ("PreserveSdr".to_string(), true),
                        ("PreserveSsl".to_string(), true),
                        ("BackupBMC".to_string(), true),
                    ]),
                )]),
            )])),
            _ => None,
        };
        UpdateParameters {
            targets: vec![target],
            apply_time: "Immediate".to_string(),
            oem,
        }
    }
}
