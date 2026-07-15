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
    jsonmap,
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
        oem::nvidia_dpu::{HostPrivilegeLevel, NicMode},
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
        BootOption, ComputerSystem, Manager, ManagerResetType,
    },
    standard::RedfishStandard,
    BiosProfileType, Boot, BootOptions, Collection, EnabledDisabled, JobState, MachineSetupDiff,
    MachineSetupStatus, ODataId, PCIeDevice, PowerState, Redfish, RedfishError, Resource, RoleId,
    Status, StatusInternal, SystemPowerControl,
};

/// AMI uses BIOS attribute SETUP001 for Administrator Password (UEFI password)
const UEFI_PASSWORD_NAME: &str = "SETUP001";

/// LenovoGB300 has no "EndlessBoot" BIOS attribute; infinite
/// boot is expressed via the LEM0003 boot-retry count, where 50 is the
/// firmware's representation for endless retries.
const GB300_INFINITE_BOOT_RETRY: i64 = 50;

/// Build a lockdown `Status` from the fully-locked / fully-unlocked booleans,
/// defaulting to `Partial` when the readout is neither.
fn lockdown_status_from(message: String, is_locked: bool, is_unlocked: bool) -> Status {
    Status {
        message,
        status: if is_locked {
            StatusInternal::Enabled
        } else if is_unlocked {
            StatusInternal::Disabled
        } else {
            StatusInternal::Partial
        },
    }
}

pub struct Bmc {
    s: RedfishStandard,
}

impl Bmc {
    pub fn new(s: RedfishStandard) -> Result<Bmc, RedfishError> {
        Ok(Bmc { s })
    }

    /// Serial-console BIOS attributes as `(key, enabled_value, disabled_value)`.
    /// A `disabled_value` of "any" means any value counts as correctly disabled.
    ///
    /// LenovoGB300's BIOS registry mostly prefixes enum values with the
    /// attribute id, but irregularly: TER001/TER010 stay bare, the port is COM0
    /// not COM1, and the hyphen in VT-UTF8 is dropped. So both forms are listed
    /// explicitly per attribute rather than derived via a prefix rule.
    fn serial_console_attrs(&self) -> Vec<(&'static str, &'static str, &'static str)> {
        let gb300 = self.s.vendor == Some(RedfishVendor::LenovoGB300);
        // (key, generic_enabled, gb300_enabled, disabled)
        const ATTRS: &[(&str, &str, &str, &str)] = &[
            ("TER001", "Enabled", "Enabled", "Disabled"), // Console Redirection
            ("TER010", "Enabled", "Enabled", "Disabled"), // Console Redirection EMS
            ("TER06B", "COM1", "TER06BCOM0", "any"),      // Out-of-Band Mgmt Port
            ("TER0021", "115200", "TER0021115200", "any"), // Bits per second
            ("TER0020", "115200", "TER0020115200", "any"), // Bits per second EMS
            ("TER012", "VT100Plus", "TER012VT100Plus", "any"), // Terminal Type
            ("TER011", "VT-UTF8", "TER011VTUTF8", "any"), // Terminal Type EMS
            ("TER05D", "None", "TER05DNone", "any"),      // Flow Control
        ];
        ATTRS
            .iter()
            .map(|&(key, generic, gb300_val, disabled)| {
                (key, if gb300 { gb300_val } else { generic }, disabled)
            })
            .collect()
    }

    /// LenovoGB300 lockdown status: USB support (attribute-id prefixed enum,
    /// e.g. "USB000Disabled") plus the host interface. There is no KCS BIOS
    /// attribute on this platform, so it is not part of the status.
    async fn lockdown_status_gb300(&self) -> Result<Status, RedfishError> {
        let bios = self.s.bios().await?;
        let url = format!("Systems/{}/Bios", self.s.system_id());
        let attrs = jsonmap::get_object(&bios, "Attributes", &url)?;
        let usb000 = jsonmap::get_str(attrs, "USB000", "Bios Attributes")?;

        let hi_url = format!("Managers/{}/HostInterfaces/Self", self.s.manager_id());
        let (_status, hi): (_, serde_json::Value) = self.s.client.get(&hi_url).await?;
        let hi_enabled = hi
            .get("InterfaceEnabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        let message = format!("usb_support={usb000}, host_interface={hi_enabled}");

        let is_locked = usb000 == "USB000Disabled" && !hi_enabled;
        let is_unlocked = usb000 == "USB000Enabled" && hi_enabled;

        Ok(lockdown_status_from(message, is_locked, is_unlocked))
    }

    /// LenovoAMI-specific lockdown status via OEM ConfigBMC endpoint.
    async fn lockdown_status_lenovo_ami(&self) -> Result<Status, RedfishError> {
        const LOCKDOWN_FIELDS: &[&str] = &[
            "LockoutHostControl",
            "LockoutBiosVariableWriteMode",
            "LockdownBiosSettingsChange",
            "LockdownBiosUpgradeDowngrade",
        ];

        let config_bmc_url = format!("Managers/{}/Oem/ConfigBMC", self.s.manager_id());
        let (_status, body): (_, serde_json::Value) = self.s.client.get(&config_bmc_url).await?;

        let values: Vec<&str> = LOCKDOWN_FIELDS
            .iter()
            .map(|key| body.get(key).and_then(|v| v.as_str()).unwrap_or("unknown"))
            .collect();

        let message = LOCKDOWN_FIELDS
            .iter()
            .zip(&values)
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(", ");

        let is_locked = values.iter().all(|&v| v == "Enable");
        let is_unlocked = values.iter().all(|&v| v == "Disable");

        Ok(lockdown_status_from(message, is_locked, is_unlocked))
    }
}
impl Redfish for Bmc {
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

