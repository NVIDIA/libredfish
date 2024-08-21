/*
 * SPDX-FileCopyrightText: Copyright (c) 2023 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
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
use std::{
    collections::{HashMap, HashSet},
    default,
    path::Path,
    time::Duration,
};

use reqwest::{Method, StatusCode};
use tracing::debug;

use crate::model::manager_network_protocol::ManagerNetworkProtocol;
use crate::model::sel::LogEntry;
use crate::model::serial_interface::SerialInterface;
use crate::model::service_root::ServiceRoot;
use crate::model::software_inventory::SoftwareInventory;
use crate::model::task::Task;
use crate::model::thermal::Thermal;
use crate::model::{account_service::ManagerAccount, service_root::RedfishVendor};
use crate::model::{power, thermal, BootOption, InvalidValueError, Manager, Managers, ODataId};
use crate::model::{power::Power, update_service::UpdateService};
use crate::model::{secure_boot::SecureBoot, sensor::GPUSensors};
use crate::network::{RedfishHttpClient, REDFISH_ENDPOINT};
use crate::{
    model, Boot, EnabledDisabled, JobState, NetworkDeviceFunction, NetworkPort, PowerState,
    Redfish, RoleId, Status, Systems,
};
use crate::{
    model::chassis::{Chassis, NetworkAdapter},
    MachineSetupStatus,
};
use crate::{BootOptions, Collection, PCIeDevice, RedfishError, Resource};

const UEFI_PASSWORD_NAME: &str = "AdministratorPassword";

/// The calls that use the Redfish standard without any OEM extensions.
#[derive(Clone)]
pub struct RedfishStandard {
    pub client: RedfishHttpClient,
    pub vendor: Option<RedfishVendor>,
    manager_id: String,
    system_id: String,
    service_root: ServiceRoot,
}

#[async_trait::async_trait]
impl Redfish for RedfishStandard {
    async fn create_user(
        &self,
        username: &str,
        password: &str,
        role_id: RoleId,
    ) -> Result<(), RedfishError> {
        let mut data = HashMap::new();
        data.insert("UserName", username.to_string());
        data.insert("Password", password.to_string());
        data.insert("RoleId", role_id.to_string());
        self.client
            .post("AccountService/Accounts", data)
            .await
            .map(|_resp| Ok(()))?
    }

    async fn change_username(&self, old_name: &str, new_name: &str) -> Result<(), RedfishError> {
        let account = self.get_account_by_name(old_name).await?;
        let Some(account_id) = account.id else {
            return Err(RedfishError::UserNotFound(format!(
                "{old_name} has no ID field"
            )));
        };
        let url = format!("AccountService/Accounts/{account_id}");
        let mut data = HashMap::new();
        data.insert("UserName", new_name);
        self.client
            .patch(&url, &data)
            .await
            .map(|_status_code| Ok(()))?
    }

    async fn change_password(&self, user: &str, new_pass: &str) -> Result<(), RedfishError> {
        let account = self.get_account_by_name(user).await?;
        let Some(account_id) = account.id else {
            return Err(RedfishError::UserNotFound(format!(
                "{user} has no ID field"
            )));
        };
        self.change_password_by_id(&account_id, new_pass).await
    }

    async fn change_password_by_id(
        &self,
        account_id: &str,
        new_pass: &str,
    ) -> Result<(), RedfishError> {
        let url = format!("AccountService/Accounts/{}", account_id);
        let mut data = HashMap::new();
        data.insert("Password", new_pass);
        self.client
            .patch(&url, &data)
            .await
            .map(|_status_code| Ok(()))?
    }

    async fn get_accounts(&self) -> Result<Vec<ManagerAccount>, RedfishError> {
        let account_ids = self.get_members("AccountService/Accounts").await?;
        let mut accounts = Vec::with_capacity(account_ids.len());
        for id in account_ids {
            let account = self.get_account_by_id(&id).await?;
            accounts.push(account);
        }
        accounts.sort();
        Ok(accounts)
    }

    async fn get_power_state(&self) -> Result<PowerState, RedfishError> {
        let system = self.get_system().await?;
        Ok(system.power_state)
    }

    async fn get_power_metrics(&self) -> Result<Power, RedfishError> {
        let power = self.get_power_metrics().await?;
        Ok(power)
    }

    async fn power(&self, action: model::SystemPowerControl) -> Result<(), RedfishError> {
        let url = format!("Systems/{}/Actions/ComputerSystem.Reset", self.system_id);
        let mut arg = HashMap::new();
        arg.insert("ResetType", action.to_string());
        // Lenovo: The expected HTTP response code is 204 No Content
        self.client.post(&url, arg).await.map(|_resp| Ok(()))?
    }

    async fn bmc_reset(&self) -> Result<(), RedfishError> {
        let url = format!("Managers/{}/Actions/Manager.Reset", self.manager_id);
        let mut arg = HashMap::new();
        // Dell only has GracefulRestart. The spec, and Lenovo, also have ForceRestart.
        // Response code 204 No Content is fine.
        arg.insert("ResetType", "GracefulRestart".to_string());
        self.client.post(&url, arg).await.map(|_resp| Ok(()))?
    }

    async fn chassis_reset(
        &self,
        chassis_id: &str,
        reset_type: model::SystemPowerControl,
    ) -> Result<(), RedfishError> {
        let url = format!("Chassis/{}/Actions/Chassis.Reset", chassis_id);
        let mut arg = HashMap::new();

        arg.insert("ResetType", reset_type.to_string());
        self.client.post(&url, arg).await.map(|_resp| Ok(()))?
    }

    async fn get_thermal_metrics(&self) -> Result<Thermal, RedfishError> {
        let thermal = self.get_thermal_metrics().await?;
        Ok(thermal)
    }

    async fn get_gpu_sensors(&self) -> Result<Vec<GPUSensors>, RedfishError> {
        Err(RedfishError::NotSupported(
            "No GPUs on this machine".to_string(),
        ))
    }

    async fn get_system_event_log(&self) -> Result<Vec<LogEntry>, RedfishError> {
        Err(RedfishError::NotSupported("SEL".to_string()))
    }

    async fn bios(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        let url = format!("Systems/{}/Bios", self.system_id());
        let (_status_code, body) = self.client.get(&url).await?;
        Ok(body)
    }

    async fn pending(&self) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        let url = format!("Systems/{}/Bios/Settings", self.system_id());
        self.pending_with_url(&url).await
    }

    async fn clear_pending(&self) -> Result<(), RedfishError> {
        let url = format!("Systems/{}/Bios/Settings", self.system_id());
        self.clear_pending_with_url(&url).await
    }

    async fn machine_setup(&self) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported("machine_setup".to_string()))
    }

    async fn machine_setup_status(&self) -> Result<MachineSetupStatus, RedfishError> {
        Err(RedfishError::NotSupported("machine_setup_status".to_string()))
    }

    async fn set_machine_password_policy(&self) -> Result<(), RedfishError> {
        use serde_json::Value::Number;
        let body = HashMap::from([
            ("AccountLockoutThreshold", Number(0.into())),
            ("AccountLockoutDuration", Number(0.into())),
            ("AccountLockoutCounterResetAfter", Number(0.into())),
        ]);
        self.client
            .patch("AccountService", body)
            .await
            .map(|_status_code| ())
    }

    async fn lockdown(&self, _target: EnabledDisabled) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported("lockdown".to_string()))
    }

    async fn lockdown_status(&self) -> Result<Status, RedfishError> {
        Err(RedfishError::NotSupported("lockdown_status".to_string()))
    }

    async fn setup_serial_console(&self) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported(
            "setup_serial_console".to_string(),
        ))
    }

    async fn serial_console_status(&self) -> Result<Status, RedfishError> {
        Err(RedfishError::NotSupported(
            "setup_serial_console".to_string(),
        ))
    }

    async fn get_boot_options(&self) -> Result<BootOptions, RedfishError> {
        self.get_boot_options().await
    }

    async fn get_boot_option(&self, option_id: &str) -> Result<BootOption, RedfishError> {
        let url = format!("Systems/{}/BootOptions/{}", self.system_id(), option_id);
        let (_status_code, body) = self.client.get(&url).await?;
        Ok(body)
    }

    async fn boot_once(&self, _target: Boot) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported("boot_once".to_string()))
    }

    async fn boot_first(&self, _target: Boot) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported("boot_first".to_string()))
    }

    async fn clear_tpm(&self) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported("clear_tpm".to_string()))
    }

    async fn pcie_devices(&self) -> Result<Vec<PCIeDevice>, RedfishError> {
        let mut out = Vec::new();
        let mut seen = HashSet::new(); // Dell redfish response has duplicates
        let system = self.get_system().await?;
        debug!("Listing {} PCIe devices..", system.pcie_devices.len());
        for member in system.pcie_devices {
            let url = member
                .odata_id
                .replace(&format!("/{REDFISH_ENDPOINT}/"), "");
            if seen.contains(&url) {
                continue;
            }
            let p: PCIeDevice = self.client.get(&url).await?.1;
            seen.insert(url);
            if p.id.is_none() || p.manufacturer.is_none() {
                // Lenovo has lots of all-null devices with name "Adapater". Ignore those.
                continue;
            }
            out.push(p);
        }
        out.sort_unstable_by(|a, b| a.manufacturer.partial_cmp(&b.manufacturer).unwrap());
        Ok(out)
    }

    async fn get_firmware(&self, id: &str) -> Result<SoftwareInventory, RedfishError> {
        let url = format!("UpdateService/FirmwareInventory/{}", id);
        let (_status_code, body) = self.client.get(&url).await?;
        Ok(body)
    }

    async fn update_firmware(&self, firmware: tokio::fs::File) -> Result<Task, RedfishError> {
        let (_status_code, body) = self.client.post_file("UpdateService", firmware).await?;
        Ok(body)
    }

    async fn update_firmware_multipart(
        &self,
        _filename: &Path,
        _reboot: bool,
        _timeout: Duration,
    ) -> Result<String, RedfishError> {
        Err(RedfishError::NotSupported(
            "Multipart firmware updates not currently supported on this platform".to_string(),
        ))
    }

    async fn get_tasks(&self) -> Result<Vec<String>, RedfishError> {
        self.get_members("TaskService/Tasks/").await
    }

    /// http://redfish.dmtf.org/schemas/v1/TaskCollection.json
    async fn get_task(&self, id: &str) -> Result<Task, RedfishError> {
        let url = format!("TaskService/Tasks/{}", id);
        let (_status_code, body) = self.client.get(&url).await?;
        Ok(body)
    }
    
    /// Vec of chassis id
    /// http://redfish.dmtf.org/schemas/v1/ChassisCollection.json
    async fn get_chassis_all(&self) -> Result<Vec<String>, RedfishError> {
        self.get_members("Chassis/").await
    }

    async fn get_chassis(&self, id: &str) -> Result<Chassis, RedfishError> {
        let url = format!("Chassis/{}", id);
        let (_status_code, body) = self.client.get(&url).await?;
        Ok(body)
    }

    async fn get_chassis_network_adapters(
        &self,
        chassis_id: &str,
    ) -> Result<Vec<String>, RedfishError> {
        let url = format!("Chassis/{}/NetworkAdapters", chassis_id);
        self.get_members(&url).await
    }

    async fn get_base_network_adapters(
        &self,
        _system_id: &str,
    ) -> Result<Vec<String>, RedfishError> {
        // this is only implemented in iLO5, and will be removed in iLO6.
        Err(RedfishError::NotSupported(
            "BaseNetworkAdapter is only supported in iLO5".to_string(),
        ))
    }

    async fn get_base_network_adapter(
        &self,
        _system_id: &str,
        _id: &str,
    ) -> Result<NetworkAdapter, RedfishError> {
        // this is only implemented in iLO5, and will be removed in iLO6.
        Err(RedfishError::NotSupported(
            "BaseNetworkAdapter is only supported in iLO5".to_string(),
        ))
    }

    async fn get_chassis_network_adapter(
        &self,
        chassis_id: &str,
        id: &str,
    ) -> Result<NetworkAdapter, RedfishError> {
        let url = format!("Chassis/{}/NetworkAdapters/{}", chassis_id, id);
        let (_, body) = self.client.get(&url).await?;
        Ok(body)
    }

    /// http://redfish.dmtf.org/schemas/v1/EthernetInterfaceCollection.json
    async fn get_manager_ethernet_interfaces(&self) -> Result<Vec<String>, RedfishError> {
        let url = format!("Managers/{}/EthernetInterfaces", self.manager_id);
        self.get_members(&url).await
    }

    async fn get_manager_ethernet_interface(
        &self,
        id: &str,
    ) -> Result<crate::EthernetInterface, RedfishError> {
        let url = format!("Managers/{}/EthernetInterfaces/{}", self.manager_id(), id);
        let (_status_code, body) = self.client.get(&url).await?;
        Ok(body)
    }

    async fn get_system_ethernet_interfaces(&self) -> Result<Vec<String>, RedfishError> {
        let url = format!("Systems/{}/EthernetInterfaces", self.system_id);
        self.get_members(&url).await
    }

    async fn get_system_ethernet_interface(
        &self,
        id: &str,
    ) -> Result<crate::EthernetInterface, RedfishError> {
        let url = format!("Systems/{}/EthernetInterfaces/{}", self.system_id(), id);
        let (_status_code, body) = self.client.get(&url).await?;
        Ok(body)
    }

    /// http://redfish.dmtf.org/schemas/v1/SoftwareInventoryCollection.json#/definitions/SoftwareInventoryCollection
    async fn get_software_inventories(&self) -> Result<Vec<String>, RedfishError> {
        self.get_members("UpdateService/FirmwareInventory").await
    }

    async fn get_system(&self) -> Result<model::ComputerSystem, RedfishError> {
        let url = format!("Systems/{}/", self.system_id);
        let host: model::ComputerSystem = self.client.get(&url).await?.1;
        Ok(host)
    }

    async fn get_secure_boot(&self) -> Result<SecureBoot, RedfishError> {
        let url = format!("Systems/{}/SecureBoot", self.system_id());
        let (_status_code, body) = self.client.get(&url).await?;
        Ok(body)
    }

    async fn enable_secure_boot(&self) -> Result<(), RedfishError> {
        let mut data = HashMap::new();
        data.insert("SecureBootEnable", true);
        let url = format!("Systems/{}/SecureBoot", self.system_id());
        let _status_code = self.client.patch(&url, data).await?;
        Ok(())
    }

    async fn add_secure_boot_certificate(&self, pem_cert: &str) -> Result<Task, RedfishError> {
        let mut data = HashMap::new();
        data.insert("CertificateString", pem_cert);
        data.insert("CertificateType", "PEM");
        let url = format!(
            "Systems/{}/SecureBoot/SecureBootDatabases/db/Certificates",
            self.system_id()
        );
        let (_status_code, resp_opt, _resp_headers) = self
            .client
            .req::<Task, _>(Method::POST, &url, Some(data), None, None, Vec::new())
            .await?;
        match resp_opt {
            Some(response_body) => Ok(response_body),
            None => Err(RedfishError::NoContent),
        }
    }

    async fn disable_secure_boot(&self) -> Result<(), RedfishError> {
        let mut data = HashMap::new();
        data.insert("SecureBootEnable", false);
        let url = format!("Systems/{}/SecureBoot", self.system_id());
        let _status_code = self.client.patch(&url, data).await?;
        Ok(())
    }

    async fn get_network_device_functions(
        &self,
        _chassis_id: &str,
    ) -> Result<Vec<String>, RedfishError> {
        Err(RedfishError::NotSupported(
            "get_network_device_functions".to_string(),
        ))
    }

    async fn get_network_device_function(
        &self,
        _chassis_id: &str,
        _id: &str,
        _port: Option<&str>,
    ) -> Result<NetworkDeviceFunction, RedfishError> {
        Err(RedfishError::NotSupported(
            "get_network_device_function".to_string(),
        ))
    }

    async fn get_ports(&self, _chassis_id: &str) -> Result<Vec<String>, RedfishError> {
        Err(RedfishError::NotSupported("get_ports".to_string()))
    }

    async fn get_port(&self, _chassis_id: &str, _id: &str) -> Result<NetworkPort, RedfishError> {
        Err(RedfishError::NotSupported("get_port".to_string()))
    }

    async fn change_uefi_password(
        &self,
        current_uefi_password: &str,
        new_uefi_password: &str,
    ) -> Result<Option<String>, RedfishError> {
        self.change_bios_password(UEFI_PASSWORD_NAME, current_uefi_password, new_uefi_password)
            .await
    }

    async fn change_boot_order(&self, _boot_array: Vec<String>) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported("change_boot_order".to_string()))
    }

    async fn get_service_root(&self) -> Result<ServiceRoot, RedfishError> {
        let (_status_code, body) = self.client.get("").await?;
        Ok(body)
    }

    async fn get_systems(&self) -> Result<Vec<String>, RedfishError> {
        let (_, systems): (_, Systems) = self.client.get("Systems/").await?;
        if systems.members.is_empty() {
            return Ok(vec!["1".to_string()]); // default to DMTF standard suggested
        }
        let v: Vec<String> = systems
            .members
            .into_iter()
            .map(|d| {
                d.odata_id
                    .trim_matches('/')
                    .split('/')
                    .last()
                    .unwrap()
                    .to_string()
            })
            .collect();

        Ok(v)
    }

    async fn get_manager(&self) -> Result<Manager, RedfishError> {
        let (_, manager): (_, Manager) = self
            .client
            .get(&format!("Managers/{}", self.manager_id()))
            .await?;
        Ok(manager)
    }

    async fn get_managers(&self) -> Result<Vec<String>, RedfishError> {
        let (_, bmcs): (_, Managers) = self.client.get("Managers/").await?;
        if bmcs.members.is_empty() {
            return Ok(vec!["1".to_string()]);
        }
        let v: Vec<String> = bmcs
            .members
            .into_iter()
            .map(|d| {
                d.odata_id
                    .trim_matches('/')
                    .split('/')
                    .last()
                    .unwrap()
                    .to_string()
            })
            .collect();
        Ok(v)
    }

    async fn bmc_reset_to_defaults(&self) -> Result<(), RedfishError> {
        let url = format!(
            "Managers/{}/Actions/Manager.ResetToDefaults",
            self.manager_id
        );
        let mut arg = HashMap::new();
        arg.insert("ResetToDefaultsType", "ResetAll".to_string());
        self.client.post(&url, arg).await.map(|_resp| Ok(()))?
    }

    async fn get_job_state(&self, _job_id: &str) -> Result<JobState, RedfishError> {
        Err(RedfishError::NotSupported("get_job_state".to_string()))
    }

    async fn get_resource(&self, id: ODataId) -> Result<Resource, RedfishError> {
        let url = id.odata_id.replace(&format!("/{REDFISH_ENDPOINT}/"), "");
        let (_, mut resource): (StatusCode, Resource) = self.client.get(url.as_str()).await?;

        resource.url = url;
        Ok(resource)
    }

    // This function appends ?$expand=.($levels=1) to the URL, as defined by Redfish spec, to expand first level URIs.
    async fn get_collection(&self, id: ODataId) -> Result<Collection, RedfishError> {
        let url = format!(
            "{}?$expand=.($levels=1)",
            id.odata_id.replace(&format!("/{REDFISH_ENDPOINT}/"), "")
        );
        let (_, body): (_, HashMap<String, serde_json::Value>) =
            self.client.get(url.as_str()).await?;
        Ok(Collection {
            url: url.clone(),
            body,
        })
    }

    async fn set_boot_order_dpu_first(&self, _address: Option<String>) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported(
            "set_boot_order_dpu_first".to_string(),
        ))
    }

    async fn clear_uefi_password(
        &self,
        current_uefi_password: &str,
    ) -> Result<Option<String>, RedfishError> {
        self.change_uefi_password(current_uefi_password, "").await
    }

    async fn get_update_service(&self) -> Result<UpdateService, RedfishError> {
        let (_, update_service) = self.client.get(self.update_service().as_str()).await?;
        Ok(update_service)
    }

    async fn get_base_mac_address(&self) -> Result<Option<String>, RedfishError> {
        Err(RedfishError::NotSupported(
            "get_base_mac_address".to_string(),
        ))
    }

    async fn lockdown_bmc(&self, _target: EnabledDisabled) -> Result<(), RedfishError> {
        Err(RedfishError::NotSupported("lockdown_bmc".to_string()))
    }

    async fn is_ipmi_over_lan_enabled(&self) -> Result<bool, RedfishError> {
        let network_protocol = self.get_manager_network_protocol().await?;
        match network_protocol.ipmi {
            Some(ipmi_status) => match ipmi_status.protocol_enabled {
                Some(is_ipmi_enabled) => Ok(is_ipmi_enabled),
                None => Err(RedfishError::GenericError {
                    error: format!(
                        "protocol_enabled is None in the server's ipmi status: {ipmi_status:#?}"
                    ),
                }),
            },
            None => Err(RedfishError::GenericError {
                error: format!(
                    "ipmi is None in the server's network service settings: {network_protocol:#?}"
                ),
            }),
        }
    }

    async fn enable_ipmi_over_lan(&self, target: EnabledDisabled) -> Result<(), RedfishError> {
        let url = format!("Managers/{}/NetworkProtocol", self.manager_id(),);
        let mut ipmi_data = HashMap::new();
        ipmi_data.insert("ProtocolEnabled", target.is_enabled());

        let mut data = HashMap::new();
        data.insert("IPMI", ipmi_data);

        self.client.patch(&url, data).await.map(|_status_code| ())
    }
}

impl RedfishStandard {
    //
    // PUBLIC
    //

    pub async fn get_members(&self, url: &str) -> Result<Vec<String>, RedfishError> {
        let (_, mut body): (_, HashMap<String, serde_json::Value>) = self.client.get(url).await?;
        let key = "Members";
        let members_json = body.remove(key).ok_or_else(|| RedfishError::MissingKey {
            key: key.to_string(),
            url: url.to_string(),
        })?;
        let Ok(members) = serde_json::from_value::<Vec<ODataId>>(members_json) else {
            return Err(RedfishError::InvalidKeyType {
                key: key.to_string(),
                expected_type: "Vec<ODataId>".to_string(),
                url: url.to_string(),
            });
        };
        let member_ids: Vec<String> = members
            .into_iter()
            .map(|d| d.odata_id.split('/').last().unwrap().to_string())
            .collect();
        Ok(member_ids)
    }

    /// Fetch root URL and record the vendor, if any
    pub fn set_vendor(
        &mut self,
        vendor: RedfishVendor,
    ) -> Result<Box<dyn crate::Redfish>, RedfishError> {
        self.vendor = Some(vendor);
        debug!("BMC Vendor: {vendor}");
        match vendor {
            // nvidia dgx systems may have both ami and nvidia as vendor strings depending on hw
            // ami also ships its bmc fw for other system vendors.
            RedfishVendor::AMI => {
                if self.system_id == "DGX" && self.manager_id == "BMC" {
                    Ok(Box::new(crate::nvidia_viking::Bmc::new(self.clone())?))
                } else {
                    Err(RedfishError::NotSupported(format!(
                        "vendor: AMI, system: {}, bmc: {}",
                        self.system_id, self.manager_id
                    )))
                }
            }
            RedfishVendor::Dell => Ok(Box::new(crate::dell::Bmc::new(self.clone())?)),
            RedfishVendor::Hpe => Ok(Box::new(crate::hpe::Bmc::new(self.clone())?)),
            RedfishVendor::Lenovo => Ok(Box::new(crate::lenovo::Bmc::new(self.clone())?)),
            RedfishVendor::Nvidia => Ok(Box::new(crate::nvidia_dpu::Bmc::new(self.clone())?)),
            RedfishVendor::Supermicro => Ok(Box::new(crate::supermicro::Bmc::new(self.clone())?)),
            _ => Ok(Box::new(self.clone())),
        }
    }

    /// Fetch and set System number. Needed for all `Systems/{system_id}/...` calls
    pub fn set_system_id(&mut self, system_id: &str) -> Result<(), RedfishError> {
        self.system_id = system_id.to_string();
        Ok(())
    }

    /// Fetch and set Manager number. Needed for all `Managers/{system_id}/...` calls
    pub fn set_manager_id(&mut self, manager_id: &str) -> Result<(), RedfishError> {
        self.manager_id = manager_id.to_string();
        Ok(())
    }

    /// Saves the service_root for later use
    pub fn set_service_root(&mut self, service_root: ServiceRoot) -> Result<(), RedfishError> {
        self.service_root = service_root;
        Ok(())
    }

    /// Create client object
    pub fn new(client: RedfishHttpClient) -> Self {
        Self {
            client,
            manager_id: "".to_string(),
            system_id: "".to_string(),
            vendor: None,
            service_root: default::Default::default(),
        }
    }

    pub fn system_id(&self) -> &str {
        &self.system_id
    }

    pub fn manager_id(&self) -> &str {
        &self.manager_id
    }

    /// Gets the location of the update service from the saved service root
    pub fn update_service(&self) -> String {
        self.service_root
            .update_service
            .clone()
            .unwrap_or_default()
            .get("@odata.id")
            .unwrap_or(&serde_json::Value::String(
                "/redfish/v1/UpdateService".to_string(), // Sane default
            ))
            .as_str()
            .unwrap_or_default()
            .replace("/redfish/v1/", "") // Remove starting /redfish/v1 as we add it elsewhere
            .to_string()
    }

    pub async fn get_boot_options(&self) -> Result<model::BootOptions, RedfishError> {
        let url = format!("Systems/{}/BootOptions", self.system_id());
        let (_status_code, body) = self.client.get(&url).await?;
        Ok(body)
    }

    pub async fn get_first_boot_option(&self) -> Result<BootOption, RedfishError> {
        let boot_options = self.get_boot_options().await?;
        let Some(member) = boot_options.members.first() else {
            return Err(RedfishError::NoContent);
        };
        let url = member
            .odata_id
            .replace(&format!("/{REDFISH_ENDPOINT}/"), "");
        let b: BootOption = self.client.get(&url).await?.1;
        Ok(b)
    }

    // The URL differs for Lenovo, but the rest is the same
    pub async fn pending_with_url(
        &self,
        pending_url: &str,
    ) -> Result<HashMap<String, serde_json::Value>, RedfishError> {
        let pending_attrs = self.pending_attributes(pending_url).await?;
        let current_attrs = self.bios_attributes().await?;
        Ok(attr_diff(&pending_attrs, &current_attrs))
    }

    // There's no standard Redfish way to clear pending BIOS settings, so we find the
    // pending changes and set them back to their existing values
    pub async fn clear_pending_with_url(&self, pending_url: &str) -> Result<(), RedfishError> {
        let pending_attrs = self.pending_attributes(pending_url).await?;
        let current_attrs = self.bios_attributes().await?;
        let diff = attr_diff(&pending_attrs, &current_attrs);

        let mut reset_attrs = HashMap::new();
        for k in diff.keys() {
            reset_attrs.insert(k, current_attrs.get(k));
        }
        let mut body = HashMap::new();
        body.insert("Attributes", reset_attrs);
        self.client
            .patch(pending_url, body)
            .await
            .map(|_status_code| ())
    }

    /// Get the first serial interface
    /// On Dell it has no useful content. On Lenovo and Supermicro it does,
    /// and on Supermicro it's part of setting up Serial-Over-LAN.
    pub async fn get_serial_interface(&self) -> Result<SerialInterface, RedfishError> {
        let interface_id = self.get_serial_interface_name().await?;
        let url = format!(
            "Managers/{}/SerialInterfaces/{}",
            self.manager_id(),
            interface_id
        );
        let (_status_code, body) = self.client.get(&url).await?;
        Ok(body)
    }

    /// The name of the first serial interface.
    /// I have not seen a box with any number except exactly one yet.
    pub async fn get_serial_interface_name(&self) -> Result<String, RedfishError> {
        let url = format!("Managers/{}/SerialInterfaces", self.manager_id());
        let mut members = self.get_members(&url).await?;
        let Some(member) = members.pop() else {
            return Err(RedfishError::InvalidValue {
                url: url.to_string(),
                field: "0".to_string(),
                err: InvalidValueError("Members array is empty, no SerialInterfaces".to_string()),
            });
        };
        Ok(member)
    }

    // BIOS attributes that will be applied on next restart
    pub async fn pending_attributes(
        &self,
        pending_url: &str,
    ) -> Result<serde_json::Map<String, serde_json::Value>, RedfishError> {
        let (_sc, mut body): (reqwest::StatusCode, HashMap<String, serde_json::Value>) =
            self.client.get(pending_url).await?;
        let mut attrs = body
            .remove("Attributes")
            .ok_or_else(|| RedfishError::MissingKey {
                key: "Attributes".to_string(),
                url: pending_url.to_string(),
            })?;
        let attrs_map = match attrs.as_object_mut() {
            Some(m) => m,
            None => {
                return Err(RedfishError::InvalidKeyType {
                    key: "Attributes".to_string(),
                    expected_type: "Map".to_string(),
                    url: pending_url.to_string(),
                })
            }
        };
        Ok(core::mem::take(attrs_map))
    }

    // Current BIOS attributes
    pub async fn bios_attributes(&self) -> Result<serde_json::Value, RedfishError> {
        let mut b = self.bios().await?;
        b.remove("Attributes")
            .ok_or_else(|| RedfishError::MissingKey {
                key: "Attributes".to_string(),
                url: format!("Systems/{}/Bios", self.system_id()),
            })
    }

    pub async fn get_account_by_id(
        &self,
        account_id: &str,
    ) -> Result<ManagerAccount, RedfishError> {
        let url = format!("AccountService/Accounts/{account_id}");
        let (_status_code, body) = self.client.get(&url).await?;
        Ok(body)
    }

    /// Iterates all accounts comparing the username. In practice I've never seen a BMC with more
    /// than about three accounts, so perf not a concern.
    /// Returns an error if the account does not exist.
    pub async fn get_account_by_name(
        &self,
        username: &str,
    ) -> Result<ManagerAccount, RedfishError> {
        let account_ids = self.get_members("AccountService/Accounts").await?;
        for id in account_ids {
            let account = self.get_account_by_id(&id).await?;
            if account.username == username {
                return Ok(account);
            }
        }
        Err(RedfishError::UserNotFound(username.to_string()))
    }

    /// Dell ships with all sixteen user accounts populated but disabled.
    /// To create an account we have to edit one of them.
    pub async fn edit_account(
        &self,
        account_id: u8,
        username: &str,
        password: &str,
        role_id: RoleId,
        enabled: bool,
    ) -> Result<(), RedfishError> {
        let url = format!("AccountService/Accounts/{account_id}");
        let account = ManagerAccount {
            id: None, // it's in the URL, must not be set here
            username: username.to_string(),
            password: Some(password.to_string()),
            enabled: Some(enabled),
            role_id: role_id.to_string(),
            ..Default::default()
        };
        self.client
            .patch(&url, &account)
            .await
            .map(|_status_code| Ok(()))?
    }

    //
    // PRIVATE
    //

    /// Query the power status from the server
    #[allow(dead_code)]
    pub async fn get_power_status(&self) -> Result<power::Power, RedfishError> {
        let url = format!("Chassis/{}/Power/", self.system_id());
        let (_status_code, body) = self.client.get(&url).await?;
        Ok(body)
    }

    /// Query the power supplies and voltages stats from the server
    pub async fn get_power_metrics(&self) -> Result<power::Power, RedfishError> {
        let url = format!("Chassis/{}/Power/", self.system_id());
        let (_status_code, body) = self.client.get(&url).await?;
        Ok(body)
    }

    /// Query the thermal status from the server
    pub async fn get_thermal_metrics(&self) -> Result<thermal::Thermal, RedfishError> {
        let url = format!("Chassis/{}/Thermal/", self.system_id());
        let (_status_code, body) = self.client.get(&url).await?;
        Ok(body)
    }

    pub async fn change_bios_password(
        &self,
        password_name: &str,
        current_bios_password: &str,
        new_bios_password: &str,
    ) -> Result<Option<String>, RedfishError> {
        let mut url = format!("Systems/{}/Bios/", self.system_id);

        match self.vendor {
            Some(RedfishVendor::Hpe) => {
                url = format!("{}Settings/Actions/Bios.ChangePasswords", url);
            }
            _ => {
                url = format!("{}Actions/Bios.ChangePassword", url);
            }
        }

        let mut arg = HashMap::new();
        arg.insert("PasswordName", password_name.to_string());
        arg.insert("OldPassword", current_bios_password.to_string());
        arg.insert("NewPassword", new_bios_password.to_string());
        self.client.post(&url, arg).await.map(|_resp| Ok(None))?
    }

    /// Query the network service settings for the server
    pub async fn get_manager_network_protocol(
        &self,
    ) -> Result<ManagerNetworkProtocol, RedfishError> {
        let url = format!("Managers/{}/NetworkProtocol", self.manager_id(),);
        let (_status_code, body) = self.client.get(&url).await?;
        Ok(body)
    }
}

// Key/value pairs that different between these two sets of attributes
// The left needs to be a full map, but the right side only needs to support `get`.
fn attr_diff(
    l: &serde_json::Map<String, serde_json::Value>,
    r: &serde_json::Value,
) -> HashMap<String, serde_json::Value> {
    l.iter()
        .filter(|(k, v)| r.get(k) != Some(v))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}
