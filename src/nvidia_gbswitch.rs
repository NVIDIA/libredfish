use reqwest::StatusCode;
use std::{collections::HashMap, path::Path, time::Duration};

use crate::model::account_service::ManagerAccount;
use crate::model::certificate::Certificate;
use crate::model::component_integrity::ComponentIntegrities;
use crate::model::oem::nvidia_dpu::NicMode;
use crate::model::sensor::{GPUSensors, Sensors};
use crate::model::service_root::RedfishVendor;
use crate::model::task::Task;
use crate::model::thermal::{LeakDetector, Temperature, TemperaturesOemNvidia, Thermal};
use crate::model::update_service::{ComponentType, TransferProtocolType, UpdateService};
use crate::model::PCIeDevices;
use crate::REDFISH_ENDPOINT;
use crate::{
    model::{
        boot::{BootSourceOverrideEnabled, BootSourceOverrideTarget},
        chassis::{Assembly, NetworkAdapter},
        sel::{LogEntry, LogEntryCollection},
        service_root::ServiceRoot,
        storage::Drives,
        BootOption, ComputerSystem, Manager,
    },
    standard::RedfishStandard,
    BiosProfileType, Chassis, Collection, NetworkDeviceFunction, ODataId, Redfish, RedfishError,
    Resource,
};
use crate::{EnabledDisabled, JobState, MachineSetupStatus, PCIeDevice, RoleId};

const UEFI_PASSWORD_NAME: &str = "AdminPassword";

pub struct Bmc {
    s: RedfishStandard,
}

impl Bmc {
    pub fn new(s: RedfishStandard) -> Result<Bmc, RedfishError> {
        Ok(Bmc { s })
    }
}

#[derive(Copy, Clone)]
pub enum BootOptionName {
    Http,
    Pxe,
    UefiHd,
}

impl BootOptionName {
    fn to_string(self) -> &'static str {
        match self {
            BootOptionName::Http => "UEFI HTTPv4",
            BootOptionName::Pxe => "UEFI PXEv4",
            BootOptionName::UefiHd => "HD(",
        }
    }
}

enum BootOptionMatchField {
    DisplayName,
    UefiDevicePath,
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

    /// Note that GH200 account_ids are not numbers but usernames: "root", "admin", etc
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

    async fn get_firmware(
        &self,
        id: &str,
    ) -> Result<crate::model::software_inventory::SoftwareInventory, RedfishError> {
        self.s.get_firmware(id).await
    }

    async fn get_software_inventories(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_software_inventories().await
    }

    async fn get_tasks(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_tasks().await
    }

    async fn get_task(&self, id: &str) -> Result<crate::model::task::Task, RedfishError> {
        self.s.get_task(id).await
    }

    async fn get_power_state(&self) -> Result<crate::PowerState, RedfishError> {
        self.s.get_power_state().await
    }

    async fn get_power_metrics(&self) -> Result<crate::Power, RedfishError> {
        Err(RedfishError::NotSupported(
            "GH200 PowerSubsystem not populated".to_string(),
        ))
    }

    async fn power(&self, action: crate::SystemPowerControl) -> Result<(), RedfishError> {
        self.s.power(action).await
    }

    async fn bmc_reset(&self) -> Result<(), RedfishError> {
        self.s.bmc_reset().await
    }

    async fn chassis_reset(
        &self,
        chassis_id: &str,
        reset_type: crate::SystemPowerControl,
    ) -> Result<(), RedfishError> {
        self.s.chassis_reset(chassis_id, reset_type).await
    }

