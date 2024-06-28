use std::{collections::HashMap, path::Path, time::Duration};

use crate::{
    model::{
        account_service::ManagerAccount,
        chassis::{Chassis, NetworkAdapter},
        network_device_function::NetworkDeviceFunction,
        oem::hpe::{self, BootDevices},
        power::Power,
        secure_boot::SecureBoot,
        sel::{LogEntry, LogEntryCollection},
        service_root::ServiceRoot,
        software_inventory::SoftwareInventory,
        storage,
        task::Task,
        thermal::Thermal,
        BootOption, ComputerSystem, Manager,
    },
    Boot, BootOptions,
    standard::RedfishStandard,
    MachineSetupStatus, JobState, PCIeDevice, PowerState, Redfish, RedfishError, RoleId, Status,
    EnabledDisabled::{self, Disabled, Enabled},
    StatusInternal, SystemPowerControl,
};

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
        if action == SystemPowerControl::ForceRestart {
            // hpe ilo does warm reset with gracefulrestart op
            self.s.power(SystemPowerControl::GracefulRestart).await
        } else {
            self.s.power(action).await
        }
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
        self.set_boot_order(BootDevices::Pxe).await?;
        Ok(())
    }

    async fn machine_setup_status(&self) -> Result<MachineSetupStatus, RedfishError> {
        Err(RedfishError::NotSupported("machine_setup_status".to_string()))
    }

    async fn set_machine_password_policy(&self) -> Result<(), RedfishError> {
        use serde_json::Value;
        let hpe = Value::Object(serde_json::Map::from_iter(vec![
            (
                "AuthFailureDelayTimeSeconds".to_string(),
                Value::Number(0.into()),
            ),
            (
                "AuthFailureLoggingThreshold".to_string(),
                Value::Number(0.into()),
            ),
            (
                "AuthFailuresBeforeDelay".to_string(),
                Value::Number(0.into()),
            ),
            ("EnforcePasswordComplexity".to_string(), Value::Bool(false)),
        ]));
        let mut oem = serde_json::Map::new();
        oem.insert("Hpe".to_string(), hpe);

        let mut body = HashMap::new();
        body.insert("Oem".to_string(), serde_json::Value::Object(oem));

        self.s
            .client
            .patch("AccountService", body)
            .await
            .map(|_status_code| ())
    }

    async fn lockdown(&self, target: EnabledDisabled) -> Result<(), RedfishError> {
        match target {
            Enabled => self.enable_lockdown().await,
            Disabled => self.disable_lockdown().await,
        }
    }

    async fn lockdown_status(&self) -> Result<Status, RedfishError> {
        let mut url = format!("Systems/{}/Bios", self.s.system_id());
        let (_status_code, bios): (_, hpe::Bios) = self.s.client.get(url.as_str()).await?;
        let bios = bios.attributes;
        url = format!("Managers/{}", self.s.manager_id());
        let (_status, bmc): (_, hpe::SetOemHpeLockdown) = self.s.client.get(url.as_str()).await?;
        let message = format!(
            "usb_boot={}, virtual_nic_enabled={}",
            bios.usb_boot.clone().unwrap_or("Unknown".to_string()),
            bmc.oem.hpe.virtual_nic_enabled
        );
        // todo: kcs_enabled
        Ok(Status {
            message,
            status: if bios.usb_boot.is_some()
                && bios.usb_boot.clone().unwrap() == "Disabled"
                && !bmc.oem.hpe.virtual_nic_enabled
            //&& bios.kcs_enabled.is_some() && bios.kcs_enabled.unwrap() == "false"
            {
                StatusInternal::Enabled
            } else if bios.usb_boot.is_some()
                && bios.usb_boot.clone().unwrap() == "Enabled"
                && bmc.oem.hpe.virtual_nic_enabled
            // if bios.usb_boot == "Enabled" && bios.kcs_enabled.clone().is_some() && bios.kcs_enabled.clone().unwrap() == "true"
            {
                StatusInternal::Disabled
            } else {
                StatusInternal::Partial
            },
        })
    }

    async fn setup_serial_console(&self) -> Result<(), RedfishError> {
        let serial_console = hpe::BiosSerialConsoleAttributes {
            embedded_serial_port: "Com2Irq3".to_string(),
            ems_console: "Virtual".to_string(),
            serial_console_baud_rate: "BaudRate115200".to_string(),
            serial_console_emulation: "Vt100Plus".to_string(),
            serial_console_port: "Virtual".to_string(),
            uefi_serial_debug_level: "ErrorsOnly".to_string(),
            virtual_serial_port: "Com1Irq4".to_string(),
        };
        let set_serial_attrs = hpe::SetBiosSerialConsoleAttributes {
            attributes: serial_console,
        };
        let url = format!("Systems/{}/Bios/settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_serial_attrs)
            .await
            .map(|_status_code| ())
    }

    async fn serial_console_status(&self) -> Result<Status, RedfishError> {
        self.bios_serial_console_status().await
        // TODO: add bmc serial console service status
    }

    async fn get_boot_options(&self) -> Result<BootOptions, RedfishError> {
        self.s.get_boot_options().await
    }

    async fn get_boot_option(&self, option_id: &str) -> Result<BootOption, RedfishError> {
        self.s.get_boot_option(option_id).await
    }

    async fn boot_first(&self, target: Boot) -> Result<(), RedfishError> {
        // TODO: possibly remove this redundant matching, the enum is based on the bmc capabilities
        match target {
            Boot::Pxe => self.set_boot_order(BootDevices::Pxe).await,
            Boot::HardDisk => self.set_boot_order(BootDevices::Hdd).await,
            Boot::UefiHttp => self.set_boot_order(BootDevices::UefiHttp).await,
        }
    }

    async fn boot_once(&self, target: Boot) -> Result<(), RedfishError> {
        self.boot_first(target).await
    }

    async fn clear_tpm(&self) -> Result<(), RedfishError> {
        let tpm = hpe::TpmAttributes {
            tpm2_operation: "Clear".to_string(),
            tpm_visibility: "Visible".to_string(),
        };
        let set_tpm_attrs = hpe::SetTpmAttributes { attributes: tpm };
        let url = format!("Systems/{}/Bios/settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_tpm_attrs)
            .await
            .map(|_status_code| ())
    }

    async fn pending(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        let url = format!("Systems/{}/Bios/settings/", self.s.system_id());
        self.s.pending_with_url(&url).await
    }

    async fn clear_pending(&self) -> Result<(), RedfishError> {
        // TODO
        Ok(())
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
            .change_uefi_password(current_uefi_password, new_uefi_password)
            .await
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
        self.s.get_job_state(job_id).await
    }
}

impl Bmc {
    async fn enable_bios_lockdown(&self) -> Result<(), RedfishError> {
        let lockdown_attrs = hpe::BiosLockdownAttributes {
            //            kcs_enabled: None, // todo: this needs to be set to "false" based on the bmc and bios ver
            usb_boot: Disabled,
        };
        let set_lockdown = hpe::SetBiosLockdownAttributes {
            attributes: lockdown_attrs,
        };
        let url = format!("Systems/{}/Bios/settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_lockdown)
            .await
            .map(|_status_code| ())
    }

    async fn enable_bmc_lockdown(&self) -> Result<(), RedfishError> {
        let lockdown_attrs = hpe::OemHpeLockdownAttrs {
            virtual_nic_enabled: false,
        };
        let set_lockdown1 = hpe::OemHpeLockdown {
            hpe: lockdown_attrs,
        };
        let set_lockdown2 = hpe::SetOemHpeLockdown { oem: set_lockdown1 };
        let url = format!("Managers/{}/", self.s.manager_id());
        self.s
            .client
            .patch(&url, set_lockdown2)
            .await
            .map(|_status_code| ())
    }

    async fn enable_lockdown(&self) -> Result<(), RedfishError> {
        self.enable_bios_lockdown().await?;
        self.enable_bmc_lockdown().await
    }

    async fn disable_bios_lockdown(&self) -> Result<(), RedfishError> {
        let lockdown_attrs = hpe::BiosLockdownAttributes {
            //            kcs_enabled: None, // todo: this needs to be set to "false" based on the bmc and bios ver
            usb_boot: Enabled,
        };
        let set_lockdown = hpe::SetBiosLockdownAttributes {
            attributes: lockdown_attrs,
        };
        let url = format!("Systems/{}/Bios/settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_lockdown)
            .await
            .map(|_status_code| ())
    }

    async fn disable_bmc_lockdown(&self) -> Result<(), RedfishError> {
        let lockdown_attrs = hpe::OemHpeLockdownAttrs {
            virtual_nic_enabled: true,
        };
        let set_lockdown1 = hpe::OemHpeLockdown {
            hpe: lockdown_attrs,
        };
        let set_lockdown2 = hpe::SetOemHpeLockdown { oem: set_lockdown1 };
        let url = format!("Managers/{}/", self.s.manager_id());
        self.s
            .client
            .patch(&url, set_lockdown2)
            .await
            .map(|_status_code| ())
    }

    async fn disable_lockdown(&self) -> Result<(), RedfishError> {
        self.disable_bios_lockdown().await?;
        self.disable_bmc_lockdown().await
    }

    async fn set_virt_enable(&self) -> Result<(), RedfishError> {
        let virt_attrs = hpe::VirtAttributes {
            proc_amd_io_vt: Enabled,
            sriov: Enabled,
        };
        let set_virt_attrs = hpe::SetVirtAttributes {
            attributes: virt_attrs,
        };
        let url = format!("Systems/{}/Bios/settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_virt_attrs)
            .await
            .map(|_status_code| ())
    }

    async fn set_uefi_nic_boot(&self) -> Result<(), RedfishError> {
        let uefi_nic_boot = hpe::UefiHttpAttributes {
            dhcpv4: Enabled,
            http_support: "Auto".to_string(),
        };
        let set_uefi_nic_boot = hpe::SetUefiHttpAttributes {
            attributes: uefi_nic_boot,
        };
        let url = format!("Systems/{}/Bios/settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, set_uefi_nic_boot)
            .await
            .map(|_status_code| ())
    }

    async fn change_boot_order(&self, boot_array: Vec<String>) -> Result<(), RedfishError> {
        let new_boot_order = hpe::SetOemHpeBoot {
            persistent_boot_config_order: boot_array,
        };
        let url = format!("Systems/{}/Bios/oem/hpe/boot/settings/", self.s.system_id());
        self.s
            .client
            .patch(&url, new_boot_order)
            .await
            .map(|_status_code| ())
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
        let with_name_str = match device {
            BootDevices::Pxe => "nic.",
            BootDevices::UefiHttp => "nic.",
            BootDevices::Hdd => "hd.",
            _ => ".",
        };
        let mut ordered = Vec::new(); // the final boot options
        let url = format!("Systems/{}/Bios/oem/hpe/boot/", self.s.system_id());
        let (_, body): (_, hpe::OemHpeBoot) = self.s.client.get(&url).await?;

        for member in body.persistent_boot_config_order {
            if member.to_ascii_lowercase().contains(with_name_str) {
                ordered.insert(0, member);
                continue;
            }
            ordered.push(member);
        }
        Ok(Some(ordered))
    }

    async fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError> {
        let url = format!("Systems/{}/LogServices/IML/Entries", self.s.system_id());
        let (_status_code, log_entry_collection): (_, LogEntryCollection) =
            self.s.client.get(&url).await?;
        let log_entries = log_entry_collection.members;
        Ok(log_entries)
    }

    async fn bios_serial_console_status(&self) -> Result<Status, RedfishError> {
        let message = String::new();

        let enabled = true;
        let disabled = false;
        /*
        let url = &format!("Systems/{}/Bios", self.s.system_id());
        let (_status_code, bios): (_, hpe::Bios) = self.s.client.get(url).await?;
        let bios = bios.attributes;

        let val = bios.embedded_serial_port;
        message.push_str(&format!("embedded_serial_port={val} "));
        if &val == "Com2Irq3" {
            // enabled
            disabled = false;
        } else {
            // disabled
            enabled = false;
        }

        let val = bios.ems_console;
        message.push_str(&format!("ems_console={val} "));
        if &val == "Virtual" {
            disabled = false;
        } else {
            enabled = false;
        }

        let val = bios.serial_console_baud_rate;
        message.push_str(&format!("serial_console_baud_rate={val} "));
        if &val != "BaudRate115200" {
            enabled = false;
        }

        let val = bios.serial_console_emulation;
        message.push_str(&format!("serial_console_emulation={val} "));
        if &val != "Vt100Plus" {
            enabled = false;
        }

        let val = bios.serial_console_port;
        message.push_str(&format!("serial_console_port={val} "));
        if &val != "Virtual" {
            enabled = false;
        }

        let val = bios.virtual_serial_port;
        message.push_str(&format!("virtual_serial_port={val} "));
        if &val != "Com1Irq4" {
            enabled = false;
        }
        */
        Ok(Status {
            message,
            status: match (enabled, disabled) {
                (true, _) => StatusInternal::Enabled,
                (_, true) => StatusInternal::Disabled,
                _ => StatusInternal::Partial,
            },
        })
    }

    // move hpe specific code here
    #[allow(dead_code)]
    pub async fn get_array_controller(
        &self,
        controller_id: u64,
    ) -> Result<storage::ArrayController, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/",
            self.s.system_id(),
            controller_id
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    #[allow(dead_code)]
    pub async fn get_array_controllers(&self) -> Result<storage::ArrayControllers, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/",
            self.s.system_id()
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    /// Query the smart array status from the server
    #[allow(dead_code)]
    pub async fn get_smart_array_status(
        &self,
        controller_id: u64,
    ) -> Result<storage::SmartArray, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/",
            self.s.system_id(),
            controller_id
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    #[allow(dead_code)]
    pub async fn get_logical_drives(
        &self,
        controller_id: u64,
    ) -> Result<storage::LogicalDrives, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/LogicalDrives/",
            self.s.system_id(),
            controller_id
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    #[allow(dead_code)]
    pub async fn get_physical_drive(
        &self,
        drive_id: u64,
        controller_id: u64,
    ) -> Result<storage::DiskDrive, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/DiskDrives/{}/",
            self.s.system_id(),
            controller_id,
            drive_id,
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    #[allow(dead_code)]
    pub async fn get_physical_drives(
        &self,
        controller_id: u64,
    ) -> Result<storage::DiskDrives, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/DiskDrives/",
            self.s.system_id(),
            controller_id
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    #[allow(dead_code)]
    pub async fn get_storage_enclosures(
        &self,
        controller_id: u64,
    ) -> Result<storage::StorageEnclosures, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/StorageEnclosures/",
            self.s.system_id(),
            controller_id
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    #[allow(dead_code)]
    pub async fn get_storage_enclosure(
        &self,
        controller_id: u64,
        enclosure_id: u64,
    ) -> Result<storage::StorageEnclosure, RedfishError> {
        let url = format!(
            "Systems/{}/SmartStorage/ArrayControllers/{}/StorageEnclosures/{}/",
            self.s.system_id(),
            controller_id,
            enclosure_id,
        );
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }
}