    /// AMI BMC requires If-Match header for password changes
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

    fn get_tasks<'a>(&'a self) -> crate::RedfishFuture<'a, Result<Vec<String>, RedfishError>> {
        Box::pin(async move { self.s.get_tasks().await })
    }

    fn get_task<'a>(&'a self, id: &'a str) -> crate::RedfishFuture<'a, Result<Task, RedfishError>> {
        Box::pin(async move { self.s.get_task(id).await })
    }

    fn get_power_state<'a>(&'a self) -> crate::RedfishFuture<'a, Result<PowerState, RedfishError>> {
        Box::pin(async move { self.s.get_power_state().await })
    }

    fn get_service_root<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<ServiceRoot, RedfishError>> {
        Box::pin(async move { self.s.get_service_root().await })
    }

    fn get_systems<'a>(&'a self) -> crate::RedfishFuture<'a, Result<Vec<String>, RedfishError>> {
        Box::pin(async move { self.s.get_systems().await })
    }

    fn get_system<'a>(&'a self) -> crate::RedfishFuture<'a, Result<ComputerSystem, RedfishError>> {
        Box::pin(async move { self.s.get_system().await })
    }

    fn get_managers<'a>(&'a self) -> crate::RedfishFuture<'a, Result<Vec<String>, RedfishError>> {
        Box::pin(async move { self.s.get_managers().await })
    }

    fn get_manager<'a>(&'a self) -> crate::RedfishFuture<'a, Result<Manager, RedfishError>> {
        Box::pin(async move { self.s.get_manager().await })
    }

    fn get_secure_boot<'a>(&'a self) -> crate::RedfishFuture<'a, Result<SecureBoot, RedfishError>> {
        Box::pin(async move { self.s.get_secure_boot().await })
    }

    /// AMI BMC requires If-Match header for secure boot changes
    fn disable_secure_boot<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            let mut data = HashMap::new();
            data.insert("SecureBootEnable", false);
            let url = format!("Systems/{}/SecureBoot", self.s.system_id());
            self.s.client.patch_with_if_match(&url, data).await
        })
    }

    /// AMI BMC requires If-Match header for secure boot changes
    fn enable_secure_boot<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            let mut data = HashMap::new();
            data.insert("SecureBootEnable", true);
            let url = format!("Systems/{}/SecureBoot", self.s.system_id());
            self.s.client.patch_with_if_match(&url, data).await
        })
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

    fn get_power_metrics<'a>(&'a self) -> crate::RedfishFuture<'a, Result<Power, RedfishError>> {
        Box::pin(async move { self.s.get_power_metrics().await })
    }

    fn power<'a>(
        &'a self,
        action: SystemPowerControl,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.power(action).await })
    }

    /// AMI BMC only supports ForceRestart
    fn bmc_reset<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            self.s
                .reset_manager(ManagerResetType::ForceRestart, None)
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

    fn bmc_reset_to_defaults<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.bmc_reset_to_defaults().await })
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

    /// Machine setup for AMI BMC.
    ///
    /// Sets up:
    /// 1. Serial console
    /// 2. Clears TPM
    /// 3. BIOS settings
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
            self.setup_serial_console().await?;
            self.clear_tpm().await?;
            let attrs = self.machine_setup_attrs();
            self.set_bios(attrs).await?;
            Ok(None)
        })
    }

    /// Check machine setup status for AMI BMC.
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

            let mut diffs = self.diff_bios_bmc_attr().await?;

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

    fn is_bios_setup<'a>(
        &'a self,
        _boot_interface: Option<crate::BootInterfaceRef<'a>>,
    ) -> crate::RedfishFuture<'a, Result<bool, RedfishError>> {
        Box::pin(async move {
            let diffs = self.diff_bios_bmc_attr().await?;
            Ok(diffs.is_empty())
        })
    }

    /// AMI BMC requires If-Match header for password policy changes
    fn set_machine_password_policy<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
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
        })
    }

    /// AMI lockdown - controls KCS access, USB support, and Host Interface.
    /// On LenovoAMI, uses the OEM ConfigBMC endpoint to control host lockout,
    /// BIOS variable write, BIOS settings change, and BIOS upgrade/downgrade.
    fn lockdown<'a>(
        &'a self,
        target: EnabledDisabled,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            use EnabledDisabled::*;
            if self.s.vendor == Some(RedfishVendor::LenovoAMI) {
                let value = match target {
                    Enabled => "Enable",
                    Disabled => "Disable",
                };
                let body = HashMap::from([
                    ("LockoutHostControl", value),
                    ("LockoutBiosVariableWriteMode", value),
                    ("LockdownBiosSettingsChange", value),
                    ("LockdownBiosUpgradeDowngrade", value),
                ]);
                let config_bmc_url = format!("Managers/{}/Oem/ConfigBMC", self.s.manager_id());
                return self.s.client.post(&config_bmc_url, body).await.map(|_| ());
            }

            // LenovoGB300 has neither the OEM ConfigBMC endpoint nor
            // the generic AMI `KCSACP` BIOS attribute, and its USB enum values
            // are attribute-id prefixed (e.g. "USB000Disabled").
            let hi_enabled = target == Disabled;
            let bios_attrs = if self.s.vendor == Some(RedfishVendor::LenovoGB300) {
                let usb = match target {
                    Enabled => "USB000Disabled",
                    Disabled => "USB000Enabled",
                };
                HashMap::from([("USB000".to_string(), usb.into())])
            } else {
                let (kcsacp, usb) = match target {
                    Enabled => ("Deny All", "Disabled"),
                    Disabled => ("Allow All", "Enabled"),
                };
                HashMap::from([
                    ("KCSACP".to_string(), kcsacp.into()),
                    ("USB000".to_string(), usb.into()),
                ])
            };
            self.set_bios(bios_attrs).await?;

            let hi_url = format!("Managers/{}/HostInterfaces/Self", self.s.manager_id());
            let hi_body = HashMap::from([("InterfaceEnabled", hi_enabled)]);
            self.s.client.patch_with_if_match(&hi_url, hi_body).await
        })
    }

    /// AMI lockdown status - checks KCS access, USB support, and Host Interface.
    /// On LenovoAMI, reads the OEM ConfigBMC endpoint instead. On LenovoGB300,
    /// checks USB support and the host interface (no KCS/ConfigBMC there).
    fn lockdown_status<'a>(&'a self) -> crate::RedfishFuture<'a, Result<Status, RedfishError>> {
        Box::pin(async move {
            if self.s.vendor == Some(RedfishVendor::LenovoAMI) {
                return self.lockdown_status_lenovo_ami().await;
            }

            if self.s.vendor == Some(RedfishVendor::LenovoGB300) {
                return self.lockdown_status_gb300().await;
            }

            let bios = self.s.bios().await?;
            let url = format!("Systems/{}/Bios", self.s.system_id());
            let attrs = jsonmap::get_object(&bios, "Attributes", &url)?;
            let kcsacp = jsonmap::get_str(attrs, "KCSACP", "Bios Attributes")?;
            let usb000 = jsonmap::get_str(attrs, "USB000", "Bios Attributes")?;

            let hi_url = format!("Managers/{}/HostInterfaces/Self", self.s.manager_id());
            let (_status, hi): (_, serde_json::Value) = self.s.client.get(&hi_url).await?;
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

            Ok(lockdown_status_from(message, is_locked, is_unlocked))
        })
    }

    /// Setup serial console for AMI BMC via BIOS attributes.
    fn setup_serial_console<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            let attributes: HashMap<String, serde_json::Value> = self
                .serial_console_attrs()
                .into_iter()
                .map(|(key, enabled, _)| (key.to_string(), enabled.into()))
                .collect();

            self.set_bios(attributes).await
        })
    }

    /// Check serial console status for AMI BMC.
    fn serial_console_status<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Status, RedfishError>> {
        Box::pin(async move {
            let bios = self.bios().await?;
            let url = format!("Systems/{}/Bios", self.s.system_id());
            let attrs = jsonmap::get_object(&bios, "Attributes", &url)?;

            let expected = self.serial_console_attrs();

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
            let alias = match target {
                Boot::Pxe => "Pxe",
                Boot::HardDisk => "Hdd",
                Boot::UefiHttp => "UefiHttp",
            };
            self.set_boot_order(alias).await
        })
    }

    /// AMI requires patching `/Systems/{id}` (NOT `/SD`) with an `If-Match` header.
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
            // AMI BMCs default to UEFI mode when the caller doesn't specify one.
            let mode = settings.mode.unwrap_or(BootSourceOverrideMode::UEFI);
            boot_data.insert(
                "BootSourceOverrideMode".to_string(),
                mode.to_string().into(),
            );
            if let Some(uri) = settings.http_boot_uri {
                boot_data.insert("HttpBootUri".to_string(), uri.into());
            }
            let url = format!("Systems/{}", self.s.system_id());
            self.s
                .client
                .patch_with_if_match(&url, HashMap::from([("Boot", boot_data)]))
                .await?;
            Ok(None)
        })
    }

    /// AMI BMC requires If-Match header for boot order changes
    fn change_boot_order<'a>(
        &'a self,
        boot_array: Vec<String>,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            let body = HashMap::from([("Boot", HashMap::from([("BootOrder", boot_array)]))]);
            let url = format!("Systems/{}/SD", self.s.system_id());
            self.s.client.patch_with_if_match(&url, body).await
        })
    }

    fn clear_tpm<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            // GB300's Grace BIOS registry prefixes the TCG006 enum value with
            // the attribute id ("TCG006TPMClear"); other AMI platforms use the
            // bare "TPM Clear".
            let clear_value = if self.s.vendor == Some(RedfishVendor::LenovoGB300) {
                "TCG006TPMClear"
            } else {
                "TPM Clear"
            };
            self.set_bios(HashMap::from([("TCG006".to_string(), clear_value.into())]))
                .await
        })
    }

    fn pcie_devices<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Vec<PCIeDevice>, RedfishError>> {
        Box::pin(async move { self.s.pcie_devices().await })
    }

    fn update_firmware<'a>(
        &'a self,
        firmware: tokio::fs::File,
    ) -> crate::RedfishFuture<'a, Result<Task, RedfishError>> {
        Box::pin(async move { self.s.update_firmware(firmware).await })
    }

    fn update_firmware_multipart<'a>(
        &'a self,
        filename: &'a Path,
        reboot: bool,
        timeout: Duration,
        component_type: ComponentType,
    ) -> crate::RedfishFuture<'a, Result<String, RedfishError>> {
        Box::pin(async move {
            // The hardcoded URIs and keys below are only verified for the Lenovo HS350x, so
            // restrict this native path to LenovoAMI; other AMI platforms fall back to NotSupported.
            if self.s.vendor != Some(RedfishVendor::LenovoAMI) {
                return self
                    .s
                    .update_firmware_multipart(filename, reboot, timeout, component_type)
                    .await;
            }

            let (oem, targets) = ami_update_targets(&component_type)?;

            // BMC images preserve all BMC configuration via a pre-upload PATCH; BIOS preservation
            // is expressed inside OemParameters (PreserveBIOS) instead.
            if matches!(component_type, ComponentType::BMC) {
                self.set_preserve_configuration_all().await?;
            }

            let file = tokio::fs::File::open(filename)
                .await
                .map_err(|e| RedfishError::FileError(format!("Could not open file: {}", e)))?;

            let update_parameters = serde_json::to_string(&AmiUpdateParameters { targets })
                .map_err(|e| RedfishError::JsonSerializeError {
                    url: "UpdateService/upload".to_string(),
                    object_debug: "AmiUpdateParameters".to_string(),
                    source: e,
                })?;
            let oem_parameters =
                serde_json::to_string(&oem).map_err(|e| RedfishError::JsonSerializeError {
                    url: "UpdateService/upload".to_string(),
                    object_debug: "AmiOemParameters".to_string(),
                    source: e,
                })?;

            let (_status_code, loc, body) = self
                .s
                .client
                .req_update_firmware_multipart_with_oem(
                    filename,
                    file,
                    update_parameters,
                    Some(oem_parameters),
                    "UpdateService/upload", // AMI MegaRAC does "upload" instead of "MultiPartUpload"
                    false,
                    timeout,
                )
                .await?;

            extract_ami_task_id(loc.as_deref(), &body).ok_or_else(|| RedfishError::GenericError {
                error: format!("Could not locate AMI update task in response: {}", body),
            })
        })
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

    fn bios<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<HashMap<String, serde_json::Value>, RedfishError>> {
        Box::pin(async move { self.s.bios().await })
    }

    /// AMI BMC requires If-Match header for BIOS changes
    fn set_bios<'a>(
        &'a self,
        values: HashMap<String, serde_json::Value>,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            let url = format!("Systems/{}/Bios/SD", self.s.system_id());
            let body = HashMap::from([("Attributes", values)]);
            self.s.client.patch_with_if_match(&url, body).await
        })
    }

    fn reset_bios<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.factory_reset_bios().await })
    }

    /// AMI uses /Bios/SD for pending settings
    fn pending<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<HashMap<String, serde_json::Value>, RedfishError>> {
        Box::pin(async move {
            let url = format!("Systems/{}/Bios/SD", self.s.system_id());
            self.s.pending_with_url(&url).await
        })
    }

    /// AMI clear_pending - uses /Bios/SD instead of /Bios/Settings
    fn clear_pending<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
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
            self.s.client.patch_with_if_match(&pending_url, body).await
        })
    }

    fn get_network_device_functions<'a>(
        &'a self,
        chassis_id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<Vec<String>, RedfishError>> {
        Box::pin(async move { self.s.get_network_device_functions(chassis_id).await })
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

    /// AMI uses BIOS attribute SETUP001 for Administrator Password
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

    fn clear_uefi_password<'a>(
        &'a self,
        current_uefi_password: &'a str,
    ) -> crate::RedfishFuture<'a, Result<Option<String>, RedfishError>> {
        Box::pin(async move { self.change_uefi_password(current_uefi_password, "").await })
    }

    fn get_job_state<'a>(
        &'a self,
        job_id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<JobState, RedfishError>> {
        Box::pin(async move { self.s.get_job_state(job_id).await })
    }

    fn get_resource<'a>(
        &'a self,
        id: ODataId,
    ) -> crate::RedfishFuture<'a, Result<Resource, RedfishError>> {
        Box::pin(async move { self.s.get_resource(id).await })
    }

    fn get_collection<'a>(
        &'a self,
        id: ODataId,
    ) -> crate::RedfishFuture<'a, Result<Collection, RedfishError>> {
        Box::pin(async move { self.s.get_collection(id).await })
    }

    /// Set the DPU as the first boot option.
    fn set_boot_order_dpu_first<'a>(
        &'a self,
        boot_interface: crate::BootInterfaceRef<'a>,
    ) -> crate::RedfishFuture<'a, Result<Option<String>, RedfishError>> {
        Box::pin(async move {
            let mac = crate::resolve_boot_interface_mac(self, boot_interface)
                .await?
                .to_uppercase();
            let (system, all_boot_options) = self.get_system_and_boot_options().await?;

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
                    "No HTTP IPv4 boot option found for MAC {mac}; available: {:#?}",
                    all_names
                )));
            };

            let target_id = target.boot_option_reference.clone();
            let mut boot_order = system.boot.boot_order;

            let boot_order_is_set = boot_order.first() == Some(&target_id);
            if boot_order_is_set {
                tracing::info!("NO-OP: DPU ({mac}) is already first in boot order ({target_id})");
            } else {
                boot_order.retain(|id| id != &target_id);
                boot_order.insert(0, target_id.clone());
                self.change_boot_order(boot_order).await?;
            }

            if self.s.vendor == Some(RedfishVendor::LenovoGB300) {
                for option in all_boot_options
                    .iter()
                    .filter(|option| matches!(option.alias.as_deref(), Some("Pxe" | "UefiHttp")))
                {
                    let should_be_enabled = option.boot_option_reference == target_id;
                    if option.boot_option_enabled != Some(should_be_enabled) {
                        let url = format!(
                            "Systems/{}/BootOptions/{}/SD",
                            self.s.system_id(),
                            option.id
                        );
                        self.s
                            .client
                            .patch_with_if_match(
                                &url,
                                HashMap::from([("BootOptionEnabled", should_be_enabled)]),
                            )
                            .await?;
                    }
                }
            }

            Ok(None)
        })
    }

    /// Check if boot order is setup correctly
    fn is_boot_order_setup<'a>(
        &'a self,
        boot_interface: crate::BootInterfaceRef<'a>,
    ) -> crate::RedfishFuture<'a, Result<bool, RedfishError>> {
        Box::pin(async move {
            let mac = crate::resolve_boot_interface_mac(self, boot_interface).await?;
            let (expected, actual) = self.get_expected_and_actual_first_boot_option(&mac).await?;
            let Some(expected) = expected else {
                return Ok(false);
            };
            if actual.as_deref() != Some(&expected) {
                return Ok(false);
            }

            let network_options_setup = if self.s.vendor == Some(RedfishVendor::LenovoGB300) {
                let (_, all_boot_options) = self.get_system_and_boot_options().await?;
                all_boot_options
                    .iter()
                    .filter(|option| matches!(option.alias.as_deref(), Some("Pxe" | "UefiHttp")))
                    .all(|option| {
                        option.boot_option_enabled == Some(option.display_name == expected)
                    })
            } else {
                true
            };
            Ok(network_options_setup)
        })
    }

    fn get_update_service<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<UpdateService, RedfishError>> {
        Box::pin(async move { self.s.get_update_service().await })
    }

    fn get_base_mac_address<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Option<String>, RedfishError>> {
        Box::pin(async move { self.s.get_base_mac_address().await })
    }

    /// AMI lockdown_bmc - BMC-only lockdown (Host Interface only)
    fn lockdown_bmc<'a>(
        &'a self,
        target: EnabledDisabled,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            let interface_enabled = target == EnabledDisabled::Disabled;
            let hi_body = HashMap::from([("InterfaceEnabled", interface_enabled)]);
            let hi_url = format!("Managers/{}/HostInterfaces/Self", self.s.manager_id());
            self.s.client.patch_with_if_match(&hi_url, hi_body).await
        })
    }

    fn is_ipmi_over_lan_enabled<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<bool, RedfishError>> {
        Box::pin(async move { self.s.is_ipmi_over_lan_enabled().await })
    }

    /// AMI BMC requires If-Match header for network protocol changes
    fn enable_ipmi_over_lan<'a>(
        &'a self,
        target: EnabledDisabled,
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            let url = format!("Managers/{}/NetworkProtocol", self.s.manager_id());
            let ipmi_data = HashMap::from([("ProtocolEnabled", target.is_enabled())]);
            let data = HashMap::from([("IPMI", ipmi_data)]);
            self.s.client.patch_with_if_match(&url, data).await
        })
    }

    fn enable_rshim_bmc<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.enable_rshim_bmc().await })
    }

    /// AMI clear_nvram - sets RECV000 (Reset NVRAM) to "Enabled"
    fn clear_nvram<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move {
            // The GB300 Grace BIOS registry has no RECV000 (Reset NVRAM)
            // attribute, so there is no equivalent knob to set.
            if self.s.vendor == Some(RedfishVendor::LenovoGB300) {
                return Err(RedfishError::NotSupported(
                    "clear_nvram: no RECV000 BIOS attribute on LenovoGB300".to_string(),
                ));
            }
            self.set_bios(HashMap::from([("RECV000".to_string(), "Enabled".into())]))
                .await
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
            if self.s.vendor == Some(RedfishVendor::LenovoGB300) {
                return self
                    .set_bios(HashMap::from([(
                        "LEM0003".to_string(),
                        GB300_INFINITE_BOOT_RETRY.into(),
                    )]))
                    .await;
            }
            self.set_bios(HashMap::from([(
                "EndlessBoot".to_string(),
                "Enabled".into(),
            )]))
            .await
        })
    }

    fn is_infinite_boot_enabled<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<Option<bool>, RedfishError>> {
        Box::pin(async move {
            let bios = self.s.bios().await?;
            let url = format!("Systems/{}/Bios", self.s.system_id());
            let attrs = jsonmap::get_object(&bios, "Attributes", &url)?;
            if self.s.vendor == Some(RedfishVendor::LenovoGB300) {
                // LEM0003 may be reported as a JSON number or a numeric string.
                let Some(value) = attrs.get("LEM0003") else {
                    return Ok(None);
                };
                let retry = value
                    .as_i64()
                    .or_else(|| value.as_str().and_then(|s| s.parse::<i64>().ok()));
                return Ok(retry.map(|r| r == GB300_INFINITE_BOOT_RETRY));
            }
            let endless_boot = jsonmap::get_str(attrs, "EndlessBoot", "Bios Attributes")?;
            Ok(Some(endless_boot == "Enabled"))
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

    fn get_component_integrities<'a>(
        &'a self,
    ) -> crate::RedfishFuture<'a, Result<ComponentIntegrities, RedfishError>> {
        Box::pin(async move { self.s.get_component_integrities().await })
    }

    fn get_firmware_for_component<'a>(
        &'a self,
        component_integrity_id: &'a str,
    ) -> crate::RedfishFuture<'a, Result<SoftwareInventory, RedfishError>> {
        Box::pin(async move {
            self.s
                .get_firmware_for_component(component_integrity_id)
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

    /// AMI doesn't support AC power cycle through standard power action
    fn ac_powercycle_supported_by_power(&self) -> bool {
        false
    }

    fn set_utc_timezone<'a>(&'a self) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.set_utc_timezone().await })
    }

    fn set_ntp_servers<'a>(
        &'a self,
        servers: &'a [String],
    ) -> crate::RedfishFuture<'a, Result<(), RedfishError>> {
        Box::pin(async move { self.s.set_manager_ntp_servers(servers).await })
    }
}

