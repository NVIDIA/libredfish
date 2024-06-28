use std::{collections::HashMap, fmt, path::Path, time::Duration};

pub mod model;
use model::account_service::ManagerAccount;
pub use model::chassis::{Chassis, NetworkAdapter};
pub use model::ethernet_interface::EthernetInterface;
pub use model::network_device_function::NetworkDeviceFunction;
use model::oem::nvidia_dpu::{HostPrivilegeLevel, InternalCPUModel};
pub use model::port::NetworkPort;
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
mod hpe;
mod lenovo;
mod network;
mod nvidia_dpu;
mod nvidia_viking;
mod supermicro;
pub use network::{Endpoint, RedfishClientPool, RedfishClientPoolBuilder, REDFISH_ENDPOINT};
pub mod standard;
pub use error::RedfishError;

use crate::model::power::Power;
use crate::model::sel::LogEntry;
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

    /// Adds certificate to secure boot DB
    /// Need to reboot DPU for UEFI Redfish client to execute.
    async fn add_secure_boot_certificate(&self, pem_cert: &str) -> Result<Task, RedfishError>;

    /// Power supplies and voltages metrics
    async fn get_power_metrics(&self) -> Result<Power, RedfishError>;

    /// Change power state: on, off, reboot, etc
    async fn power(&self, action: SystemPowerControl) -> Result<(), RedfishError>;

    /// call this to setup bios and bmc
    async fn machine_setup(&self) -> Result<(), RedfishError>;

    /// Reboot the BMC itself
    async fn bmc_reset(&self) -> Result<(), RedfishError>;

    /// Reset BMC to the factory defaults.
    async fn bmc_reset_to_defaults(&self) -> Result<(), RedfishError>;

    /// Fans and temperature sensors
    async fn get_thermal_metrics(&self) -> Result<Thermal, RedfishError>;

    /// get system event log similar to ipmitool sel
    async fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError>;

    /// Is everything that machine_setup does already done?
    async fn machine_setup_status(&self) -> Result<MachineSetupStatus, RedfishError>;

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
    ) -> Result<String, RedfishError>;

    /*
     * Diagnostic calls
     */
    /// All the BIOS values for this provider. Very OEM specific.
    async fn bios(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError>;

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
    ) -> Result<NetworkDeviceFunction, RedfishError>;

    // List all Chassises
    async fn get_chassis_all(&self) -> Result<Vec<String>, RedfishError>;

    // Get Chassis details
    async fn get_chassis(&self, id: &str) -> Result<Chassis, RedfishError>;

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

    // List all High Speed Ports of a given Chassis
    async fn get_ports(&self, chassis_id: &str) -> Result<Vec<String>, RedfishError>;

    // Get High Speed Port details
    async fn get_port(&self, chassis_id: &str, id: &str) -> Result<NetworkPort, RedfishError>;

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
}

// When Carbide drops it's `IpmiCommand.launch_command` background job system, we can
// remove the Serialize and Deserialize here.
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
