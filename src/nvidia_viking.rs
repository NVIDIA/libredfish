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
        boot::{
            BootOverride, BootSourceOverrideEnabled, BootSourceOverrideMode,
            BootSourceOverrideTarget,
        },
        certificate::Certificate,
        chassis::{Assembly, Chassis, NetworkAdapter},
        component_integrity::ComponentIntegrities,
        network_device_function::NetworkDeviceFunction,
        oem::{
            nvidia_dpu::{HostPrivilegeLevel, NicMode},
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
        user: &'a str,
        new: &'a str,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.change_password(user, new).await })
    }

    /*
        https://docs.nvidia.com/dgx/dgxh100-user-guide/redfish-api-supp.html
        curl -k -u <bmc-user>:<password> --request PATCH 'https://<bmc-ip-address>/redfish/v1/AccountService/Accounts/2' --header 'If-Match: *'  --header 'Content-Type: application/json' --data-raw '{ "Password" : "<password>" }'
    */
    fn change_password_by_id<'a>(
        &'a self,
        account_id: &'a str,
        new_pass: &'a str,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            let url = format!("AccountService/Accounts/{}", account_id);
            let mut data = HashMap::new();
            data.insert("Password", new_pass);
            self.s.client.patch_with_if_match(&url, data).await
        })
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
        Box::pin(async move { self.s.power(action).await })
    }

    fn ac_powercycle_supported_by_power(&self) -> bool {
        false
    }

    fn bmc_reset<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            self.s
                .reset_manager(
                    ManagerResetType::ForceRestart,
                    Some(vec![(IF_MATCH, "*".to_string())]),
                )
                .await
        })
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
        Box::pin(async move {
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
        })
    }

    fn get_system_event_log<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Vec<LogEntry>, RedfishError>> {
        Box::pin(async move { self.get_system_event_log().await })
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
        Box::pin(async move { self.clear_nvram().await })
    }

    fn machine_setup<'a>(
        &'a self,
        _boot_interface: Option<crate::BootInterfaceRef<'a>>,
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
            self.set_bios_attributes().await?;
            Ok(None)
        })
    }

    fn machine_setup_status<'a>(
        &'a self,
        boot_interface: Option<crate::BootInterfaceRef<'a>>,
    ) -> crate::RedfishFuture<'a, Result<MachineSetupStatus, RedfishError>> {
        Box::pin(async move {
            // Resolve `InterfaceId` to a MAC via the Redfish-standard
            // EthernetInterface resource.
            let resolved_mac = match boot_interface {
                Some(b) => Some(crate::resolve_boot_interface_mac(self, b).await?),
                None => None,
            };
            let boot_interface_mac = resolved_mac.as_deref();

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
        })
    }

    fn lockdown<'a>(
        &'a self,
        target: EnabledDisabled,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            use EnabledDisabled::*;
            match target {
                Enabled => self.enable_lockdown().await,
                Disabled => self.disable_lockdown().await,
            }
        })
    }

    // TODO: This needs rework
    fn lockdown_status<'a>(&'a self) -> crate::RedfishFuture<'a, Result<Status, RedfishError>> {
        Box::pin(async move {
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
                    let is_locked = kcs == KCS_INTERFACE_DISABLE_DENY_ALL
                        || kcs == KCS_INTERFACE_DISABLE_DISABLED;
                    let is_unlocked = (kcs == KCS_INTERFACE_DISABLE_ALLOW_ALL
                        || kcs == KCS_INTERFACE_DISABLE_ENABLED)
                        && rf == EnabledDisabled::Enabled;

                    let status = if is_locked {
                        StatusInternal::Enabled
                    } else if is_unlocked {
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
        })
    }

    fn setup_serial_console<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
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
        })
    }

    fn serial_console_status<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Status, RedfishError>> {
        Box::pin(async move {
            self.bios_serial_console_status().await
            // TODO: add bmc serial console status
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

    fn boot_once<'a>(&'a self, target: Boot) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            let override_target = match target {
                Boot::Pxe => BootSourceOverrideTarget::Pxe,
                Boot::HardDisk => BootSourceOverrideTarget::Hdd,
                Boot::UefiHttp => BootSourceOverrideTarget::UefiHttp,
            };
            Redfish::set_boot_override(
                self,
                BootOverride {
                    target: override_target,
                    enabled: BootSourceOverrideEnabled::Once,
                    mode: None,
                    http_boot_uri: None,
                },
            )
            .await?;
            Ok(())
        })
    }

    fn boot_first<'a>(
        &'a self,
        target: Boot,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            match target {
                Boot::Pxe => self.set_boot_order(BootDevices::Pxe).await,
                Boot::HardDisk => self.set_boot_order(BootDevices::Hdd).await,
                Boot::UefiHttp => self.set_boot_order(BootDevices::UefiHttp).await,
            }
        })
    }

    fn set_boot_override<'a>(
        &'a self,
        settings: BootOverride,
    ) -> crate::RedfishFuture<'a, Result<Option<String>, RedfishError>> {
        Box::pin(async move {
            let mut boot_data: HashMap<String, serde_json::Value> = HashMap::new();
            boot_data.insert(
                "BootSourceOverrideTarget".to_string(),
                settings.target.to_string().into(),
            );
            boot_data.insert(
                "BootSourceOverrideEnabled".to_string(),
                settings.enabled.to_string().into(),
            );
            // Viking BMCs default to UEFI mode when the caller doesn't specify one.
            let mode = settings.mode.unwrap_or(BootSourceOverrideMode::UEFI);
            boot_data.insert(
                "BootSourceOverrideMode".to_string(),
                mode.to_string().into(),
            );
            if let Some(uri) = settings.http_boot_uri {
                boot_data.insert("HttpBootUri".to_string(), uri.into());
            }
            let data = HashMap::from([("Boot", boot_data)]);
            // Viking BMCs use a pending-settings `SD` endpoint that requires an If-Match
            // ETag from a prior GET to succeed.
            let url = format!("Systems/{}/SD", self.s.system_id());
            let (_, body): (_, HashMap<String, serde_json::Value>) =
                self.s.client.get(&url).await?;
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
            Ok(None)
        })
    }

    fn clear_tpm<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            let tpm = BiosAttributes {
                tpm_operation: Some(DEFAULT_TPM_OPERATION.to_string()),
                tpm_support: Some(DEFAULT_TPM_SUPPORT),
                ..Default::default()
            };

            let set_tpm_attrs = SetBiosAttributes { attributes: tpm };
            return self.patch_bios_attributes(set_tpm_attrs).await;
        })
    }

    fn pending<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<HashMap<String, serde_json::Value>, RedfishError>> {
        Box::pin(async move {
            let url = format!("Systems/{}/Bios/SD", self.s.system_id());
            self.s.pending_with_url(&url).await
        })
    }

    fn clear_pending<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            // TODO: check with viking team, unsupported
            Ok(())
        })
    }

    fn pcie_devices<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Vec<PCIeDevice>, RedfishError>> {
        Box::pin(async move {
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
        })
    }

    fn update_firmware<'a>(
        &'a self,
        firmware: tokio::fs::File,
    ) -> crate::RedfishFuture<'a, Result<Task, RedfishError>> {
        Box::pin(async move { self.s.update_firmware(firmware).await })
    }

    /// update_firmware_multipart returns a string with the task ID
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
                .map_err(|e| RedfishError::FileError(format!("Could not open file: {e}")))?;

            let parameters = serde_json::to_string(&UpdateParameters::new(component_type))
                .map_err(|e| RedfishError::JsonSerializeError {
                    url: "".to_string(),
                    object_debug: "".to_string(),
                    source: e,
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
        })
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

    fn get_update_service<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<UpdateService, RedfishError>> {
        Box::pin(async move { self.s.get_update_service().await })
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
        Box::pin(async move {
            self.s
                .get_members_with_timout(
                    "UpdateService/FirmwareInventory",
                    Some(Duration::from_secs(180)),
                )
                .await
        })
    }

    fn get_system<'a>(&'a self) -> crate::RedfishFuture<'a, Result<ComputerSystem, RedfishError>> {
        Box::pin(async move { self.s.get_system().await })
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

    fn get_secure_boot_certificates<'a>(
        &'a self,
        database_id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<Vec<String>, RedfishError>> {
        Box::pin(async move { self.s.get_secure_boot_certificates(database_id).await })
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
        Box::pin(async move {
            let mut data = HashMap::new();
            data.insert("SecureBootEnable", true);
            let url = format!("Systems/{}/SecureBoot", self.s.system_id());
            return self.s.client.patch_with_if_match(&url, data).await;
        })
    }

    fn disable_secure_boot<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            let mut data = HashMap::new();
            data.insert("SecureBootEnable", false);
            let url = format!("Systems/{}/SecureBoot", self.s.system_id());
            return self.s.client.patch_with_if_match(&url, data).await;
        })
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
                .change_bios_password(UEFI_PASSWORD_NAME, current_uefi_password, new_uefi_password)
                .await
        })
    }

    fn change_boot_order<'a>(
        &'a self,
        boot_array: Vec<String>,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.change_boot_order_with_etag(boot_array, None).await })
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

    fn get_base_mac_address<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Option<String>, RedfishError>> {
        Box::pin(async move { self.s.get_base_mac_address().await })
    }

    //
    // Details of changing boot order in DGX H100 can be found at
    // https://docs.nvidia.com/dgx/dgxh100-user-guide/redfish-api-supp.html#modifying-the-boot-order-on-dgx-h100-using-redfish.

    fn set_boot_order_dpu_first<'a>(
        &'a self,
        boot_interface: crate::BootInterfaceRef<'a>,
    ) -> crate::RedfishFuture<'a, Result<Option<String>, RedfishError>> {
        Box::pin(async move {
            let address = crate::resolve_boot_interface_mac(self, boot_interface).await?;
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
        })
    }

    fn clear_uefi_password<'a>(
        &'a self,
        current_uefi_password: &'a str,
    ) -> crate::RedfishFuture<'a, Result<Option<String>, RedfishError>> {
        Box::pin(async move { self.change_uefi_password(current_uefi_password, "").await })
    }

    fn lockdown_bmc<'a>(
        &'a self,
        target: crate::EnabledDisabled,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.lockdown_bmc(target).await })
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

    /***
         curl -k -u admin:admin
         --request POST
         --location 'https://<bmcip>/redfish/v1/UpdateService/Actions/Oem/NvidiaUpdateService.ClearNVRAM' \
         --header 'Content-Type: application/json' \
         --data '{
         "Targets": ["/redfish/v1/UpdateService/FirmwareInventory/HostBIOS_0"]
         }'
    ***/
    fn clear_nvram<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
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
        })
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
        Box::pin(async move {
            let attrs = BiosAttributes {
                nvidia_infiniteboot: DEFAULT_NVIDIA_INFINITEBOOT.into(),
                ..Default::default()
            };
            let set_attrs = SetBiosAttributes { attributes: attrs };
            self.patch_bios_attributes(set_attrs).await
        })
    }

    fn is_infinite_boot_enabled<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Option<bool>, RedfishError>> {
        Box::pin(async move {
            let bios = self.get_bios().await?;
            match bios.attributes.nvidia_infiniteboot {
                Some(is_infinite_boot_enabled) => Ok(Some(
                    is_infinite_boot_enabled == DEFAULT_NVIDIA_INFINITEBOOT,
                )),
                None => Ok(None),
            }
        })
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
        boot_interface: crate::BootInterfaceRef<'a>,
    ) -> crate::RedfishFuture<'a, Result<bool, RedfishError>> {
        Box::pin(async move {
            let mac = crate::resolve_boot_interface_mac(self, boot_interface).await?;
            let (expected, actual) = self.get_expected_and_actual_first_boot_option(&mac).await?;
            Ok(expected.is_some() && expected == actual)
        })
    }

    fn is_bios_setup<'a>(
        &'a self,
        _boot_interface: Option<crate::BootInterfaceRef<'a>>,
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

    /// Returns the KCS values (lockdown_enabled, lockdown_disabled) based on
    /// what the current firmware accepts. Newer firmware uses "Enabled"/"Disabled"
    /// instead of "Deny All"/"Allow All".
    fn kcs_lockdown_values(&self, current_kcs: &Option<String>) -> (&'static str, &'static str) {
        match current_kcs.as_deref() {
            Some(KCS_INTERFACE_DISABLE_ENABLED | KCS_INTERFACE_DISABLE_DISABLED) => (
                KCS_INTERFACE_DISABLE_DISABLED,
                KCS_INTERFACE_DISABLE_ENABLED,
            ),
            _ => (
                KCS_INTERFACE_DISABLE_DENY_ALL,
                KCS_INTERFACE_DISABLE_ALLOW_ALL,
            ),
        }
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

        let bios = self.get_bios().await?;
        let (kcs_disable_deny, _) =
            self.kcs_lockdown_values(&bios.attributes.kcs_interface_disable);
        let lockdown_attrs = BiosAttributes {
            kcs_interface_disable: kcs_disable_deny.to_string().into(),
            redfish_enable: Disabled.into(), // todo: this should be disabled for the virtual usb nic, not yet implemented by dgx team
            ..Default::default()
        };
        let set_lockdown = SetBiosAttributes {
            attributes: lockdown_attrs,
        };
        self.patch_bios_attributes(set_lockdown).await
    }

    async fn disable_lockdown(&self) -> Result<(), RedfishError> {
        let bios = self.get_bios().await?;
        let (_, kcs_disable_allow) =
            self.kcs_lockdown_values(&bios.attributes.kcs_interface_disable);
        let lockdown_attrs = BiosAttributes {
            kcs_interface_disable: kcs_disable_allow.to_string().into(),
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
