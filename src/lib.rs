use std::collections::HashMap;
use std::fs::File;

pub mod model;
pub use model::network_device_function::{NetworkDeviceFunction, NetworkDeviceFunctionCollection};
pub use model::chassis::{Chassis, ChassisCollection};
pub use model::port::{NetworkPort, NetworkPortCollection};
pub use model::ethernet_interface::{EthernetInterface, EthernetInterfaceCollection};
pub use model::system::{BootOptions, PCIeDevice, PowerState, SystemPowerControl, Systems};
use model::task::Task;
pub use model::EnabledDisabled;
use model::{
    secure_boot::SecureBoot,
    software_inventory::{SoftwareInventory, SoftwareInventoryCollection},
    BootOption, ComputerSystem,
};
use serde::{Deserialize, Serialize};

mod dell;
mod error;
mod lenovo;
mod network;
mod nvidia;
pub use network::{Endpoint, RedfishClientPool, RedfishClientPoolBuilder, REDFISH_ENDPOINT};
mod standard;
use crate::model::power::Power;
use crate::model::thermal::Thermal;
pub use error::RedfishError;

/// Interface to a BMC Redfish server. All calls will include one or more HTTP network calls.
pub trait Redfish: Send + Sync + 'static {
    /// Change password for the user
    fn change_password(&self, user: &str, new: &str) -> Result<(), RedfishError>;

    // Get firmware version for particular firmware inventory id
    fn get_firmware(&self, id: &str) -> Result<SoftwareInventory, RedfishError>;

    // Get software inventory collection
    fn get_software_inventories(&self) -> Result<SoftwareInventoryCollection, RedfishError>;

    // Get information about a task
    fn get_task(&self, id: &str) -> Result<Task, RedfishError>;

    /// Is this thing even on?
    fn get_power_state(&self) -> Result<PowerState, RedfishError>;

    /// Returns info about computer system.
    fn get_system(&self) -> Result<ComputerSystem, RedfishError>;

    /// Get Secure Boot state
    fn get_secure_boot(&self) -> Result<SecureBoot, RedfishError>;

    /// Disables Secure Boot
    fn disable_secure_boot(&self) -> Result<(), RedfishError>;

    /// Power supplies and voltages metrics
    fn get_power_metrics(&self) -> Result<Power, RedfishError>;

    /// Change power state: on, off, reboot, etc
    fn power(&self, action: SystemPowerControl) -> Result<(), RedfishError>;

    /// call this to setup bios and bmc
    fn machine_setup(&self) -> Result<(), RedfishError>;

    /// Fans and temperature sensors
    fn get_thermal_metrics(&self) -> Result<Thermal, RedfishError>;

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
}

// When Carbide drops it's `IpmiCommand.launch_command` background job system, we can
// remove the Serialize and Deserialize here.
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub enum Boot {
    Pxe,
    HardDisk,
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
