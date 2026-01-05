use crate::{Assembly, REDFISH_ENDPOINT};
use reqwest::StatusCode;
use std::{collections::HashMap, path::Path, time::Duration};
use tokio::fs::File;

use crate::model::account_service::ManagerAccount;
use crate::model::oem::nvidia_dpu::NicMode;
use crate::model::power::{Power, PowerSupplies, PowerSupply, Voltages};
use crate::model::sensor::{GPUSensors, Sensor, Sensors};
use crate::model::service_root::RedfishVendor;
use crate::model::task::Task;
use crate::model::update_service::{ComponentType, TransferProtocolType, UpdateService};
use crate::{
    model::{
        chassis::NetworkAdapter,
        sel::{LogEntry, LogEntryCollection},
        service_root::ServiceRoot,
        storage::Drives,
        BootOption, ComputerSystem, Manager,
    },
    standard::RedfishStandard,
    BiosProfileType, Collection, NetworkDeviceFunction, ODataId, Redfish, RedfishError, Resource,
};
use crate::{EnabledDisabled, JobState, MachineSetupStatus, RoleId};
use crate::model::certificate::Certificate;
use crate::model::component_integrity::ComponentIntegrities;

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
        let mut voltages = Vec::new();
        let mut power_supplies = Vec::new();
        // liteon powershelf has a strange redfish tree. assemble this
        let mut url = "Chassis/powershelf/PowerSubsystem/PowerSupplies".to_string();
        let (_status_code, ps): (StatusCode, PowerSupplies) = self.s.client.get(&url).await?;
        for supply in ps.members {
            url = supply
                .odata_id
                .replace(&format!("/{REDFISH_ENDPOINT}/"), "");
            let (_status_code, power_supply): (StatusCode, PowerSupply) =
                self.s.client.get(&url).await?;
            power_supplies.push(power_supply);
        }

        url = "Chassis/powershelf/Sensors".to_string();
        let (_status_code, sensors): (StatusCode, Sensors) = self.s.client.get(&url).await?;
        for sensor in sensors.members {
            // now all voltage sensors in all chassis
            if !sensor.odata_id.contains("voltage") {
                continue;
            }
            url = sensor
                .odata_id
                .replace(&format!("/{REDFISH_ENDPOINT}/"), "");
            let (_status_code, t): (StatusCode, Sensor) = self.s.client.get(&url).await?;
            let sensor: Voltages = Voltages::from(t);
            voltages.push(sensor);
        }

        let power = Power {
            odata: None,
            id: "Power".to_string(),
            name: "Power".to_string(),
            power_control: vec![],
            power_supplies: Some(power_supplies),
            voltages: Some(voltages),
            redundancy: None,
        };
        Ok(power)
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
        let url = "Chassis/powershelf/Thermal/".to_string();
        let (_status_code, body) = self.s.client.get(&url).await?;
        Ok(body)
    }

    async fn get_gpu_sensors(&self) -> Result<Vec<GPUSensors>, RedfishError> {
        Err(RedfishError::NotSupported("no gpus".to_string()))
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
        // we don't do any changes for powershelves
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
            // Never lock
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
        Err(RedfishError::NotSupported(
            "Lite-on powershelf does not support changing boot order".to_string(),
        ))
    }

    async fn get_boot_option(&self, _option_id: &str) -> Result<BootOption, RedfishError> {
        Err(RedfishError::NotSupported(
            "Lite-on powershelf does not support changing boot order".to_string(),
        ))
    }

    async fn boot_once(&self, _target: crate::Boot) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported(
            "Lite-on powershelf does not support changing boot order".to_string(),
        ))
    }

    async fn boot_first(&self, _target: crate::Boot) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported(
            "Lite-on powershelf does not support changing boot order".to_string(),
        ))
    }

    async fn clear_tpm(&self) -> Result<(), RedfishError> {
        self.s.clear_tpm().await
    }

    async fn pcie_devices(&self) -> Result<Vec<crate::PCIeDevice>, RedfishError> {
        Err(RedfishError::NotSupported(
            "Lite-on powershelf doesn't have PCIeDevices tree".to_string(),
        ))
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
        filename: &Path,
        _reboot: bool,
        timeout: Duration,
        _component_type: ComponentType,
    ) -> Result<String, RedfishError> {
        let firmware = File::open(&filename)
            .await
            .map_err(|e| RedfishError::FileError(format!("Could not open file: {}", e)))?;

        let update_service = self.s.get_update_service().await?;

        if update_service.multipart_http_push_uri.is_empty() {
            return Err(RedfishError::NotSupported(
                "Host BMC does not support HTTP multipart push".to_string(),
            ));
        }

        let parameters = "{}".to_string();

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
    }

    async fn bios(
        &self,
    ) -> Result<std::collections::HashMap<String, serde_json::Value>, RedfishError> {
        self.s.bios().await
    }

    async fn set_bios(
        &self,
        values: HashMap<String, serde_json::Value>,
    ) -> Result<(), RedfishError> {
        self.s.set_bios(values).await
    }

    async fn reset_bios(&self) -> Result<(), RedfishError> {
        self.s.reset_bios().await
    }

    /// lite-on powershelf has no bios attributes
    async fn pending(
        &self,
    ) -> Result<std::collections::HashMap<String, serde_json::Value>, RedfishError> {
        self.s.pending().await
    }

    /// gh200 has no bios attributes
    async fn clear_pending(&self) -> Result<(), RedfishError> {
        self.s.clear_pending().await
    }

    async fn get_system(&self) -> Result<ComputerSystem, RedfishError> {
        self.s.get_system().await
    }

    async fn get_secure_boot(&self) -> Result<crate::model::secure_boot::SecureBoot, RedfishError> {
        self.s.get_secure_boot().await
    }

    async fn enable_secure_boot(&self) -> Result<(), RedfishError> {
        self.s.enable_secure_boot().await
    }

    async fn disable_secure_boot(&self) -> Result<(), RedfishError> {
        self.s.disable_secure_boot().await
    }

    async fn add_secure_boot_certificate(
        &self,
        _pem_cert: &str,
        _database_id: &str,
    ) -> Result<Task, RedfishError> {
        Err(RedfishError::NotSupported(
            "Lite-on powershelf secure boot unsupported".to_string(),
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
            "Lite-on powershelf doesn't have NetworkAdapters tree".to_string(),
        ))
    }

    async fn get_chassis_network_adapter(
        &self,
        _chassis_id: &str,
        _id: &str,
    ) -> Result<NetworkAdapter, RedfishError> {
        Err(RedfishError::NotSupported(
            "Lite-on powershelf  doesn't have NetworkAdapters tree".to_string(),
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
            "Lite-on powershelf doesn't have Systems EthernetInterface".to_string(),
        ))
    }

    async fn get_ports(
        &self,
        _chassis_id: &str,
        _network_adapter: &str,
    ) -> Result<Vec<String>, RedfishError> {
        Err(RedfishError::NotSupported(
            "Lite-on powershelf doesn't have NetworkAdapters tree".to_string(),
        ))
    }

    async fn get_port(
        &self,
        _chassis_id: &str,
        _network_adapter: &str,
        _id: &str,
    ) -> Result<crate::NetworkPort, RedfishError> {
        Err(RedfishError::NotSupported(
            "Lite-on powershelf doesn't have NetworkAdapters tree".to_string(),
        ))
    }

    async fn get_network_device_function(
        &self,
        _chassis_id: &str,
        _id: &str,
        _port: Option<&str>,
    ) -> Result<NetworkDeviceFunction, RedfishError> {
        Err(RedfishError::NotSupported(
            "Lite-on powershelf doesn't have NetworkAdapters tree".to_string(),
        ))
    }

    /// http://redfish.dmtf.org/schemas/v1/NetworkDeviceFunctionCollection.json
    async fn get_network_device_functions(
        &self,
        _chassis_id: &str,
    ) -> Result<Vec<String>, RedfishError> {
        Err(RedfishError::NotSupported(
            "Lite-on powershelf doesn't have NetworkAdapters tree".to_string(),
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

    async fn change_boot_order(&self, _boot_array: Vec<String>) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported(
            "Lite-on powershelf does not support changing boot order".to_string(),
        ))
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
            "set_dpu_first_boot_order".to_string(),
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

    async fn get_secure_boot_certificate(
        &self,
        _database_id: &str,
        _certificate_id: &str,
    ) -> Result<Certificate, RedfishError> {
        Err(RedfishError::NotSupported(
            "not supported".to_string(),
        ))
    }

    async fn get_secure_boot_certificates(
        &self,
        _database_id: &str,
    ) -> Result<Vec<String>, RedfishError> {
        Err(RedfishError::NotSupported(
            "not supported".to_string(),
        ))
    }

    async fn is_bios_setup(&self, _boot_interface_mac: Option<&str>) -> Result<bool, RedfishError> {
        Err(RedfishError::NotSupported(
            "not supported".to_string(),
        ))
    }

    async fn enable_infinite_boot(&self) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported(
            "not supported".to_string(),
        ))
    }

    async fn trigger_evidence_collection(
        &self,
        _url: &str,
        _nonce: &str,
    ) -> Result<Task, RedfishError> {
        Err(RedfishError::NotSupported(
            "not supported".to_string(),
        ))
    }

    async fn get_evidence(
        &self,
        _url: &str,
    ) -> Result<crate::model::component_integrity::Evidence, RedfishError> {
        Err(RedfishError::NotSupported(
            "not supported".to_string(),
        ))
    }

    async fn get_firmware_for_component(
        &self,
        _component_integrity_id: &str,
    ) -> Result<crate::model::software_inventory::SoftwareInventory, RedfishError> {
        Err(RedfishError::NotSupported(
            "not supported".to_string(),
        ))
    }

    async fn get_component_ca_certificate(
        &self,
        _url: &str,
    ) -> Result<crate::model::component_integrity::CaCertificate, RedfishError> {
        Err(RedfishError::NotSupported(
            "not supported".to_string(),
        ))
    }

    async fn get_chassis_assembly(&self, _chassis_id: &str) -> Result<Assembly, RedfishError> {
        Err(RedfishError::NotSupported(
            "not supported".to_string(),
        ))
    }

    fn ac_powercycle_supported_by_power(&self) -> bool {
        false
    }

    async fn is_boot_order_setup(&self, _mac_address: &str) -> Result<bool, RedfishError> {
        Err(RedfishError::NotSupported(
            "not supported".to_string(),
        ))
    }

    async fn get_component_integrities(&self) -> Result<ComponentIntegrities, RedfishError> {
        Err(RedfishError::NotSupported(
            "not supported".to_string(),
        ))
    }
}

impl Bmc {
    async fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError> {
        // there's an EventLog too, but its always returning Not found!
        let url = format!(
            "Managers/{}/LogServices/EventLog/Entries",
            self.s.manager_id()
        );
        let (_status_code, log_entry_collection): (_, LogEntryCollection) =
            self.s.client.get(&url).await?;
        let log_entries = log_entry_collection.members;
        Ok(log_entries)
    }
}
