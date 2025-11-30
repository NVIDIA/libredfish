/*
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
use std::{collections::HashMap, fmt, path::Path, time::Duration};

pub mod model;
use model::account_service::ManagerAccount;
pub use model::chassis::{Assembly, Chassis, NetworkAdapter};
pub use model::ethernet_interface::EthernetInterface;
pub use model::network_device_function::NetworkDeviceFunction;
use model::oem::nvidia_dpu::{HostPrivilegeLevel, InternalCPUModel, NicMode};
pub use model::port::NetworkPort;
pub use model::resource::{Collection, OData, Resource};
use model::sensor::GPUSensors;
use model::service_root::{RedfishVendor, ServiceRoot};
use model::software_inventory::SoftwareInventory;
pub use model::system::{BootOptions, PCIeDevice, PowerState, SystemPowerControl, Systems};
use model::task::Task;
use model::update_service::{ComponentType, TransferProtocolType, UpdateService};
pub use model::EnabledDisabled;
use model::Manager;
use model::{secure_boot::SecureBoot, BootOption, ComputerSystem, ODataId};
use serde::{Deserialize, Serialize};
mod dell;
mod error;
mod hpe;
pub mod jsonmap;
mod lenovo;
mod network;
mod nvidia_dpu;

mod nvidia_gbswitch;
mod nvidia_gbx00;
mod nvidia_gh200;
mod nvidia_viking;
mod supermicro;
pub use network::{Endpoint, RedfishClientPool, RedfishClientPoolBuilder, REDFISH_ENDPOINT};
pub mod standard;
pub use error::RedfishError;

use crate::model::certificate::Certificate;
use crate::model::component_integrity::ComponentIntegrities;
use crate::model::power::Power;
use crate::model::sel::LogEntry;
use crate::model::storage::Drives;
use crate::model::thermal::Thermal;

/// Interface to a BMC Redfish server. All calls will include one or more HTTP network calls.
#[async_trait::async_trait]
pub trait Redfish: Send + Sync + 'static {
    /// Rename a user
    async fn change_username(&self, old_name: &str, new_name: &str) -> Result<(), RedfishError>;

    /// Change password by username
    /// This looks up the ID for given username before calling change_password_by_id.
    /// That lookup makes it unsuitable for changing the initial password on
    /// PasswordChangeRequired.
    async fn change_password(&self, username: &str, new_pass: &str) -> Result<(), RedfishError>;

    /// Change password by id
    async fn change_password_by_id(
        &self,
        account_id: &str,
        new_pass: &str,
    ) -> Result<(), RedfishError>;

    /// List current user accounts
    async fn get_accounts(&self) -> Result<Vec<ManagerAccount>, RedfishError>;

    /// Create a new user
    async fn create_user(
        &self,
        username: &str,
        password: &str,
        role_id: RoleId,
    ) -> Result<(), RedfishError>;

    /// Delete a BMC user
    async fn delete_user(&self, username: &str) -> Result<(), RedfishError>;

    // Get firmware version for particular firmware inventory id
    async fn get_firmware(&self, id: &str) -> Result<SoftwareInventory, RedfishError>;

    // Get software inventory collection
    async fn get_software_inventories(&self) -> Result<Vec<String>, RedfishError>;

    // List all Tasks
    async fn get_tasks(&self) -> Result<Vec<String>, RedfishError>;

    // Get information about a task
    async fn get_task(&self, id: &str) -> Result<Task, RedfishError>;

    /// Is this thing even on?
    async fn get_power_state(&self) -> Result<PowerState, RedfishError>;

    /// Returns info about operations that the service supports.
    async fn get_service_root(&self) -> Result<ServiceRoot, RedfishError>;

    /// Returns info about available computer systems.
    async fn get_systems(&self) -> Result<Vec<String>, RedfishError>;

    /// Returns info about computer system.
    async fn get_system(&self) -> Result<ComputerSystem, RedfishError>;

    /// Returns info about available managers.
    async fn get_managers(&self) -> Result<Vec<String>, RedfishError>;

    /// Returns info about managers
    async fn get_manager(&self) -> Result<Manager, RedfishError>;

    /// Get Secure Boot state
    async fn get_secure_boot(&self) -> Result<SecureBoot, RedfishError>;

    /// Disables Secure Boot
    async fn disable_secure_boot(&self) -> Result<(), RedfishError>;

    /// Enables Secure Boot
    async fn enable_secure_boot(&self) -> Result<(), RedfishError>;

    async fn get_secure_boot_certificate(
        &self,
        database_id: &str,
        certificate_id: &str,
    ) -> Result<Certificate, RedfishError>;

    async fn get_secure_boot_certificates(
        &self,
        database_id: &str,
    ) -> Result<Vec<String>, RedfishError>;

    /// Adds certificate to secure boot DB
    /// database_id: "db" for database, "pk" for PK database
    /// Need to reboot DPU for UEFI Redfish client to execute.
    async fn add_secure_boot_certificate(
        &self,
        pem_cert: &str,
        database_id: &str,
    ) -> Result<Task, RedfishError>;

    /// Power supplies and voltages metrics
    async fn get_power_metrics(&self) -> Result<Power, RedfishError>;

    /// Change power state: on, off, reboot, etc
    async fn power(&self, action: SystemPowerControl) -> Result<(), RedfishError>;

    /// Reboot the BMC itself
    async fn bmc_reset(&self) -> Result<(), RedfishError>;

    /// Reset Chassis
    async fn chassis_reset(
        &self,
        chassis_id: &str,
        reset_type: SystemPowerControl,
    ) -> Result<(), RedfishError>;

    /// Reset BMC to the factory defaults.
    async fn bmc_reset_to_defaults(&self) -> Result<(), RedfishError>;

    /// Fans and temperature sensors
    async fn get_thermal_metrics(&self) -> Result<Thermal, RedfishError>;

    /// Voltage, temperature, etc sensors for gpus if they exist.
    async fn get_gpu_sensors(&self) -> Result<Vec<GPUSensors>, RedfishError>;

    /// get system event log similar to ipmitool sel
    async fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError>;

    /// get bmc event log (power events, etc.)
    async fn get_bmc_event_log(
        &self,
        from: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<LogEntry>, RedfishError>;

    /// get drives metrics
    async fn get_drives_metrics(&self) -> Result<Vec<Drives>, RedfishError>;

    /// Sets up a reasonable UEFI configuration.
    /// remember to call lockdown() afterwards to secure the server
    /// - boot_interface_mac: MAC Address of the NIC you wish to boot from
    ///   If not given we look for a Mellanox Bluefield DPU and use that.
    ///   Not applicable to Supermicro and the DPU itself.
    /// bios_profiles: Map of vendor/model (with spaces replaced by underscores)/profile/type
    ///   to extra settings; expected to come from config rather than hardcoded.
    /// selected_profile: Profile to use (if present)
    async fn machine_setup(
        &self,
        boot_interface_mac: Option<&str>,
        bios_profiles: &BiosProfileVendor,
        selected_profile: BiosProfileType,
    ) -> Result<(), RedfishError>;

    /// Is everything that machine_setup does already done?
    async fn machine_setup_status(
        &self,
        boot_interface_mac: Option<&str>,
    ) -> Result<MachineSetupStatus, RedfishError>;

    /// Check if only the BIOS/BMC setup is done
    async fn is_bios_setup(&self, boot_interface_mac: Option<&str>) -> Result<bool, RedfishError>;

    /// Apply a standard BMC password policy. This varies a lot by vendor,
    /// but at a minimum we want passwords to never expire, because our BMCs are
    /// not actively used by humans.
    async fn set_machine_password_policy(&self) -> Result<(), RedfishError>;

    /// Lock the BIOS and BMC ready for tenant use. Disabled reverses the changes.
    async fn lockdown(&self, target: EnabledDisabled) -> Result<(), RedfishError>;

    /// Are the BIOS and BMC currently locked down?
    async fn lockdown_status(&self) -> Result<Status, RedfishError>;

    /// Enable SSH access to console
    async fn setup_serial_console(&self) -> Result<(), RedfishError>;

    /// Is the serial console setup?
    async fn serial_console_status(&self) -> Result<Status, RedfishError>;

    /// Show available boot options
    async fn get_boot_options(&self) -> Result<BootOptions, RedfishError>;

    /// Show available boot options
    async fn get_boot_option(&self, option_id: &str) -> Result<BootOption, RedfishError>;

    /// Boot a single time of the given target. Does not change boot order after that.
    async fn boot_once(&self, target: Boot) -> Result<(), RedfishError>;

    /// Change boot order putting this target first
    async fn boot_first(&self, target: Boot) -> Result<(), RedfishError>;

    /// Change boot order by setting boot array.
    async fn change_boot_order(&self, boot_array: Vec<String>) -> Result<(), RedfishError>;

    /// Reset and enable the TPM
    async fn clear_tpm(&self) -> Result<(), RedfishError>;

    /// List PCIe devices
    async fn pcie_devices(&self) -> Result<Vec<PCIeDevice>, RedfishError>;

    /// Update BMC firmware
    async fn update_firmware(&self, filename: tokio::fs::File) -> Result<Task, RedfishError>;

    /// Update UEFI firmware, returns a task ID
    async fn update_firmware_multipart(
        &self,
        firmware: &Path,
        reboot: bool,
        timeout: Duration,
        component_type: ComponentType,
    ) -> Result<String, RedfishError>;

    /// This action shall update installed software components in a software image file located at an ImageURI parameter-specified URI.
    /// image_uri - The URI of the software image to install.
    /// transfer_protocol - The network protocol that the update service uses to retrieve the software image file located at the URI provided in ImageURI.
    /// This parameter is ignored if the URI provided in ImageURI contains a scheme.
    /// targets - An array of URIs that indicate where to apply the update image.
    async fn update_firmware_simple_update(
        &self,
        image_uri: &str,
        targets: Vec<String>,
        transfer_protocol: TransferProtocolType,
    ) -> Result<Task, RedfishError>;

    /*
     * Diagnostic calls
     */
    /// All the BIOS values for this provider. Very OEM specific.
    async fn bios(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError>;

    /// Modify specific BIOS values.  Also very OEM and model specific.
    async fn set_bios(
        &self,
        values: HashMap<String, serde_json::Value>,
    ) -> Result<(), RedfishError>;

    /// Reset BIOS to factory settings
    async fn reset_bios(&self) -> Result<(), RedfishError>;

    /// Pending BIOS attributes. Changes that were requested but not applied yet because
    /// they need a reboot.
    async fn pending(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError>;

    /// Clear all pending jobs
    async fn clear_pending(&self) -> Result<(), RedfishError>;

    // List all Network Device Functions of a given Chassis
    async fn get_network_device_functions(
        &self,
        chassis_id: &str,
    ) -> Result<Vec<String>, RedfishError>;

    // Get Network Device Function details
    async fn get_network_device_function(
        &self,
        chassis_id: &str,
        id: &str,
        port: Option<&str>,
    ) -> Result<NetworkDeviceFunction, RedfishError>;

    // List all Chassises
    async fn get_chassis_all(&self) -> Result<Vec<String>, RedfishError>;

    // Get Chassis details
    async fn get_chassis(&self, id: &str) -> Result<Chassis, RedfishError>;

    // Get Chassis Assembly details
    async fn get_chassis_assembly(&self, chassis_id: &str) -> Result<Assembly, RedfishError>;

    // List all Network Adapters for the specific Chassis
    async fn get_chassis_network_adapters(
        &self,
        chassis_id: &str,
    ) -> Result<Vec<String>, RedfishError>;

    // Get Network Adapter details for the specific Chassis and Network Adapter
    async fn get_chassis_network_adapter(
        &self,
        chassis_id: &str,
        id: &str,
    ) -> Result<NetworkAdapter, RedfishError>;

    // List all Base Network Adapters for the specific Chassis
    // Only implemented in iLO5
    async fn get_base_network_adapters(&self, system_id: &str)
        -> Result<Vec<String>, RedfishError>;

    // Get Base Network Adapter details for the specific Chassis and Network Adapter
    // Only implemented in iLO5
    async fn get_base_network_adapter(
        &self,
        system_id: &str,
        id: &str,
    ) -> Result<NetworkAdapter, RedfishError>;

    // List all High Speed Ports of a given Chassis
    async fn get_ports(
        &self,
        chassis_id: &str,
        network_adapter: &str,
    ) -> Result<Vec<String>, RedfishError>;

    // Get High Speed Port details
    async fn get_port(
        &self,
        chassis_id: &str,
        network_adapter: &str,
        id: &str,
    ) -> Result<NetworkPort, RedfishError>;

    // List all Ethernet Interfaces for the default `Manager`
    async fn get_manager_ethernet_interfaces(&self) -> Result<Vec<String>, RedfishError>;

    // Get Ethernet Interface details for an interface on the default `Manager`
    async fn get_manager_ethernet_interface(
        &self,
        id: &str,
    ) -> Result<EthernetInterface, RedfishError>;

    // List all Ethernet Interfaces for the default `System`
    async fn get_system_ethernet_interfaces(&self) -> Result<Vec<String>, RedfishError>;

    // Get Ethernet Interface details for an interface on the default `System`
    async fn get_system_ethernet_interface(
        &self,
        id: &str,
    ) -> Result<EthernetInterface, RedfishError>;

    // Change UEFI Password
    async fn change_uefi_password(
        &self,
        current_uefi_password: &str,
        new_uefi_password: &str,
    ) -> Result<Option<String>, RedfishError>;

    async fn get_job_state(&self, job_id: &str) -> Result<JobState, RedfishError>;

    /// A kind-of-generic method to retrieve any Redfish resource. A resource is a top level object defined by Redfish spec snd
    /// implements trait named IsResource. A resource should have @odata.type and @odata.id annotations as defined by the spec.
    ///
    /// Method takes OdatIaD as the input that is defined as the URI for the resource.
    ///
    /// The following two macros are provided to implement IsResource trait for objects. Use the one that mathces
    /// the struct depending on how @odata.id and @odata.type are captured. Example use of macros:
    ///
    ///  impl_is_resource_for_option_odatalinks!(crate::EthernetInterface);   # captures @odata.xxxx annotations in Option<ODataLinks>
    ///  impl_is_resource!(crate::model::PCIeDevice);                         # Uses OData instead
    ///
    ///
    /// This method returns Resource struct that contains the raw JSON and can be converted to an resource by calling try_get<T>()
    /// method. Resource::try_get<T>() method will desrialize JSON making surethat requested type T matches with @odata.type. Error will be
    /// returned otherwise. This imposes a restriction on naming struct's for resources. @odata.type has the format #<ResourceType>.<Version>.<TermName>
    /// Struct name for @odata.type should be named <TermName>. For example, @odata.type for systems is "@odata.type": "#ComputerSystem.v1_17_0.ComputerSystem".
    /// Corresponding RUST struct is named ComputerSystem.
    ///
    /// Example ussage:
    /// let chassis : Chassis =  redfish.get_resource(chassis_odata_id)
    ///                             .await
    ///                              .and_then(|r| {r.try_get()})?;
    ///
    ///
    async fn get_resource(&self, id: ODataId) -> Result<Resource, RedfishError>;

    /// A kind-of-generic api to retrieve any resource. See get_resource() api for more details.
    /// This method returns Collection object that contains raw JSON and can be conveted to
    /// generic type ResourceCollection<T> via generic method try_get()
    /// Sample usage:
    ///
    /// let rc_nw_adapter : ResourceCollection<NetworkAdapter> =  self.s.get_collection(na_id)
    ///                                                              .await
    ///                                                              .and_then(|r| r.try_get())?;
    /// try_get() will make sure that @odata.type of the returned collection matches with requested type T; error is
    /// returned otherwise.
    /// ODataId passed in should be a URI of resource collection as defined by Redfish spec. Resource collection's @odata.type
    /// ends with suffix Collection. For example, @odata.type of EthernetInfetface collection is
    ///
    ///    "#EthernetInterfaceCollection.EthernetInterfaceCollection"
    ///
    /// This collection can only be connverted to ResourceCollection<EthernetInterface>
    ///
    /// This method fetches all member objects of the collection in a single request by appending
    /// '?$expand=.($levels=1)' to the URI as defined by the spec.
    async fn get_collection(&self, id: ODataId) -> Result<Collection, RedfishError>;

    /// This method will change the boot order so that system will attempt to boot from the dpu first.
    /// Method will make a platforn specifc best errert to identify the dpu specific boot option.
    /// It will choose Uefi Http IPv4 option if any.
    /// If dpu's mac can be passed in as  mac_address to identify the dpu, otherwise method will attempt to find the dpu
    /// by enumeration NetworkAdapters and associated resources.
    async fn set_boot_order_dpu_first(
        &self,
        mac_address: &str,
    ) -> Result<Option<String>, RedfishError>;

    async fn clear_uefi_password(
        &self,
        current_uefi_password: &str,
    ) -> Result<Option<String>, RedfishError>;

    async fn get_update_service(&self) -> Result<UpdateService, RedfishError>;

    async fn get_base_mac_address(&self) -> Result<Option<String>, RedfishError>;

    async fn lockdown_bmc(&self, target: EnabledDisabled) -> Result<(), RedfishError>;

    async fn is_ipmi_over_lan_enabled(&self) -> Result<bool, RedfishError>;

    async fn enable_ipmi_over_lan(&self, target: EnabledDisabled) -> Result<(), RedfishError>;

    async fn enable_rshim_bmc(&self) -> Result<(), RedfishError>;

    // Only applicable to Vikings
    async fn clear_nvram(&self) -> Result<(), RedfishError>;

    // Only applicable to DPUs
    async fn get_nic_mode(&self) -> Result<Option<NicMode>, RedfishError>;

    // Only applicable to DPUs
    async fn set_nic_mode(&self, mode: NicMode) -> Result<(), RedfishError>;

    /// Enable infinite boot
    async fn enable_infinite_boot(&self) -> Result<(), RedfishError>;

    /// Check if infinite boot is enabled
    async fn is_infinite_boot_enabled(&self) -> Result<Option<bool>, RedfishError>;

    // Only applicable to DPUs
    async fn set_host_rshim(&self, enabled: EnabledDisabled) -> Result<(), RedfishError>;

    // Only applicable to DPUs
    async fn get_host_rshim(&self) -> Result<Option<EnabledDisabled>, RedfishError>;

    // Only applicable to Dells
    async fn set_idrac_lockdown(&self, enabled: EnabledDisabled) -> Result<(), RedfishError>;

    // Only applicable to Dells
    async fn get_boss_controller(&self) -> Result<Option<String>, RedfishError>;

    // Only applicable to Dells
    async fn decommission_storage_controller(
        &self,
        controller_id: &str,
    ) -> Result<Option<String>, RedfishError>;

    // Only applicable to Dells
    async fn create_storage_volume(
        &self,
        controller_id: &str,
        volume_name: &str,
        raid_type: &str,
    ) -> Result<Option<String>, RedfishError>;

    fn ac_powercycle_supported_by_power(&self) -> bool;

    /// Check if the boot order is configured as we expect (Network boot)
    async fn is_boot_order_setup(&self, mac_address: &str) -> Result<bool, RedfishError>;

    /// Returns info about component integrity
    async fn get_component_integrities(&self) -> Result<ComponentIntegrities, RedfishError>;

    /// Returns info about component integrity
    async fn get_firmware_for_component(
        &self,
        component_integrity_id: &str,
    ) -> Result<SoftwareInventory, RedfishError>;

    /// Component/evidence apis are taking URL as of now since not sure if all vendors keep
    /// certificate and evidence in chassis/same place. Once tested with all vendors, the url can
    /// be changed into id and device parameters.
    /// Fetches component certificate
    async fn get_component_ca_certificate(
        &self,
        url: &str,
    ) -> Result<model::component_integrity::CaCertificate, RedfishError>;

    /// Trigger evidence collection
    async fn trigger_evidence_collection(
        &self,
        url: &str,
        nonce: &str,
    ) -> Result<Task, RedfishError>;

    /// Fetches component certificate
    async fn get_evidence(
        &self,
        url: &str,
    ) -> Result<model::component_integrity::Evidence, RedfishError>;
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum Boot {
    Pxe,
    HardDisk,
    UefiHttp,
}

impl fmt::Display for Boot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// The current status of something (lockdown, serial_console), saying whether it has been enabled,
/// disabled, or the necessary settings are only partially applied.
#[derive(Clone, PartialEq, Debug)]
pub struct Status {
    pub(crate) status: StatusInternal,
    pub(crate) message: String,
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
enum StatusInternal {
    Enabled,
    Partial,
    Disabled,
}

impl fmt::Display for StatusInternal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

/// BMC User Roles
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub enum RoleId {
    Administrator,
    Operator,
    ReadOnly,
    NoAccess,
}

impl fmt::Display for RoleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

impl Status {
    /// Did enabling complete successfully?
    pub fn is_fully_enabled(&self) -> bool {
        self.status == StatusInternal::Enabled
    }

    /// Did disabling complete successfuly (or thing was never enabled in the first place)?
    pub fn is_fully_disabled(&self) -> bool {
        self.status == StatusInternal::Disabled
    }

    /// Did lockdown enable/disable fail part way through, so we are partially locked?
    pub fn is_partially_enabled(&self) -> bool {
        self.status == StatusInternal::Partial
    }

    /// A vendor specific message detailing the individual status of the parts that are needed to
    /// enable or disabled. Format of message will change, do not parse.
    pub fn message(&self) -> &str {
        &self.message
    }

    // build_fake creates a Status for use in test environments, as its details are private.
    pub fn build_fake(enabled: EnabledDisabled) -> Self {
        Self {
            status: match enabled {
                EnabledDisabled::Enabled => StatusInternal::Enabled,
                EnabledDisabled::Disabled => StatusInternal::Disabled,
            },
            message: "Fake".to_string(),
        }
    }
}

#[derive(Debug)]
pub struct MachineSetupStatus {
    pub is_done: bool,
    pub diffs: Vec<MachineSetupDiff>,
}

impl fmt::Display for MachineSetupStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_done {
            write!(f, "OK")
        } else {
            write!(
                f,
                "Mismatch: {:?}",
                self.diffs
                    .iter()
                    .map(|d| d.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )?;
            Ok(())
        }
    }
}

#[derive(Debug)]
pub struct MachineSetupDiff {
    pub key: String,
    pub expected: String,
    pub actual: String,
}

impl fmt::Display for MachineSetupDiff {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} is '{}' expected '{}'",
            self.key, self.actual, self.expected
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "lowercase")] // No tag requried - this is not nested
pub enum JobState {
    Scheduled,
    ScheduledWithErrors,
    Running,
    Completed,
    CompletedWithErrors,
    Unknown,
}

impl JobState {
    fn from_str(s: &str) -> JobState {
        match s {
            "Scheduled" => JobState::Scheduled,
            "Running" => JobState::Running,
            "Completed" => JobState::Completed,
            "CompletedWithErrors" => JobState::CompletedWithErrors,
            _ => JobState::Unknown,
        }
    }
}

#[derive(
    Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Hash, Copy, clap::ValueEnum, Default,
)]
#[serde(rename_all = "lowercase")]
pub enum BiosProfileType {
    #[default]
    Performance,
    PowerEfficiency,
}

pub type BiosProfileProfiles = HashMap<BiosProfileType, HashMap<String, serde_json::Value>>;
pub type BiosProfileModel = HashMap<String, BiosProfileProfiles>;
pub type BiosProfileVendor = HashMap<RedfishVendor, BiosProfileModel>;

// Simplify model names so that we can put them in toml files as categories
pub fn model_coerce(original: &str) -> String {
    str::replace(original, " ", "_")
}