impl Bmc {
    /// Enable preservation of all BMC configuration before a BMC firmware flash. PATCHes
    /// `UpdateService` with `If-Match: *` setting every `Oem.AMIUpdateService.PreserveConfiguration`
    /// key to `true`. A non-2xx response is an error so we never silently flash without preserving
    /// config.
    async fn set_preserve_configuration_all(&self) -> Result<(), RedfishError> {
        let preserve: HashMap<&str, bool> = AMI_PRESERVE_CONFIGURATION_KEYS
            .iter()
            .map(|key| (*key, true))
            .collect();
        let body = serde_json::json!({
            "Oem": {
                "AMIUpdateService": {
                    "PreserveConfiguration": preserve,
                }
            }
        });
        self.s
            .client
            .patch_with_if_match("UpdateService", body)
            .await
    }

    async fn get_system_and_boot_options(
        &self,
    ) -> Result<(ComputerSystem, Vec<BootOption>), RedfishError> {
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
        Ok((system, all_boot_options))
    }

    /// Finds the first boot option matching the given alias and moves it to the front
    /// of the boot order.
    async fn set_boot_order(&self, alias: &str) -> Result<(), RedfishError> {
        let (system, all_boot_options) = self.get_system_and_boot_options().await?;

        let target = all_boot_options
            .iter()
            .find(|opt| opt.alias.as_deref() == Some(alias));

        let target_ref = target
            .ok_or_else(|| {
                let all_names: Vec<_> = all_boot_options
                    .iter()
                    .map(|b| {
                        format!(
                            "{}: {} (alias={})",
                            b.boot_option_reference,
                            b.display_name,
                            b.alias.as_deref().unwrap_or("none")
                        )
                    })
                    .collect();
                RedfishError::MissingBootOption(format!(
                    "No boot option with alias {:?} found; available: {:#?}",
                    alias, all_names
                ))
            })?
            .boot_option_reference
            .clone();

        let mut boot_order = system.boot.boot_order;

        if boot_order.first() == Some(&target_ref) {
            return Ok(());
        }

        boot_order.retain(|id| id != &target_ref);
        boot_order.insert(0, target_ref);
        self.change_boot_order(boot_order).await
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
        let (system, all_boot_options) = self.get_system_and_boot_options().await?;

        let expected_first_boot_option = all_boot_options
            .iter()
            .find(|opt| {
                let display = opt.display_name.to_uppercase();
                display.contains("HTTP") && display.contains("IPV4") && display.contains(&mac)
            })
            .map(|opt| opt.display_name.clone());

        let actual_first_boot_option = system.boot.boot_order.first().and_then(|first_ref| {
            all_boot_options
                .iter()
                .find(|opt| &opt.boot_option_reference == first_ref)
                .map(|opt| opt.display_name.clone())
        });

        Ok((expected_first_boot_option, actual_first_boot_option))
    }