    async fn get_thermal_metrics(&self) -> Result<crate::Thermal, RedfishError> {
        let mut temperatures = Vec::new();
        let fans = Vec::new();
        let mut leak_detectors = Vec::new();

        // gb200 bianca has temperature sensors in several chassis items
        let chassis_all = self.s.get_chassis_all().await?;
        for chassis_id in chassis_all {
            if chassis_id != "MGX_NVSwitch_0" {
                continue;
            }
            let mut url = format!("Chassis/{}", chassis_id);
            let (_status_code, chassis): (StatusCode, Chassis) = self.s.client.get(&url).await?;
            if chassis.thermal_subsystem.is_some() {
                url = format!("Chassis/{}/ThermalSubsystem/ThermalMetrics", chassis_id);
                let (_status_code, temps): (StatusCode, TemperaturesOemNvidia) =
                    self.s.client.get(&url).await?;
                if let Some(temp) = temps.temperature_readings_celsius {
                    for t in temp {
                        let sensor: Temperature = Temperature::from(t);
                        temperatures.push(sensor);
                    }
                }
                // walk through leak detection sensors and add those
                url = format!(
                    "Chassis/{}/ThermalSubsystem/LeakDetection/LeakDetectors",
                    chassis_id
                );

                let res: Result<(StatusCode, Sensors), RedfishError> =
                    self.s.client.get(&url).await;

                if let Ok((_, sensors)) = res {
                    for sensor in sensors.members {
                        url = sensor
                            .odata_id
                            .replace(&format!("/{REDFISH_ENDPOINT}/"), "");
                        let (_status_code, l): (StatusCode, LeakDetector) =
                            self.s.client.get(&url).await?;
                        leak_detectors.push(l);
                    }
                }
            }
        }
        let thermals = Thermal {
            temperatures,
            fans,
            leak_detectors: Some(leak_detectors),
            ..Default::default()
        };
        Ok(thermals)
    }

    async fn get_gpu_sensors(&self) -> Result<Vec<GPUSensors>, RedfishError> {
        Err(RedfishError::NotSupported(
            "No GPUs on the switch".to_string(),
        ))
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

    async fn machine_setup(
        &self,
        _boot_interface_mac: Option<&str>,
        _bios_profiles: &HashMap<
            RedfishVendor,
            HashMap<String, HashMap<BiosProfileType, HashMap<String, serde_json::Value>>>,
        >,
        _selected_profile: BiosProfileType,
    ) -> Result<(), RedfishError> {
        Ok(())
    }

    async fn machine_setup_status(
        &self,
        _boot_interface_mac: Option<&str>,
    ) -> Result<MachineSetupStatus, RedfishError> {
        let diffs = vec![];

        Ok(MachineSetupStatus {
            is_done: diffs.is_empty(),
            diffs,
        })
    }

    async fn set_machine_password_policy(&self) -> Result<(), RedfishError> {
        use serde_json::Value::Number;
        // These are also the defaults
        let body = HashMap::from([
            // 10 attempts before lockout. This is the default on GB Switch.
            ("AccountLockoutThreshold", Number(10.into())),
            // 600 is the smallest value it will accept. 10 minutes, in seconds.
            ("AccountLockoutDuration", Number(600.into())),
        ]);
        self.s
            .client
            .patch("AccountService", body)
            .await
            .map(|_status_code| ())
    }

    async fn lockdown(&self, _target: crate::EnabledDisabled) -> Result<(), RedfishError> {
        // OpenBMC does not provide a lockdown
        // carbide calls this so don't return an error, otherwise GH200 would need special handling
        Ok(())
    }

    async fn lockdown_status(&self) -> Result<crate::Status, RedfishError> {
        self.s.lockdown_status().await
    }

    async fn setup_serial_console(&self) -> Result<(), RedfishError> {
        self.s.setup_serial_console().await
    }

    async fn serial_console_status(&self) -> Result<crate::Status, RedfishError> {
        self.s.serial_console_status().await
    }

    async fn get_boot_options(&self) -> Result<crate::BootOptions, RedfishError> {
        self.s.get_boot_options().await
    }

    async fn get_boot_option(&self, option_id: &str) -> Result<BootOption, RedfishError> {
        self.s.get_boot_option(option_id).await
    }

    async fn boot_once(&self, target: crate::Boot) -> Result<(), RedfishError> {
        match target {
            crate::Boot::Pxe => {
                self.set_boot_override(
                    BootSourceOverrideTarget::Pxe,
                    BootSourceOverrideEnabled::Once,
                )
                .await
            }
            crate::Boot::HardDisk => {
                self.set_boot_override(
                    BootSourceOverrideTarget::Hdd,
                    BootSourceOverrideEnabled::Once,
                )
                .await
            }
            crate::Boot::UefiHttp => {
                // : UefiHttp isn't in the GH200's list of AllowableValues, but it seems to work
                self.set_boot_override(
                    BootSourceOverrideTarget::UefiHttp,
                    BootSourceOverrideEnabled::Once,
                )
                .await
            }
        }
    }

    async fn boot_first(&self, target: crate::Boot) -> Result<(), RedfishError> {
        match target {
            crate::Boot::Pxe => self.set_boot_order(BootOptionName::Pxe).await,
            crate::Boot::HardDisk => {
                // We're looking for a UefiDevicePath like this:
                // HD(1,GPT,A04D0F1E-E02F-4725-9434-0699B52D8FF2,0x800,0x100000)/\\EFI\\ubuntu\\shimaa64.efi
                // The DisplayName will be something like "ubuntu".
                let boot_array = self
                    .get_boot_options_ids_with_first(
                        BootOptionName::UefiHd,
                        BootOptionMatchField::UefiDevicePath,
                    )
                    .await?;
                self.change_boot_order(boot_array).await
            }
            crate::Boot::UefiHttp => self.set_boot_order(BootOptionName::Http).await,
        }
    }

    async fn clear_tpm(&self) -> Result<(), RedfishError> {
        self.s.clear_tpm().await
    }

    async fn pcie_devices(&self) -> Result<Vec<PCIeDevice>, RedfishError> {
        let mut out = Vec::new();

        // gb200 has pcie devices on several chassis items
        let chassis_all = self.s.get_chassis_all().await?;
        for chassis_id in chassis_all {
            if chassis_id.contains("BMC") {
                continue;
            }

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
                    // To be considered enabled, the PCIE device needs to have
                    // an ID, a status, and the status needs to be enabled.
                    let is_device_enabled = p.id.is_some()
                        && p.status.as_ref().is_some_and(|s| {
                            s.state
                                .as_ref()
                                .is_some_and(|state| state.to_lowercase().contains("enabled"))
                        });
                    if !is_device_enabled {
                        continue;
                    }
                    out.push(p);
                }
            }
        }

