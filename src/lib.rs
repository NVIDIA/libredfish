use std::collections::HashMap;
use std::fmt;
use std::fs::File;

pub mod model;
pub use model::chassis::{Chassis, ChassisCollection};
pub use model::ethernet_interface::{EthernetInterface, EthernetInterfaceCollection};
pub use model::network_device_function::{NetworkDeviceFunction, NetworkDeviceFunctionCollection};
use model::oem::nvidia::{HostPrivilegeLevel, InternalCPUModel};
pub use model::port::{NetworkPort, NetworkPortCollection};
use model::service_root::ServiceRoot;
use model::software_inventory::SoftwareInventory;
pub use model::system::{BootOptions, PCIeDevice, PowerState, SystemPowerControl, Systems};
use model::task::Task;
pub use model::EnabledDisabled;
use model::Manager;
use model::{secure_boot::SecureBoot, BootOption, ComputerSystem};
use serde::{Deserialize, Serialize};

mod dell;
mod error;
mod lenovo;
mod network;
mod nvidia;
pub use network::{Endpoint, RedfishClientPool, RedfishClientPoolBuilder, REDFISH_ENDPOINT};
pub mod standard;
pub use error::RedfishError;

use crate::model::power::Power;
use crate::model::sel::LogEntry;
use crate::model::thermal::Thermal;

