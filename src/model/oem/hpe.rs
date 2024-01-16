use crate::EnabledDisabled;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::model::{
    Action, ActionsManagerReset, Availableaction, Commandshell, ResourceHealth, ResourceState,
    ResourceStatus, Status,
};
use crate::model::{Firmware, LinkType, ODataId, ODataLinks, StatusVec};

#[derive(Debug, Deserialize, Serialize, Copy, Clone, Eq, PartialEq)]
#[allow(clippy::upper_case_acronyms)]
pub enum BootDevices {
    None,
    Pxe,
    Cd,
    Usb,
    Hdd,
    BiosSetup,
    Utilities,
    Diags,
    UefiShell,
    UefiTarget,
    SDCard,
    UefiHttp,
}

impl fmt::Display for BootDevices {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct BiosAttributes {
    #[serde(rename = "AMDPerformanceWorkloadProfile")]
    pub amd_performance_workload_profile: Option<String>, // amd specific bios settings will not be present on intels
    pub access_control_service: Option<String>,
    pub acpi_hpet: Option<String>,
    pub acpi_root_bridge_pxm: Option<String>,
    pub acpi_slit: Option<String>,
    pub adv_crash_dump_mode: Option<String>,
    pub allow_login_with_ilo: Option<String>,
    pub amd_dma_remapping: Option<String>,
    pub amd_l1_prefetcher: Option<String>,
    pub amd_l2_prefetcher: Option<String>,
    pub amd_mem_p_states: Option<String>,
    pub amd_memory_burst_refresh: Option<String>,
    pub amd_memory_interleaving: Option<String>,
    pub amd_memory_intlv_size: Option<String>,
    pub amd_mmcfg_base3_gb: Option<String>,
    pub amd_periodic_directory_rinse: Option<String>,
    pub amd_secure_memory_encryption: Option<String>,
    pub amd_virtual_drtm_device: Option<String>,
    pub application_power_boost: Option<String>,
    pub asset_tag_protection: Option<String>,
    pub auto_power_on: Option<String>,
    pub boot_mode: Option<String>,
    pub boot_order_policy: Option<String>,
    pub c_state_efficiency_mode: Option<String>,
    pub collab_power_control: Option<String>,
    pub consistent_dev_naming: Option<String>,
    pub data_fabric_c_state_enable: Option<String>,
    pub daylight_savings_time: Option<String>,
    pub determinism_control: Option<String>,
    pub dhcpv4: Option<String>,
    pub dram_controller_power_down: Option<String>,
    pub dynamic_pcie_rate_change: Option<String>,
    pub dynamic_power_capping: Option<String>,
    pub emb_sata1_aspm: Option<String>,
    pub emb_sata1_enable: Option<String>,
    #[serde(rename = "EmbSata1PCIeOptionROM")]
    pub emb_sata1_pcie_option_rom: Option<String>,
    pub emb_video_connection: Option<String>,
    pub embedded_diagnostics: Option<String>,
    pub embedded_ipxe: Option<String>,
    pub embedded_serial_port: Option<String>,
    pub embedded_uefi_shell: Option<String>,
    pub ems_console: Option<String>,
    pub enabled_cores_per_proc: Option<String>,
    #[serde(rename = "EnhancedPreferredIOBusEnable")]
    pub enhanced_preferred_io_bus_enable: Option<String>,
    pub erase_user_defaults: Option<String>,
    pub extended_ambient_temp: Option<String>,
    pub extended_mem_test: Option<String>,
    pub f11_boot_menu: Option<String>,
    #[serde(rename = "FCScanPolicy")]
    pub fc_scan_policy: Option<String>,
    pub fan_fail_policy: Option<String>,
    pub fan_install_req: Option<String>,
    pub hour_format: Option<String>,
    pub http_support: Option<String>,
    pub infinity_fabric_pstate: Option<String>,
    pub intelligent_provisioning: Option<String>,
    pub ipmi_watchdog_timer_action: Option<String>,
    pub ipmi_watchdog_timer_status: Option<String>,
    pub ipmi_watchdog_timer_timeout: Option<String>,
    pub ipv4_address: Option<String>,
    pub ipv4_gateway: Option<String>,
    #[serde(rename = "Ipv4PrimaryDNS")]
    pub ipv4_primary_dns: Option<String>,
    pub ipv4_subnet_mask: Option<String>,
    pub ipv6_address: Option<String>,
    pub ipv6_config_policy: Option<String>,
    pub ipv6_duid: Option<String>,
    pub ipv6_gateway: Option<String>,
    #[serde(rename = "Ipv6PrimaryDNS")]
    pub ipv6_primary_dns: Option<String>,
    pub ipxe_auto_start_script_location: Option<String>,
    pub ipxe_boot_order: Option<String>,
    pub ipxe_script_auto_start: Option<String>,
    pub ipxe_script_verification: Option<String>,
    pub ipxe_startup_url: Option<String>,
    pub kcs_enabled: Option<String>, // only available on newer uefi and bmc firmware
    #[serde(rename = "LastLevelCacheAsNUMANode")]
    pub last_level_cache_as_numa_node: Option<String>,
    #[serde(rename = "MaxMemBusFreqMHz")]
    pub max_mem_bus_freq_mhz: Option<String>,
    pub max_pcie_speed: Option<String>,
    pub maximum_sev_asid: Option<String>,
    pub mem_patrol_scrubbing: Option<String>,
    pub mem_refresh_rate: Option<String>,
    pub microsoft_secured_core_support: Option<String>,
    pub min_proc_idle_power: Option<String>,
    pub minimum_sev_asid: Option<i64>,
    pub mixed_power_supply_reporting: Option<String>,
    pub network_boot_retry: Option<String>,
    pub network_boot_retry_count: Option<i64>,
    pub no_execution_protection: Option<String>,
    pub numa_group_size_opt: Option<String>,
    pub numa_memory_domains_per_socket: Option<String>,
    pub nvme_option_rom: Option<String>,
    pub nvme_raid: Option<String>,
    pub ocp1_auxiliary_power: Option<String>,
    pub omit_boot_device_event: Option<String>,
    pub package_power_limit_control_mode: Option<String>,
    pub package_power_limit_value: Option<i64>,
    pub patrol_scrub_duration: Option<i64>,
    pub pci_resource_padding: Option<String>,
    pub performance_determinism: Option<String>,
    pub platform_certificate: Option<String>,
    #[serde(rename = "PlatformRASPolicy")]
    pub platform_ras_policy: Option<String>,
    pub post_asr: Option<String>,
    pub post_asr_delay: Option<String>,
    pub post_boot_progress: Option<String>,
    pub post_discovery_mode: Option<String>,
    pub post_f1_prompt: Option<String>,
    pub post_screen_mode: Option<String>,
    pub post_video_support: Option<String>,
    pub power_button: Option<String>,
    pub power_on_delay: Option<String>,
    pub power_regulator: Option<String>,
    pub pre_boot_network: Option<String>,
    pub preboot_network_env_policy: Option<String>,
    pub preboot_network_proxy: Option<String>,
    #[serde(rename = "PreferredIOBusEnable")]
    pub preferred_io_bus_enable: Option<String>,
    #[serde(rename = "PreferredIOBusNumber")]
    pub preferred_io_bus_number: Option<i64>,
    #[serde(rename = "ProcAMDBoost")]
    pub proc_amd_boost: Option<String>,
    #[serde(rename = "ProcAMDBoostControl")]
    pub proc_amd_boost_control: Option<String>,
    pub proc_aes: Option<String>,
    pub proc_amd_fmax: Option<i64>,
    pub proc_amd_io_vt: Option<String>,
    #[serde(rename = "ProcSMT")]
    pub proc_smt: Option<String>,
    pub proc_x2_apic: Option<String>,
    pub product_id: Option<String>,
    pub redundant_power_supply: Option<String>,
    pub removable_flash_boot_seq: Option<String>,
    pub restore_defaults: Option<String>,
    pub restore_manufacturing_defaults: Option<String>,
    pub rom_selection: Option<String>,
    pub sata_sanitize: Option<String>,
    pub sata_secure_erase: Option<String>,
    pub save_user_defaults: Option<String>,
    pub sci_ras_support: Option<String>,
    pub sec_start_backup_image: Option<String>,
    pub secure_boot_status: Option<String>,
    pub serial_console_baud_rate: Option<String>,
    pub serial_console_emulation: Option<String>,
    pub serial_console_port: Option<String>,
    pub serial_number: Option<String>,
    pub server_asset_tag: Option<String>,
    pub server_config_lock_status: Option<String>,
    pub server_name: Option<String>,
    pub setup_browser_selection: Option<String>,
    pub speculative_lock_scheduling: Option<String>,
    pub sriov: Option<String>,
    pub thermal_config: Option<String>,
    pub thermal_shutdown: Option<String>,
    pub time_format: Option<String>,
    pub time_zone: Option<String>,
    #[serde(rename = "TPM2EndorsementDisable")]
    pub tpm2_endorsement_disable: Option<String>,
    #[serde(rename = "TPM2StorageDisable")]
    pub tpm2_storage_disable: Option<String>,
    pub tpm20_software_interface_operation: Option<String>,
    pub tpm20_software_interface_status: Option<String>,
    pub tpm2_operation: Option<String>,
    pub tpm_active_pcrs: Option<String>,
    pub tpm_chip_id: Option<String>,
    pub tpm_fips: Option<String>,
    pub tpm_fips_mode_switch: Option<String>,
    pub tpm_mode_switch_operation: Option<String>,
    pub tpm_state: Option<String>,
    pub tpm_type: Option<String>,
    pub tpm_uefi_oprom_measuring: Option<String>,
    pub tpm_visibility: Option<String>,
    pub transparent_secure_memory_encryption: Option<String>,
    pub uefi_optimized_boot: Option<String>,
    pub uefi_serial_debug_level: Option<String>,
    pub uefi_shell_boot_order: Option<String>,
    pub uefi_shell_physical_presence_keystroke: Option<String>,
    pub uefi_shell_script_verification: Option<String>,
    pub uefi_shell_startup: Option<String>,
    pub uefi_shell_startup_location: Option<String>,
    pub uefi_shell_startup_url: Option<String>,
    pub uefi_shell_startup_url_from_dhcp: Option<String>,
    pub uefi_variable_access_fw_control: Option<String>,
    pub usb_boot: Option<String>,
    pub usb_control: Option<String>,
    pub user_defaults_state: Option<String>,
    pub utility_lang: Option<String>,
    pub virtual_serial_port: Option<String>,
    pub vlan_control: Option<String>,
    pub vlan_id: Option<i64>,
    pub vlan_priority: Option<i64>,
    pub wake_on_lan: Option<String>,
    pub workload_profile: Option<String>,
    #[serde(rename = "XGMIForceLinkWidth")]
    pub xgmi_force_link_width: Option<String>,
    #[serde(rename = "XGMIMaxLinkWidth")]
    pub xgmi_max_link_width: Option<String>,
    #[serde(rename = "iSCSISoftwareInitiator")]
    pub iscsi_software_initiator: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Bios {
    #[serde(rename = "@odata.context")]
    pub odata_context: String,
    pub attributes: BiosAttributes,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct BiosLockdownAttributes {
    //    pub kcs_enabled: Option<String>,
    pub usb_boot: EnabledDisabled,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SetBiosLockdownAttributes {
    pub attributes: BiosLockdownAttributes,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct BiosSerialConsoleAttributes {
    pub embedded_serial_port: String,
    pub ems_console: String,
    pub serial_console_baud_rate: String,
    pub serial_console_emulation: String,
    pub serial_console_port: String,
    pub uefi_serial_debug_level: String,
    pub virtual_serial_port: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SetBiosSerialConsoleAttributes {
    pub attributes: BiosSerialConsoleAttributes,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct TpmAttributes {
    pub tpm2_operation: String,
    pub tpm_visibility: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SetTpmAttributes {
    pub attributes: TpmAttributes,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct VirtAttributes {
    pub proc_amd_io_vt: EnabledDisabled,
    pub sriov: EnabledDisabled,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SetVirtAttributes {
    pub attributes: VirtAttributes,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct UefiHttpAttributes {
    pub dhcpv4: EnabledDisabled,
    pub http_support: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SetUefiHttpAttributes {
    pub attributes: UefiHttpAttributes,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Manager {
    #[serde(flatten)]
    pub odata: ODataLinks,
    pub actions: Action,
    pub available_actions: Vec<Availableaction>,
    pub command_shell: Commandshell,
    pub description: String,
    pub ethernet_interfaces: ODataId,
    pub firmware: Firmware,
    pub firmware_version: String,
    pub graphical_console: Commandshell,
    pub id: String,
    pub log_services: ODataId,
    pub manager_type: String,
    pub name: String,
    pub network_protocol: ODataId,
    pub oem: OemHpWrapper,
    pub serial_console: Commandshell,
    pub status: Status,
    #[serde(rename = "Type")]
    pub root_type: String,
    #[serde(rename = "UUID")]
    pub uuid: String,
    pub virtual_media: ODataId,
}

impl StatusVec for Manager {
    fn get_vec(&self) -> Vec<ResourceStatus> {
        let mut v: Vec<ResourceStatus> = Vec::new();
        for res in &self.oem.hp.i_lo_self_test_results {
            v.push(res.get_resource_status());
        }
        v
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OemHpActionshpiloResetToFactoryDefault {
    #[serde(rename = "ResetType@Redfish.AllowableValues")]
    pub reset_type_redfish_allowable_values: Vec<String>,
    pub target: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OemHpAction {
    #[serde(rename = "#HpiLO.ClearRestApiState")]
    pub hpi_lo_clear_rest_api_state: ActionsManagerReset,
    #[serde(rename = "#HpiLO.ResetToFactoryDefaults")]
    pub hpi_lo_reset_to_factory_defaults: OemHpActionshpiloResetToFactoryDefault,
    #[serde(rename = "#HpiLO.iLOFunctionality")]
    pub hpi_lo_i_lo_functionality: ActionsManagerReset,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct OemHpAvailableactionsCapability {
    pub allowable_values: Vec<String>,
    pub property_name: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct OemHpAvailableaction {
    pub action: String,
    pub capabilities: Vec<OemHpAvailableactionsCapability>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct OemHpFederationconfig {
    #[serde(rename = "IPv6MulticastScope")]
    pub i_pv6_multicast_scope: String,
    pub multicast_announcement_interval: i64,
    pub multicast_discovery: String,
    pub multicast_time_to_live: i64,
    #[serde(rename = "iLOFederationManagement")]
    pub i_lo_federation_management: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct OemHpFirmwareCurrent {
    pub date: String,
    pub debug_build: bool,
    pub major_version: i64,
    pub minor_version: i64,
    pub time: String,
    pub version_string: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct OemHpFirmware {
    pub current: OemHpFirmwareCurrent,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct OemHpLicense {
    pub license_key: String,
    pub license_string: String,
    pub license_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct OemHpIloselftestresult {
    pub notes: String,
    pub self_test_name: String,
    pub status: ResourceHealth,
}
impl OemHpIloselftestresult {
    fn get_resource_status(&self) -> ResourceStatus {
        ResourceStatus {
            health: Some(self.status),
            state: Some(ResourceState::Enabled),
            health_rollup: Some(self.status),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct OemHp {
    #[serde(flatten)]
    pub oem_type: HpType,
    pub actions: OemHpAction,
    pub available_actions: Vec<OemHpAvailableaction>,
    pub clear_rest_api_status: String,
    pub federation_config: OemHpFederationconfig,
    pub firmware: OemHpFirmware,
    pub license: OemHpLicense,
    #[serde(rename = "RequiredLoginForiLORBSU")]
    pub required_login_fori_lorbsu: bool,
    #[serde(rename = "SerialCLISpeed")]
    pub serial_cli_speed: i64,
    #[serde(rename = "SerialCLIStatus")]
    pub serial_cli_status: String,
    #[serde(rename = "VSPLogDownloadEnabled")]
    pub vsp_log_download_enabled: bool,
    #[serde(rename = "iLOSelfTestResults")]
    pub i_lo_self_test_results: Vec<OemHpIloselftestresult>,
    #[serde(rename = "links", flatten)]
    pub links: LinkType,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct OemHpWrapper {
    pub hp: OemHp,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct HpType {
    #[serde(rename = "@odata.type")]
    pub odata_type: String,
    #[serde(rename = "Type")]
    pub hp_type: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct OemHpeLockdownAttrs {
    #[serde(rename = "VirtualNICEnabled")]
    pub virtual_nic_enabled: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct OemHpeLockdown {
    pub hpe: OemHpeLockdownAttrs,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SetOemHpeLockdown {
    pub oem: OemHpeLockdown,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct OemHpeBootSource {
    pub boot_option_number: String,
    pub boot_string: String,
    pub structured_boot_string: String,
    #[serde(rename = "UEFIDevicePath")]
    pub uefi_device_path: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct OemHpeBoot {
    #[serde(flatten)]
    pub odata: ODataLinks,
    pub id: String,
    pub name: String,
    pub boot_sources: Vec<OemHpeBootSource>,
    pub default_boot_order: Vec<String>,
    pub persistent_boot_config_order: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct SetOemHpeBoot {
    pub persistent_boot_config_order: Vec<String>,
}