        out.sort_unstable_by(|a, b| a.manufacturer.cmp(&b.manufacturer));
        Ok(out)
    }

    async fn update_firmware(
        &self,
        firmware: tokio::fs::File,
    ) -> Result<crate::model::task::Task, RedfishError> {
        self.s.update_firmware(firmware).await
    }

    async fn get_update_service(&self) -> Result<UpdateService, RedfishError> {
        self.s.get_update_service().await
    }

    async fn update_firmware_multipart(
        &self,
        _filename: &Path,
        _reboot: bool,
        _timeout: Duration,
        _component_type: ComponentType,
    ) -> Result<String, RedfishError> {
        Err(RedfishError::NotSupported(
            "GB Switch firmware update unsupported".to_string(),
        ))
    }

    async fn bios(
        &self,
    ) -> Result<std::collections::HashMap<String, serde_json::Value>, RedfishError> {
        Err(RedfishError::NotSupported(
            "GB Switch Bios unsupported".to_string(),
        ))
    }

    async fn set_bios(
        &self,
        _values: HashMap<String, serde_json::Value>,
    ) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported(
            "GB Switch Bios unsupported".to_string(),
        ))
    }

    async fn reset_bios(&self) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported(
            "GB Switch Bios unsupported".to_string(),
        ))
    }

    /// gb switch bios attributes?
    async fn pending(
        &self,
    ) -> Result<std::collections::HashMap<String, serde_json::Value>, RedfishError> {
        Err(RedfishError::NotSupported(
            "GB Switch Bios unsupported".to_string(),
        ))
    }

    /// gh200 has no bios attributes
    async fn clear_pending(&self) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported(
            "GB Switch Bios unsupported".to_string(),
        ))
    }

    async fn get_system(&self) -> Result<ComputerSystem, RedfishError> {
        self.s.get_system().await
    }

    async fn get_secure_boot(&self) -> Result<crate::model::secure_boot::SecureBoot, RedfishError> {
        Err(RedfishError::NotSupported(
            "GB Switch secure boot unsupported".to_string(),
        ))
    }

    async fn enable_secure_boot(&self) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported(
            "GB Switch secure boot unsupported".to_string(),
        ))
    }

    async fn disable_secure_boot(&self) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported(
            "GB Switch secure boot unsupported".to_string(),
        ))
    }

    async fn add_secure_boot_certificate(
        &self,
        _pem_cert: &str,
        _database_id: &str,
    ) -> Result<Task, RedfishError> {
        Err(RedfishError::NotSupported(
            "GB Switch secure boot unsupported".to_string(),
        ))
    }

    async fn get_chassis_all(&self) -> Result<Vec<String>, RedfishError> {
        self.s.get_chassis_all().await
    }

    async fn get_chassis(&self, id: &str) -> Result<crate::Chassis, RedfishError> {
        self.s.get_chassis(id).await
    }

    async fn get_chassis_network_adapters(
        &self,
        _chassis_id: &str,
    ) -> Result<Vec<String>, RedfishError> {
        Err(RedfishError::NotSupported(
            "GB Switch doesn't have NetworkAdapters tree".to_string(),
        ))
    }

    async fn get_chassis_network_adapter(
        &self,
        _chassis_id: &str,
        _id: &str,
    ) -> Result<NetworkAdapter, RedfishError> {
        Err(RedfishError::NotSupported(
            "GB Switch doesn't have NetworkAdapters tree".to_string(),
        ))
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
        Ok(vec![])
    }

    async fn get_system_ethernet_interface(
        &self,
        _id: &str,
    ) -> Result<crate::EthernetInterface, RedfishError> {
        Err(RedfishError::NotSupported(
            "GB Switch doesn't have Systems EthernetInterface".to_string(),
        ))
    }

    async fn get_ports(
        &self,
        _chassis_id: &str,
        _network_adapter: &str,
    ) -> Result<Vec<String>, RedfishError> {
        Err(RedfishError::NotSupported(
            "GB Switch doesn't have NetworkAdapters tree".to_string(),
        ))
    }

    async fn get_port(
        &self,
        _chassis_id: &str,
        _network_adapter: &str,
        _id: &str,
    ) -> Result<crate::NetworkPort, RedfishError> {
        Err(RedfishError::NotSupported(
            "GB Switch doesn't have NetworkAdapters tree".to_string(),
        ))
    }

    async fn get_network_device_function(
        &self,
        _chassis_id: &str,
        _id: &str,
        _port: Option<&str>,
    ) -> Result<NetworkDeviceFunction, RedfishError> {
        Err(RedfishError::NotSupported(
            "GB Switch doesn't have NetworkAdapters tree".to_string(),
        ))
    }

    /// http://redfish.dmtf.org/schemas/v1/NetworkDeviceFunctionCollection.json
    async fn get_network_device_functions(
        &self,
        _chassis_id: &str,
    ) -> Result<Vec<String>, RedfishError> {
        Err(RedfishError::NotSupported(
            "GB Switch doesn't have NetworkAdapters tree".to_string(),
        ))
    }

    // Set current_uefi_password to "" if there isn't one yet. By default there isn't a password.
    /// Set new_uefi_password to "" to disable it.
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
        let body = HashMap::from([("Boot", HashMap::from([("BootOrder", boot_array)]))]);
        let url = format!("Systems/{}/Settings", self.s.system_id());
        self.s.client.patch(&url, body).await?;
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

    async fn get_collection(&self, id: ODataId) -> Result<Collection, RedfishError> {
        self.s.get_collection(id).await
    }

    async fn get_resource(&self, id: ODataId) -> Result<Resource, RedfishError> {
        self.s.get_resource(id).await
    }

    async fn set_boot_order_dpu_first(
        &self,
        _mac_address: &str,
    ) -> Result<Option<String>, RedfishError> {
        Err(RedfishError::NotSupported(
            "Not applicable to NVSwitch".to_string(),
        ))
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

    async fn clear_nvram(&self) -> Result<(), RedfishError> {
        self.s.clear_nvram().await
    }

    async fn get_nic_mode(&self) -> Result<Option<NicMode>, RedfishError> {
        self.s.get_nic_mode().await
    }

    async fn set_nic_mode(&self, mode: NicMode) -> Result<(), RedfishError> {
        self.s.set_nic_mode(mode).await
    }

    async fn is_infinite_boot_enabled(&self) -> Result<Option<bool>, RedfishError> {
        self.s.is_infinite_boot_enabled().await
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

    fn ac_powercycle_supported_by_power(&self) -> bool {
        false
    }

    async fn is_boot_order_setup(&self, _mac_address: &str) -> Result<bool, RedfishError> {
        Err(RedfishError::NotSupported(
            "not populated for GBSwitch".to_string(),
        ))
    }

    async fn get_component_integrities(&self) -> Result<ComponentIntegrities, RedfishError> {
        Err(RedfishError::NotSupported(
            "not populated for GBSwitch".to_string(),
        ))
    }

    async fn get_firmware_for_component(
        &self,
        _component_integrity_id: &str,
    ) -> Result<crate::model::software_inventory::SoftwareInventory, RedfishError> {
        Err(RedfishError::NotSupported(
            "not populated for GBSwitch".to_string(),
        ))
    }

    async fn get_component_ca_certificate(
        &self,
        _url: &str,
    ) -> Result<crate::model::component_integrity::CaCertificate, RedfishError> {
        Err(RedfishError::NotSupported(
            "not populated for GBSwitch".to_string(),
        ))
    }

    async fn trigger_evidence_collection(
        &self,
        _url: &str,
        _nonce: &str,
    ) -> Result<Task, RedfishError> {
        Err(RedfishError::NotSupported(
            "not populated for GBSwitch".to_string(),
        ))
    }

    async fn get_evidence(
        &self,
        _url: &str,
    ) -> Result<crate::model::component_integrity::Evidence, RedfishError> {
        Err(RedfishError::NotSupported(
            "not populated for GBSwitch".to_string(),
        ))
    }

    async fn get_chassis_assembly(&self, _chassis_id: &str) -> Result<Assembly, RedfishError> {
        Err(RedfishError::NotSupported(
            "not populated for GBSwitch".to_string(),
        ))
    }

    async fn get_secure_boot_certificate(
        &self,
        _database_id: &str,
        _certificate_id: &str,
    ) -> Result<Certificate, RedfishError> {
        Err(RedfishError::NotSupported(
            "not populated for GBSwitch".to_string(),
        ))
    }

    async fn get_secure_boot_certificates(
        &self,
        _database_id: &str,
    ) -> Result<Vec<String>, RedfishError> {
        Err(RedfishError::NotSupported(
            "not populated for GBSwitch".to_string(),
        ))
    }

    async fn is_bios_setup(&self, _boot_interface_mac: Option<&str>) -> Result<bool, RedfishError> {
        Err(RedfishError::NotSupported(
            "not populated for GBSwitch".to_string(),
        ))
    }

    async fn enable_infinite_boot(&self) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported(
            "not populated for GBSwitch".to_string(),
        ))
    }
}