    /// Get the BIOS attributes for machine setup.
    fn machine_setup_attrs(&self) -> HashMap<String, serde_json::Value> {
        // The LenovoGB300 (Grace-based) uses a distinct BIOS registry: enum
        // values are prefixed with the attribute ID (e.g. "PCIS007Enabled"),
        // there is no Intel VMX knob (VMXEN) or boot-mode selector (FBO001),
        // and "Infinite Boot" is expressed via the LEM0003 retry count (50 =
        // endless boot) instead of the "EndlessBoot" attribute.
        if self.s.vendor == Some(RedfishVendor::LenovoGB300) {
            return HashMap::from([
                ("PCIS007".to_string(), "PCIS007Enabled".into()), // SR-IOV Support
                ("LEM0001".to_string(), 0.into()),                // PXE retry count
                ("NWSK000".to_string(), "NWSK000Enabled".into()), // Network Stack
                ("NWSK001".to_string(), "NWSK001Disabled".into()), // IPv4 PXE Support
                ("NWSK006".to_string(), "NWSK006Enabled".into()), // IPv4 HTTP Support
                ("NWSK002".to_string(), "NWSK002Disabled".into()), // IPv6 PXE Support
                ("NWSK007".to_string(), "NWSK007Disabled".into()), // IPv6 HTTP Support
                ("LEM0003".to_string(), GB300_INFINITE_BOOT_RETRY.into()), // Infinite Boot
            ]);
        }

        HashMap::from([
            ("VMXEN".to_string(), "Enable".into()), // VMX (Intel Virtualization)
            ("PCIS007".to_string(), "Enabled".into()), // SR-IOV Support
            ("LEM0001".to_string(), 3.into()),      // PXE retry count (remove on future FW update)
            ("NWSK000".to_string(), "Enabled".into()), // Network Stack
            ("NWSK001".to_string(), "Disabled".into()), // IPv4 PXE Support
            ("NWSK006".to_string(), "Enabled".into()), // IPv4 HTTP Support
            ("NWSK002".to_string(), "Disabled".into()), // IPv6 PXE Support
            ("NWSK007".to_string(), "Disabled".into()), // IPv6 HTTP Support
            ("FBO001".to_string(), "UEFI".into()),  // Boot Mode Select
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
            let exp = expected
                .as_str()
                .unwrap_or(&expected.to_string())
                .to_string();
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

/// FirmwareInventory collection prefix for AMI update `Targets` URIs (absolute).
const AMI_FIRMWARE_INVENTORY: &str = "/redfish/v1/UpdateService/FirmwareInventory";

/// All `Oem.AMIUpdateService.PreserveConfiguration` keys set to `true` for a BMC flash.
const AMI_PRESERVE_CONFIGURATION_KEYS: [&str; 14] = [
    "Authentication",
    "EXTLOG",
    "FRU",
    "IPMI",
    "KVM",
    "NTP",
    "Network",
    "REDFISH",
    "SDR",
    "SEL",
    "SNMP",
    "SSH",
    "Syslog",
    "WEB",
];

/// `UpdateParameters` JSON part for an AMI multipart upload. Unlike the standard Redfish path, AMI
/// names explicit `FirmwareInventory` targets and does not use `@Redfish.OperationApplyTime`.
#[derive(serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct AmiUpdateParameters {
    targets: Vec<String>,
}

/// `OemParameters` JSON part for an AMI multipart upload. `image_type` is "BMC" or "BIOS";
/// `preserve_bios` carries BIOS NVRAM preservation ("true"/"false") and is omitted for BMC.
#[derive(serde::Serialize)]
#[serde(rename_all = "PascalCase")]
struct AmiOemParameters {
    image_type: String,
    // PascalCase would produce "PreserveBios"; the AMI MegaRAC API expects BIOS
    // fully capitalized, so rename explicitly.
    #[serde(rename = "PreserveBIOS", skip_serializing_if = "Option::is_none")]
    preserve_bios: Option<String>,
}

/// Map a `ComponentType` to the AMI (`OemParameters`, `Targets`) pair using the HS350x's exact
/// URIs. BMC flashes both ROM banks; BIOS flashes `BIOSImage1` with NVRAM preservation. ROM-bank
/// selection is not expressible via `ComponentType`, so it is hardcoded.
fn ami_update_targets(
    component: &ComponentType,
) -> Result<(AmiOemParameters, Vec<String>), RedfishError> {
    match component {
        ComponentType::BMC => Ok((
            AmiOemParameters {
                image_type: "BMC".to_string(),
                preserve_bios: None,
            },
            vec![
                format!("{AMI_FIRMWARE_INVENTORY}/BMCImage1"),
                format!("{AMI_FIRMWARE_INVENTORY}/BMCImage2"),
            ],
        )),
        ComponentType::UEFI => Ok((
            AmiOemParameters {
                image_type: "BIOS".to_string(),
                preserve_bios: Some("true".to_string()),
            },
            vec![format!("{AMI_FIRMWARE_INVENTORY}/BIOSImage1")],
        )),
        other => Err(RedfishError::NotSupported(format!(
            "AMI multipart update not implemented for component type {:?}",
            other
        ))),
    }
}

/// Extract the task id from an AMI multipart upload response. AMI returns the task reference in
/// `Messages[]` where `MessageId == "Task.1.0.New"` and `MessageArgs[0]` is the task URI; we return
/// its last path segment because `get_task` formats `TaskService/Tasks/{id}`. Falls back to a
/// top-level `Id` (some firmware returns a Task object) and then the `Location` header.
fn extract_ami_task_id(location: Option<&str>, body: &str) -> Option<String> {
    let last_segment = |s: &str| {
        s.trim_end_matches('/')
            .rsplit('/')
            .next()
            .map(str::to_string)
    };

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(messages) = value.get("Messages").and_then(|m| m.as_array()) {
            for message in messages {
                if message.get("MessageId").and_then(|x| x.as_str()) == Some("Task.1.0.New") {
                    if let Some(arg) = message
                        .get("MessageArgs")
                        .and_then(|a| a.as_array())
                        .and_then(|a| a.first())
                        .and_then(|x| x.as_str())
                    {
                        return last_segment(arg);
                    }
                }
            }
        }
        if let Some(id) = value.get("Id").and_then(|x| x.as_str()) {
            return Some(id.to_string());
        }
    }

    location.and_then(last_segment)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_parameters_serializes_to_pascal_case_targets() {
        let params = AmiUpdateParameters {
            targets: vec![
                format!("{AMI_FIRMWARE_INVENTORY}/BMCImage1"),
                format!("{AMI_FIRMWARE_INVENTORY}/BMCImage2"),
            ],
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&params).unwrap()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "Targets": [
                    "/redfish/v1/UpdateService/FirmwareInventory/BMCImage1",
                    "/redfish/v1/UpdateService/FirmwareInventory/BMCImage2",
                ]
            })
        );
    }

    #[test]
    fn oem_parameters_bmc_omits_preserve_bios() {
        let oem = AmiOemParameters {
            image_type: "BMC".to_string(),
            preserve_bios: None,
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&oem).unwrap()).unwrap();
        assert_eq!(json, serde_json::json!({ "ImageType": "BMC" }));
    }

    #[test]
    fn oem_parameters_bios_includes_preserve_bios() {
        let oem = AmiOemParameters {
            image_type: "BIOS".to_string(),
            preserve_bios: Some("true".to_string()),
        };
        let json: serde_json::Value =
            serde_json::from_str(&serde_json::to_string(&oem).unwrap()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "ImageType": "BIOS", "PreserveBIOS": "true" })
        );
    }

    #[test]
    fn ami_update_targets_maps_bmc_to_both_rom_banks() {
        let (oem, targets) = ami_update_targets(&ComponentType::BMC).unwrap();
        assert_eq!(oem.image_type, "BMC");
        assert!(oem.preserve_bios.is_none());
        assert_eq!(
            targets,
            vec![
                "/redfish/v1/UpdateService/FirmwareInventory/BMCImage1".to_string(),
                "/redfish/v1/UpdateService/FirmwareInventory/BMCImage2".to_string(),
            ]
        );
    }

    #[test]
    fn ami_update_targets_maps_uefi_to_bios_image() {
        let (oem, targets) = ami_update_targets(&ComponentType::UEFI).unwrap();
        assert_eq!(oem.image_type, "BIOS");
        assert_eq!(oem.preserve_bios.as_deref(), Some("true"));
        assert_eq!(
            targets,
            vec!["/redfish/v1/UpdateService/FirmwareInventory/BIOSImage1".to_string()]
        );
    }

    #[test]
    fn ami_update_targets_rejects_unsupported_component() {
        let result = ami_update_targets(&ComponentType::PSU { num: 0 });
        assert!(matches!(result, Err(RedfishError::NotSupported(_))));
    }

    #[test]
    fn ami_update_targets_rejects_unknown_component() {
        let result = ami_update_targets(&ComponentType::Unknown);
        assert!(matches!(result, Err(RedfishError::NotSupported(_))));
    }

    #[test]
    fn extract_task_id_from_messages() {
        let body = serde_json::json!({
            "Messages": [
                {
                    "MessageId": "Base.1.0.Success",
                    "MessageArgs": []
                },
                {
                    "MessageId": "Task.1.0.New",
                    "MessageArgs": ["/redfish/v1/TaskService/Tasks/3"]
                }
            ]
        })
        .to_string();
        assert_eq!(extract_ami_task_id(None, &body), Some("3".to_string()));
    }

    #[test]
    fn extract_task_id_from_top_level_id() {
        let body = serde_json::json!({ "Id": "42" }).to_string();
        assert_eq!(extract_ami_task_id(None, &body), Some("42".to_string()));
    }

    #[test]
    fn extract_task_id_falls_back_to_location_header() {
        assert_eq!(
            extract_ami_task_id(Some("/redfish/v1/TaskService/Tasks/7"), "not json"),
            Some("7".to_string())
        );
    }

    #[test]
    fn extract_task_id_returns_none_when_absent() {
        assert_eq!(extract_ami_task_id(None, "{}"), None);
    }
}