/// Interface to a BMC Redfish server. All calls will include one or more HTTP network calls.
pub trait Redfish: Send + Sync + 'static {
    /// Change password for the user
    fn change_password(&self, user: &str, new: &str) -> Result<(), RedfishError>;

    /// Create a new user
    fn create_user(
        &self,
        username: &str,
        password: &str,
        role_id: RoleId,
    ) -> Result<(), RedfishError>;

    // Get firmware version for particular firmware inventory id
    fn get_firmware(&self, id: &str) -> Result<SoftwareInventory, RedfishError>;

    // Get software inventory collection
    fn get_software_inventories(&self) -> Result<Vec<String>, RedfishError>;

    // List all Tasks
    fn get_tasks(&self) -> Result<Vec<String>, RedfishError>;

    // Get information about a task
    fn get_task(&self, id: &str) -> Result<Task, RedfishError>;

    /// Is this thing even on?
    fn get_power_state(&self) -> Result<PowerState, RedfishError>;

    /// Returns info about operations that the service supports.
    fn get_service_root(&self) -> Result<ServiceRoot, RedfishError>;

    /// Returns info about available computer systems.
    fn get_systems(&self) -> Result<Vec<String>, RedfishError>;

    /// Returns info about computer system.
    fn get_system(&self) -> Result<ComputerSystem, RedfishError>;

    /// Returns info about available managers.
    fn get_managers(&self) -> Result<Vec<String>, RedfishError>;

    /// Returns info about managers
    fn get_manager(&self) -> Result<Manager, RedfishError>;

    /// Get Secure Boot state
    fn get_secure_boot(&self) -> Result<SecureBoot, RedfishError>;

    /// Disables Secure Boot
    fn disable_secure_boot(&self) -> Result<(), RedfishError>;

    /// Enables Secure Boot
    fn enable_secure_boot(&self) -> Result<(), RedfishError>;

    /// Adds certificate to secure boot DB
    /// Need to reboot DPU for UEFI Redfish client to execute.
    fn add_secure_boot_certificate(&self, pem_cert: &str) -> Result<Task, RedfishError>;

    /// Power supplies and voltages metrics
    fn get_power_metrics(&self) -> Result<Power, RedfishError>;

    /// Change power state: on, off, reboot, etc
    fn power(&self, action: SystemPowerControl) -> Result<(), RedfishError>;

    /// call this to setup bios and bmc
    fn machine_setup(&self) -> Result<(), RedfishError>;

    /// Reboot the BMC itself
    fn bmc_reset(&self) -> Result<(), RedfishError>;

    /// Reset BMC to the factory defaults.
    fn bmc_reset_to_defaults(&self) -> Result<(), RedfishError>;

    /// Fans and temperature sensors
    fn get_thermal_metrics(&self) -> Result<Thermal, RedfishError>;

    /// get system event log similar to ipmitool sel
    fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError>;

    /// Lock the BIOS and BMC ready for tenant use. Disabled reverses the changes.
    fn lockdown(&self, target: EnabledDisabled) -> Result<(), RedfishError>;

    /// Are the BIOS and BMC currently locked down?
    fn lockdown_status(&self) -> Result<Status, RedfishError>;

    /// Enable SSH access to console
    fn setup_serial_console(&self) -> Result<(), RedfishError>;

    /// Is the serial console setup?
    fn serial_console_status(&self) -> Result<Status, RedfishError>;

    /// Show available boot options
    fn get_boot_options(&self) -> Result<BootOptions, RedfishError>;

    /// Show available boot options
    fn get_boot_option(&self, option_id: &str) -> Result<BootOption, RedfishError>;

    /// Boot a single time of the given target. Does not change boot order after that.
    fn boot_once(&self, target: Boot) -> Result<(), RedfishError>;

    /// Change boot order putting this target first
    fn boot_first(&self, target: Boot) -> Result<(), RedfishError>;

    /// Change boot order by setting boot array.
    fn change_boot_order(&self, boot_array: Vec<String>) -> Result<(), RedfishError>;

    /// Reset and enable the TPM
    fn clear_tpm(&self) -> Result<(), RedfishError>;

    /// List PCIe devices
    fn pcie_devices(&self) -> Result<Vec<PCIeDevice>, RedfishError>;

    /// Update firmware
    fn update_firmware(&self, firmware: File) -> Result<Task, RedfishError>;

    /*
     * Diagnostic calls
     */
    /// All the BIOS values for this provider. Very OEM specific.
    fn bios(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError>;

    /// Pending BIOS attributes. Changes that were requested but not applied yet because
    /// they need a reboot.
    fn pending(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError>;

    /// Clear all pending jobs
    fn clear_pending(&self) -> Result<(), RedfishError>;

    // List all Network Device Functions of a given Chassis
    fn get_network_device_functions(&self, chassis_id: &str) -> Result<Vec<String>, RedfishError>;

    // Get Network Device Function details
    fn get_network_device_function(
        &self,
        chassis_id: &str,
        id: &str,
    ) -> Result<NetworkDeviceFunction, RedfishError>;

    // List all Chassises
    fn get_chassis_all(&self) -> Result<Vec<String>, RedfishError>;

    // Get Chassis details
    fn get_chassis(&self, id: &str) -> Result<Chassis, RedfishError>;

    // List all High Speed Ports of a given Chassis
    fn get_ports(&self, chassis_id: &str) -> Result<Vec<String>, RedfishError>;

    // Get High Speed Port details
    fn get_port(&self, chassis_id: &str, id: &str) -> Result<NetworkPort, RedfishError>;

    // List all Ethernet Interfaces
    fn get_ethernet_interfaces(&self) -> Result<Vec<String>, RedfishError>;

    // Get Ethernet Interface details
    fn get_ethernet_interface(&self, id: &str) -> Result<EthernetInterface, RedfishError>;

    // Change UEFI Password
    fn change_uefi_password(
        &self,
        current_uefi_password: &str,
        new_uefi_password: &str,
    ) -> Result<(), RedfishError>;

    // Set Internal CPU Mode
    fn set_internal_cpu_model(&self, model: InternalCPUModel) -> Result<(), RedfishError>;

    // Set Internal Host Privilege Mode
    fn set_host_privilege_level(&self, level: HostPrivilegeLevel) -> Result<(), RedfishError>;
}

// When Carbide drops it's `IpmiCommand.launch_command` background job system, we can
// remove the Serialize and Deserialize here.
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub enum Boot {
    Pxe,
    HardDisk,
    UefiHttp,
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
}