impl Bmc {
    async fn set_boot_override(
        &self,
        override_target: BootSourceOverrideTarget,
        override_enabled: BootSourceOverrideEnabled,
    ) -> Result<(), RedfishError> {
        let mut data: HashMap<String, String> = HashMap::new();
        data.insert(
            "BootSourceOverrideEnabled".to_string(),
            format!("{}", override_enabled),
        );
        data.insert(
            "BootSourceOverrideTarget".to_string(),
            format!("{}", override_target),
        );
        let url = format!("Systems/{}/Settings ", self.s.system_id());
        self.s
            .client
            .patch(&url, HashMap::from([("Boot", data)]))
            .await?;
        Ok(())
    }

    // name: The name of the device you want to make the first boot choice.
    async fn set_boot_order(&self, name: BootOptionName) -> Result<(), RedfishError> {
        let boot_array = self
            .get_boot_options_ids_with_first(name, BootOptionMatchField::DisplayName)
            .await?;
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
        with_name: BootOptionName,
        match_field: BootOptionMatchField,
    ) -> Result<Vec<String>, RedfishError> {
        let with_name_str = with_name.to_string();
        let mut ordered = Vec::new(); // the final boot options
        let boot_options = self.s.get_system().await?.boot.boot_order;
        for member in boot_options {
            let b: BootOption = self.s.get_boot_option(member.as_str()).await?;
            let is_match = match match_field {
                BootOptionMatchField::DisplayName => b.display_name.starts_with(with_name_str),
                BootOptionMatchField::UefiDevicePath => {
                    matches!(b.uefi_device_path, Some(x) if x.starts_with(with_name_str))
                }
            };
            if is_match {
                ordered.insert(0, b.id);
            } else {
                ordered.push(b.id);
            }
        }
        Ok(ordered)
    }

    async fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError> {
        let url = format!("Systems/{}/LogServices/SEL/Entries", self.s.system_id());
        let (_status_code, log_entry_collection): (_, LogEntryCollection) =
            self.s.client.get(&url).await?;
        let log_entries = log_entry_collection.members;
        Ok(log_entries)
    }
}
