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
use std::{collections::HashMap, fmt, future::Future, path::Path, pin::Pin, time::Duration};

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
mod ami;
mod dell;
mod error;
mod hpe;
pub mod jsonmap;
mod lenovo;

mod liteon_powershelf;
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

/// Reexported of reqwest for types needed in
/// RedfishClientPoolBuilder.
pub use reqwest;

use crate::model::certificate::Certificate;
use crate::model::component_integrity::ComponentIntegrities;
use crate::model::power::Power;
use crate::model::sel::LogEntry;
use crate::model::storage::Drives;
use crate::model::thermal::Thermal;

pub type RedfishFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Interface to a BMC Redfish server. All calls will include one or more HTTP network calls.
pub trait Redfish: Send + Sync + 'static {
    /// Rename a user
    fn change_username<'a>(
        &'a self,
        old_name: &'a str,
        new_name: &'a str,
    ) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// Change password by username
    /// This looks up the ID for given username before calling change_password_by_id.
    /// That lookup makes it unsuitable for changing the initial password on
    /// PasswordChangeRequired.
    fn change_password<'a>(
        &'a self,
        username: &'a str,
        new_pass: &'a str,
    ) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// Change password by id
    fn change_password_by_id<'a>(
        &'a self,
        account_id: &'a str,
        new_pass: &'a str,
    ) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// List current user accounts
    fn get_accounts<'a>(&'a self) -> RedfishFuture<'a, Result<Vec<ManagerAccount>, RedfishError>>;

    /// Create a new user
    fn create_user<'a>(
        &'a self,
        username: &'a str,
        password: &'a str,
        role_id: RoleId,
    ) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// Delete a BMC user
    fn delete_user<'a>(&'a self, username: &'a str) -> RedfishFuture<'a, Result<(), RedfishError>>;

    // Get firmware version for particular firmware inventory id
    fn get_firmware<'a>(
        &'a self,
        id: &'a str,
    ) -> RedfishFuture<'a, Result<SoftwareInventory, RedfishError>>;

    // Get software inventory collection
    fn get_software_inventories<'a>(
        &'a self,
    ) -> RedfishFuture<'a, Result<Vec<String>, RedfishError>>;

    // List all Tasks
    fn get_tasks<'a>(&'a self) -> RedfishFuture<'a, Result<Vec<String>, RedfishError>>;

    // Get information about a task
    fn get_task<'a>(&'a self, id: &'a str) -> RedfishFuture<'a, Result<Task, RedfishError>>;

    /// Is this thing even on?
    fn get_power_state<'a>(&'a self) -> RedfishFuture<'a, Result<PowerState, RedfishError>>;

    /// Returns info about operations that the service supports.
    fn get_service_root<'a>(&'a self) -> RedfishFuture<'a, Result<ServiceRoot, RedfishError>>;

    /// Returns info about available computer systems.
    fn get_systems<'a>(&'a self) -> RedfishFuture<'a, Result<Vec<String>, RedfishError>>;

    /// Returns info about computer system.
    fn get_system<'a>(&'a self) -> RedfishFuture<'a, Result<ComputerSystem, RedfishError>>;

    /// Returns info about available managers.
    fn get_managers<'a>(&'a self) -> RedfishFuture<'a, Result<Vec<String>, RedfishError>>;

    /// Returns info about managers
    fn get_manager<'a>(&'a self) -> RedfishFuture<'a, Result<Manager, RedfishError>>;

    /// Get Secure Boot state
    fn get_secure_boot<'a>(&'a self) -> RedfishFuture<'a, Result<SecureBoot, RedfishError>>;

    /// Disables Secure Boot
    fn disable_secure_boot<'a>(&'a self) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// Enables Secure Boot
    fn enable_secure_boot<'a>(&'a self) -> RedfishFuture<'a, Result<(), RedfishError>>;

    fn get_secure_boot_certificate<'a>(
        &'a self,
        database_id: &'a str,
        certificate_id: &'a str,
    ) -> RedfishFuture<'a, Result<Certificate, RedfishError>>;

    fn get_secure_boot_certificates<'a>(
        &'a self,
        database_id: &'a str,
    ) -> RedfishFuture<'a, Result<Vec<String>, RedfishError>>;

    /// Adds certificate to secure boot DB
    /// database_id: "db" for database, "pk" for PK database
    /// Need to reboot DPU for UEFI Redfish client to execute.
    fn add_secure_boot_certificate<'a>(
        &'a self,
        pem_cert: &'a str,
        database_id: &'a str,
    ) -> RedfishFuture<'a, Result<Task, RedfishError>>;

    /// Power supplies and voltages metrics
    fn get_power_metrics<'a>(&'a self) -> RedfishFuture<'a, Result<Power, RedfishError>>;

    /// Change power state: on, off, reboot, etc
    fn power<'a>(
        &'a self,
        action: SystemPowerControl,
    ) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// Reboot the BMC itself
    fn bmc_reset<'a>(&'a self) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// Reset Chassis
    fn chassis_reset<'a>(
        &'a self,
        chassis_id: &'a str,
        reset_type: SystemPowerControl,
    ) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// Reset BMC to the factory defaults.
    fn bmc_reset_to_defaults<'a>(&'a self) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// Fans and temperature sensors
    fn get_thermal_metrics<'a>(&'a self) -> RedfishFuture<'a, Result<Thermal, RedfishError>>;

    /// Voltage, temperature, etc sensors for gpus if they exist.
    fn get_gpu_sensors<'a>(&'a self) -> RedfishFuture<'a, Result<Vec<GPUSensors>, RedfishError>>;

    /// get system event log similar to ipmitool sel
    fn get_system_event_log<'a>(&'a self)
        -> RedfishFuture<'a, Result<Vec<LogEntry>, RedfishError>>;

    /// get bmc event log (power events, etc.)
    fn get_bmc_event_log<'a>(
        &'a self,
        from: Option<chrono::DateTime<chrono::Utc>>,
    ) -> RedfishFuture<'a, Result<Vec<LogEntry>, RedfishError>>;

    /// get drives metrics
    fn get_drives_metrics<'a>(&'a self) -> RedfishFuture<'a, Result<Vec<Drives>, RedfishError>>;

    /// Sets up a reasonable UEFI configuration.
    /// remember to call lockdown() afterwards to secure the server
    /// - boot_interface_mac: MAC Address of the NIC you wish to boot from
    ///   If not given we look for a Mellanox Bluefield DPU and use that.
    ///   Not applicable to Supermicro and the DPU itself.
    ///   bios_profiles: Map of vendor/model (with spaces replaced by underscores)/profile/type
    ///   to extra settings; expected to come from config rather than hardcoded.
    ///   selected_profile: Profile to use (if present)
    ///
    /// Returns Ok(Some(job_id)) when the vendor creates a job for the BIOS PATCH (e.g. Dell);
    ///
    /// Ok(None) when no job is created. Caller should wait for job completion before configuring boot order.
    fn machine_setup<'a>(
        &'a self,
        boot_interface_mac: Option<&'a str>,
        bios_profiles: &'a BiosProfileVendor,
        selected_profile: BiosProfileType,
        oem_manager_profiles: &'a BiosProfileVendor,
    ) -> RedfishFuture<'a, Result<Option<String>, RedfishError>>;

    /// Is everything that machine_setup does already done?
    fn machine_setup_status<'a>(
        &'a self,
        boot_interface_mac: Option<&'a str>,
    ) -> RedfishFuture<'a, Result<MachineSetupStatus, RedfishError>>;

    /// Check if only the BIOS/BMC setup is done
    fn is_bios_setup<'a>(
        &'a self,
        boot_interface_mac: Option<&'a str>,
    ) -> RedfishFuture<'a, Result<bool, RedfishError>>;

    /// Apply a standard BMC password policy. This varies a lot by vendor,
    /// but at a minimum we want passwords to never expire, because our BMCs are
    /// not actively used by humans.
    fn set_machine_password_policy<'a>(&'a self) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// Lock the BIOS and BMC ready for tenant use. Disabled reverses the changes.
    fn lockdown<'a>(
        &'a self,
        target: EnabledDisabled,
    ) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// Are the BIOS and BMC currently locked down?
    fn lockdown_status<'a>(&'a self) -> RedfishFuture<'a, Result<Status, RedfishError>>;

    /// Enable SSH access to console
    fn setup_serial_console<'a>(&'a self) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// Is the serial console setup?
    fn serial_console_status<'a>(&'a self) -> RedfishFuture<'a, Result<Status, RedfishError>>;

    /// Show available boot options
    fn get_boot_options<'a>(&'a self) -> RedfishFuture<'a, Result<BootOptions, RedfishError>>;

    /// Show available boot options
    fn get_boot_option<'a>(
        &'a self,
        option_id: &'a str,
    ) -> RedfishFuture<'a, Result<BootOption, RedfishError>>;

    /// Boot a single time of the given target. Does not change boot order after that.
    fn boot_once<'a>(&'a self, target: Boot) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// Change boot order putting this target first
    fn boot_first<'a>(&'a self, target: Boot) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// Change boot order by setting boot array.
    fn change_boot_order<'a>(
        &'a self,
        boot_array: Vec<String>,
    ) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// Reset and enable the TPM
    fn clear_tpm<'a>(&'a self) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// List PCIe devices
    fn pcie_devices<'a>(&'a self) -> RedfishFuture<'a, Result<Vec<PCIeDevice>, RedfishError>>;

    /// Update BMC firmware
    fn update_firmware<'a>(
        &'a self,
        filename: tokio::fs::File,
    ) -> RedfishFuture<'a, Result<Task, RedfishError>>;

    /// Update UEFI firmware, returns a task ID
    fn update_firmware_multipart<'a>(
        &'a self,
        firmware: &'a Path,
        reboot: bool,
        timeout: Duration,
        component_type: ComponentType,
    ) -> RedfishFuture<'a, Result<String, RedfishError>>;

    /// This action shall update installed software components in a software image file located at an ImageURI parameter-specified URI.
    /// image_uri - The URI of the software image to install.
    /// transfer_protocol - The network protocol that the update service uses to retrieve the software image file located at the URI provided in ImageURI.
    /// This parameter is ignored if the URI provided in ImageURI contains a scheme.
    /// targets - An array of URIs that indicate where to apply the update image.
    fn update_firmware_simple_update<'a>(
        &'a self,
        image_uri: &'a str,
        targets: Vec<String>,
        transfer_protocol: TransferProtocolType,
    ) -> RedfishFuture<'a, Result<Task, RedfishError>>;

    /*
     * Diagnostic calls
     */
    /// All the BIOS values for this provider. Very OEM specific.
    fn bios<'a>(
        &'a self,
    ) -> RedfishFuture<'a, Result<HashMap<String, serde_json::Value>, RedfishError>>;

    /// Modify specific BIOS values.  Also very OEM and model specific.
    fn set_bios<'a>(
        &'a self,
        values: HashMap<String, serde_json::Value>,
    ) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// Reset BIOS to factory settings
    fn reset_bios<'a>(&'a self) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// Pending BIOS attributes. Changes that were requested but not applied yet because
    /// they need a reboot.
    fn pending<'a>(
        &'a self,
    ) -> RedfishFuture<'a, Result<HashMap<String, serde_json::Value>, RedfishError>>;

    /// Clear all pending jobs
    fn clear_pending<'a>(&'a self) -> RedfishFuture<'a, Result<(), RedfishError>>;

    // List all Network Device Functions of a given Chassis
    fn get_network_device_functions<'a>(
        &'a self,
        chassis_id: &'a str,
    ) -> RedfishFuture<'a, Result<Vec<String>, RedfishError>>;

    // Get Network Device Function details
    fn get_network_device_function<'a>(
        &'a self,
        chassis_id: &'a str,
        id: &'a str,
        port: Option<&'a str>,
    ) -> RedfishFuture<'a, Result<NetworkDeviceFunction, RedfishError>>;

    // List all Chassises
    fn get_chassis_all<'a>(&'a self) -> RedfishFuture<'a, Result<Vec<String>, RedfishError>>;

    // Get Chassis details
    fn get_chassis<'a>(&'a self, id: &'a str) -> RedfishFuture<'a, Result<Chassis, RedfishError>>;

    // Get Chassis Assembly details
    fn get_chassis_assembly<'a>(
        &'a self,
        chassis_id: &'a str,
    ) -> RedfishFuture<'a, Result<Assembly, RedfishError>>;

    // List all Network Adapters for the specific Chassis
    fn get_chassis_network_adapters<'a>(
        &'a self,
        chassis_id: &'a str,
    ) -> RedfishFuture<'a, Result<Vec<String>, RedfishError>>;

    // Get Network Adapter details for the specific Chassis and Network Adapter
    fn get_chassis_network_adapter<'a>(
        &'a self,
        chassis_id: &'a str,
        id: &'a str,
    ) -> RedfishFuture<'a, Result<NetworkAdapter, RedfishError>>;

    // List all Base Network Adapters for the specific Chassis
    // Only implemented in iLO5
    fn get_base_network_adapters<'a>(
        &'a self,
        system_id: &'a str,
    ) -> RedfishFuture<'a, Result<Vec<String>, RedfishError>>;

    // Get Base Network Adapter details for the specific Chassis and Network Adapter
    // Only implemented in iLO5
    fn get_base_network_adapter<'a>(
        &'a self,
        system_id: &'a str,
        id: &'a str,
    ) -> RedfishFuture<'a, Result<NetworkAdapter, RedfishError>>;

    // List all High Speed Ports of a given Chassis
    fn get_ports<'a>(
        &'a self,
        chassis_id: &'a str,
        network_adapter: &'a str,
    ) -> RedfishFuture<'a, Result<Vec<String>, RedfishError>>;

    // Get High Speed Port details
    fn get_port<'a>(
        &'a self,
        chassis_id: &'a str,
        network_adapter: &'a str,
        id: &'a str,
    ) -> RedfishFuture<'a, Result<NetworkPort, RedfishError>>;

    // List all Ethernet Interfaces for the default `Manager`
    fn get_manager_ethernet_interfaces<'a>(
        &'a self,
    ) -> RedfishFuture<'a, Result<Vec<String>, RedfishError>>;

    // Get Ethernet Interface details for an interface on the default `Manager`
    fn get_manager_ethernet_interface<'a>(
        &'a self,
        id: &'a str,
    ) -> RedfishFuture<'a, Result<EthernetInterface, RedfishError>>;

    // List all Ethernet Interfaces for the default `System`
    fn get_system_ethernet_interfaces<'a>(
        &'a self,
    ) -> RedfishFuture<'a, Result<Vec<String>, RedfishError>>;

    // Get Ethernet Interface details for an interface on the default `System`
    fn get_system_ethernet_interface<'a>(
        &'a self,
        id: &'a str,
    ) -> RedfishFuture<'a, Result<EthernetInterface, RedfishError>>;

    // Change UEFI Password
    fn change_uefi_password<'a>(
        &'a self,
        current_uefi_password: &'a str,
        new_uefi_password: &'a str,
    ) -> RedfishFuture<'a, Result<Option<String>, RedfishError>>;

    fn get_job_state<'a>(
        &'a self,
        job_id: &'a str,
    ) -> RedfishFuture<'a, Result<JobState, RedfishError>>;

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
    fn get_resource<'a>(&'a self, id: ODataId)
        -> RedfishFuture<'a, Result<Resource, RedfishError>>;

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
    fn get_collection<'a>(
        &'a self,
        id: ODataId,
    ) -> RedfishFuture<'a, Result<Collection, RedfishError>>;

    /// This method will change the boot order so that system will attempt to boot from the dpu first.
    /// Method will make a platforn specifc best errert to identify the dpu specific boot option.
    /// It will choose Uefi Http IPv4 option if any.
    /// If dpu's mac can be passed in as  mac_address to identify the dpu, otherwise method will attempt to find the dpu
    /// by enumeration NetworkAdapters and associated resources.
    fn set_boot_order_dpu_first<'a>(
        &'a self,
        mac_address: &'a str,
    ) -> RedfishFuture<'a, Result<Option<String>, RedfishError>>;

    fn clear_uefi_password<'a>(
        &'a self,
        current_uefi_password: &'a str,
    ) -> RedfishFuture<'a, Result<Option<String>, RedfishError>>;

    fn get_update_service<'a>(&'a self) -> RedfishFuture<'a, Result<UpdateService, RedfishError>>;

    fn get_base_mac_address<'a>(
        &'a self,
    ) -> RedfishFuture<'a, Result<Option<String>, RedfishError>>;

    fn lockdown_bmc<'a>(
        &'a self,
        target: EnabledDisabled,
    ) -> RedfishFuture<'a, Result<(), RedfishError>>;

    fn is_ipmi_over_lan_enabled<'a>(&'a self) -> RedfishFuture<'a, Result<bool, RedfishError>>;

    fn enable_ipmi_over_lan<'a>(
        &'a self,
        target: EnabledDisabled,
    ) -> RedfishFuture<'a, Result<(), RedfishError>>;

    fn enable_rshim_bmc<'a>(&'a self) -> RedfishFuture<'a, Result<(), RedfishError>>;

    // Only applicable to Vikings
    fn clear_nvram<'a>(&'a self) -> RedfishFuture<'a, Result<(), RedfishError>>;

    // Only applicable to DPUs
    fn get_nic_mode<'a>(&'a self) -> RedfishFuture<'a, Result<Option<NicMode>, RedfishError>>;

    // Only applicable to DPUs
    fn set_nic_mode<'a>(&'a self, mode: NicMode) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// Enable infinite boot
    fn enable_infinite_boot<'a>(&'a self) -> RedfishFuture<'a, Result<(), RedfishError>>;

    /// Check if infinite boot is enabled
    fn is_infinite_boot_enabled<'a>(
        &'a self,
    ) -> RedfishFuture<'a, Result<Option<bool>, RedfishError>>;

    // Only applicable to DPUs
    fn set_host_rshim<'a>(
        &'a self,
        enabled: EnabledDisabled,
    ) -> RedfishFuture<'a, Result<(), RedfishError>>;

    // Only applicable to DPUs
    fn get_host_rshim<'a>(
        &'a self,
    ) -> RedfishFuture<'a, Result<Option<EnabledDisabled>, RedfishError>>;

    // Only applicable to Dells
    fn set_idrac_lockdown<'a>(
        &'a self,
        enabled: EnabledDisabled,
    ) -> RedfishFuture<'a, Result<(), RedfishError>>;

    // Only applicable to Dells
    fn get_boss_controller<'a>(&'a self)
        -> RedfishFuture<'a, Result<Option<String>, RedfishError>>;

    // Only applicable to Dells
    fn decommission_storage_controller<'a>(
        &'a self,
        controller_id: &'a str,
    ) -> RedfishFuture<'a, Result<Option<String>, RedfishError>>;

    // Only applicable to Dells
    fn create_storage_volume<'a>(
        &'a self,
        controller_id: &'a str,
        volume_name: &'a str,
    ) -> RedfishFuture<'a, Result<Option<String>, RedfishError>>;

    fn ac_powercycle_supported_by_power(&self) -> bool;

    /// Check if the boot order is configured as we expect (Network boot)
    fn is_boot_order_setup<'a>(
        &'a self,
        mac_address: &'a str,
    ) -> RedfishFuture<'a, Result<bool, RedfishError>>;

    /// Returns info about component integrity
    fn get_component_integrities<'a>(
        &'a self,
    ) -> RedfishFuture<'a, Result<ComponentIntegrities, RedfishError>>;

    /// Returns info about component integrity
    fn get_firmware_for_component<'a>(
        &'a self,
        component_integrity_id: &'a str,
    ) -> RedfishFuture<'a, Result<SoftwareInventory, RedfishError>>;

    /// Component/evidence apis are taking URL as of now since not sure if all vendors keep
    /// certificate and evidence in chassis/same place. Once tested with all vendors, the url can
    /// be changed into id and device parameters.
    /// Fetches component certificate
    fn get_component_ca_certificate<'a>(
        &'a self,
        url: &'a str,
    ) -> RedfishFuture<'a, Result<model::component_integrity::CaCertificate, RedfishError>>;

    /// Trigger evidence collection
    fn trigger_evidence_collection<'a>(
        &'a self,
        url: &'a str,
        nonce: &'a str,
    ) -> RedfishFuture<'a, Result<Task, RedfishError>>;

    /// Fetches component certificate
    fn get_evidence<'a>(
        &'a self,
        url: &'a str,
    ) -> RedfishFuture<'a, Result<model::component_integrity::Evidence, RedfishError>>;

    // Sets the host privilege level for a DPU
    fn set_host_privilege_level<'a>(
        &'a self,
        level: HostPrivilegeLevel,
    ) -> RedfishFuture<'a, Result<(), RedfishError>>;

    // Sets the timezone to UTC
    // Only applicable to Dells
    fn set_utc_timezone<'a>(&'a self) -> RedfishFuture<'a, Result<(), RedfishError>>;
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
