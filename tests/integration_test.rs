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
/// Test against a mockup of BMC. A mockup is a directory of JSON files mirrored from a real BMC>
/// This makes for very good test for GET (e.g. get_power_state) calls, but is only a basic test
/// for POST/PATCH. For those the mockup server checks the path exists but doesn't check the body
/// values, and always returns '204 No Content'.
///
/// See tests/mockup/README for details.
use std::{
    collections::HashSet,
    env,
    path::PathBuf,
    process::{Child, Command},
    sync::Once,
    thread::sleep,
    time::Duration,
};

use anyhow::{anyhow, Context};
use libredfish::model::service_root::RedfishVendor;
use libredfish::model::{ComputerSystem, ODataId};
use libredfish::{
    model::{
        resource::{IsResource, ResourceCollection},
        Manager,
    },
    Chassis, EthernetInterface, NetworkAdapter, PCIeDevice, Redfish,
};
use tracing::debug;

const ROOT_DIR: &str = env!("CARGO_MANIFEST_DIR");
const PYTHON_VENV_DIR: &str = "libredfish-python-venv";

// Ports we hope are not in use
const DELL_PORT: &str = "8733";
const HPE_PORT: &str = "8734";
const LENOVO_PORT: &str = "8735";
const NVIDIA_DPU_PORT: &str = "8736";
const NVIDIA_VIKING_PORT: &str = "8737";
const SUPERMICRO_PORT: &str = "8738";

static SETUP: Once = Once::new();

#[tokio::test]
async fn test_dell() -> Result<(), anyhow::Error> {
    run_integration_test("dell", DELL_PORT).await
}

#[tokio::test]
async fn test_hpe() -> Result<(), anyhow::Error> {
    run_integration_test("hpe", HPE_PORT).await
}

#[tokio::test]
async fn test_lenovo() -> Result<(), anyhow::Error> {
    run_integration_test("lenovo", LENOVO_PORT).await
}

#[tokio::test]
async fn test_nvidia_dpu() -> Result<(), anyhow::Error> {
    run_integration_test("nvidia_dpu", NVIDIA_DPU_PORT).await
}

#[tokio::test]
async fn test_nvidia_viking() -> Result<(), anyhow::Error> {
    run_integration_test("nvidia_viking", NVIDIA_VIKING_PORT).await
}

#[tokio::test]
async fn test_supermicro() -> Result<(), anyhow::Error> {
    run_integration_test("supermicro", SUPERMICRO_PORT).await
}

async fn nvidia_dpu_integration_test(redfish: &dyn Redfish) -> Result<(), anyhow::Error> {
    let vendor = redfish.get_service_root().await?.vendor;
    assert!(vendor.is_some() && vendor.unwrap() == "Nvidia");
    let sw_inventories = redfish.get_software_inventories().await?;
    assert!(redfish
        .get_firmware(&sw_inventories[0])
        .await?
        .version
        .is_some());
    let boot = redfish.get_system().await?.boot;
    let mut boot_array = boot.boot_order;
    assert!(boot_array.len() > 1);
    boot_array.swap(0, 1);
    redfish.change_boot_order(boot_array).await?;

    let system = redfish.get_system().await?;
    assert_ne!(system.serial_number, None);

    let manager_eth_interfaces = redfish.get_manager_ethernet_interfaces().await?;
    assert!(!manager_eth_interfaces.is_empty());
    assert!(redfish
        .get_manager_ethernet_interface(&manager_eth_interfaces[0])
        .await?
        .mac_address
        .is_some());

    let chassis = redfish.get_chassis_all().await?;
    assert!(!chassis.is_empty());
    assert!(redfish.get_chassis(&chassis[0]).await?.name.is_some());

    let ports = redfish.get_ports(&chassis[0]).await?;
    assert!(!ports.is_empty());
    assert!(redfish
        .get_port(&chassis[0], &ports[0])
        .await?
        .current_speed_gbps
        .is_some());

    let netdev_funcs = redfish.get_network_device_functions(&chassis[0]).await?;
    assert!(!netdev_funcs.is_empty());
    assert!(redfish
        .get_network_device_function(&chassis[0], &netdev_funcs[0], None)
        .await?
        .ethernet
        .and_then(|ethernet| ethernet.mac_address)
        .is_some());

    assert_ne!(chassis.iter().find(|&x| *x == "Card1"), None);
    let chassis = redfish.get_chassis("Card1").await?;
    assert_ne!(chassis.serial_number, None);

    assert_eq!(
        chassis.serial_number.as_ref().unwrap().trim(),
        system.serial_number.as_ref().unwrap().trim()
    );

    Ok(())
}

async fn run_integration_test(
    vendor_dir: &'static str,
    port: &'static str,
) -> Result<(), anyhow::Error> {
    SETUP.call_once(move || {
        use tracing_subscriber::fmt::Layer;
        use tracing_subscriber::prelude::*;
        use tracing_subscriber::{filter::LevelFilter, EnvFilter};
        tracing_subscriber::registry()
            .with(
                EnvFilter::builder()
                    .with_default_directive(LevelFilter::INFO.into())
                    .from_env_lossy()
                    .add_directive("hyper=warn".parse().unwrap())
                    .add_directive("reqwest=warn".parse().unwrap())
                    .add_directive("rustls=warn".parse().unwrap()),
            )
            .with(
                Layer::default()
                    .compact()
                    .with_file(true)
                    .with_line_number(true)
                    .with_ansi(false),
            )
            .init();

        let pip = create_python_venv().expect("Failed creating python virtual env");
        install_python_requirements(pip).expect("failed installing python requirements");
    });
    let python = env::temp_dir()
        .join(PYTHON_VENV_DIR)
        .join("bin")
        .join("python");
    let mut mockup_server = MockupServer {
        vendor_dir,
        port,
        python,
        process: None,
    };
    mockup_server.start()?; // stops on drop

    let endpoint = libredfish::Endpoint {
        host: format!("127.0.0.1:{port}"),
        ..Default::default()
    };

    let pool = libredfish::RedfishClientPool::builder().build()?;
    let redfish = pool.create_client(endpoint).await?;

    if vendor_dir == "nvidia_dpu" {
        return nvidia_dpu_integration_test(redfish.as_ref()).await;
    }

    // Inspect the system
    let _system = redfish.get_system().await?;

    let mut all_macs = HashSet::new();
    let manager_eth_interfaces = redfish.get_manager_ethernet_interfaces().await?;
    assert!(!manager_eth_interfaces.is_empty());
    let mut manager_eth_interface_states = Vec::new();
    for iface in &manager_eth_interfaces {
        let state = redfish.get_manager_ethernet_interface(iface).await?;
        let mac = state.mac_address.clone().unwrap();
        if !all_macs.insert(mac.clone()) {
            panic!("Duplicate MAC address {} on interface {}", mac, iface);
        }
        manager_eth_interface_states.push(state);
    }

    let system_eth_interfaces = redfish.get_system_ethernet_interfaces().await?;
    assert!(!system_eth_interfaces.is_empty());
    let mut system_eth_interface_states: Vec<libredfish::EthernetInterface> = Vec::new();
    for iface in &system_eth_interfaces {
        let state = redfish.get_system_ethernet_interface(iface).await?;
        let mac = state.mac_address.clone().unwrap();
        if !all_macs.insert(mac.clone()) {
            panic!("Duplicate MAC address {} on interface {}", mac, iface);
        }
        system_eth_interface_states.push(state);
    }

    let chassis = redfish.get_chassis_all().await?;
    assert!(!chassis.is_empty());
    for chassis_id in &chassis {
        let Ok(chassis_net_adapters) = redfish.get_chassis_network_adapters(chassis_id).await
        else {
            continue;
        };
        for net_adapter_id in &chassis_net_adapters {
            let _value = redfish
                .get_chassis_network_adapter(chassis_id, net_adapter_id)
                .await?;
        }

        if vendor_dir == "hpe" {
            let adapter_ids = redfish.get_base_network_adapters(chassis_id).await?;
            assert!(!adapter_ids.is_empty());
            for adapter_id in &adapter_ids {
                redfish
                    .get_base_network_adapter(chassis_id, adapter_id)
                    .await?;
            }
        }
    }

    assert_eq!(redfish.get_power_state().await?, libredfish::PowerState::On);
    assert!(redfish.bios().await?.len() > 8);

    redfish
        .power(libredfish::SystemPowerControl::GracefulShutdown)
        .await?;
    redfish
        .power(libredfish::SystemPowerControl::ForceOff)
        .await?;
    redfish.power(libredfish::SystemPowerControl::On).await?;

    // A real BMC requires a reboot after every change, so pretend for accuracy.
    // Dell will 400 Bad Request if you make two consecutive changes.
    redfish
        .lockdown(libredfish::EnabledDisabled::Disabled)
        .await?;
    redfish
        .power(libredfish::SystemPowerControl::ForceRestart)
        .await?;
    if vendor_dir == "dell" {
        // we're testing against static files, so these don't change
        assert!(redfish.lockdown_status().await?.is_fully_disabled());
    }

    redfish.setup_serial_console().await?;
    redfish
        .power(libredfish::SystemPowerControl::ForceRestart)
        .await?;
    assert!(redfish.serial_console_status().await?.is_fully_enabled());

    if vendor_dir != "supermicro" {
        redfish.clear_tpm().await?;
        // The mockup includes TPM clear pending operation
        assert!(!redfish.pending().await?.is_empty());
    }
    redfish
        .power(libredfish::SystemPowerControl::ForceRestart)
        .await?;

    redfish.boot_once(libredfish::Boot::Pxe).await?;
    redfish.boot_first(libredfish::Boot::HardDisk).await?;
    redfish
        .power(libredfish::SystemPowerControl::ForceRestart)
        .await?;

    redfish
        .lockdown(libredfish::EnabledDisabled::Enabled)
        .await?;
    redfish
        .power(libredfish::SystemPowerControl::GracefulRestart)
        .await?;
    if vendor_dir == "lenovo" {
        assert!(redfish.lockdown_status().await?.is_fully_enabled());
    }
    _ = redfish.get_thermal_metrics().await?;
    _ = redfish.get_power_metrics().await?;
    if vendor_dir != "lenovo" && vendor_dir != "supermicro" {
        // the lenovo mockup doesn't have this content, but their docs have it
        _ = redfish.get_system_event_log().await?;
    }

    if vendor_dir == "nvidia_viking" {
        redfish.set_boot_order_dpu_first(None).await?
    }
    resource_tests(&redfish).await?;

    Ok(())
}

async fn resource_tests(redfish: &Box<dyn Redfish>) -> Result<(), anyhow::Error> {
    pub enum UriType {
        ODataId(ODataId),
        OptionODataId(Option<ODataId>),
    }
    fn verify_collection<T: serde::de::DeserializeOwned + IsResource>(
        col: &ResourceCollection<T>,
        vendor: RedfishVendor,
    ) {
        assert_eq!(
            col.count as usize - col.failed_to_deserialize_count as usize,
            col.members.len()
        );
        let collection_type = col
            .odata
            .clone()
            .odata_type
            .split(".")
            .last()
            .unwrap_or_default()
            .replace("Collection", "");
        for m in &col.members {
            let member_odata_type = m.odata_type();
            let member_odata_type = member_odata_type
                .split(".")
                .last()
                .unwrap_or("unknown-type");
            // viking's mockup data contains some chassis w.o @odata.type, until we clean up mockup data we
            // need to bypass that case
            if member_odata_type == "" && vendor == RedfishVendor::AMI {
                continue;
            }
            assert_eq!(collection_type, member_odata_type);
        }
    }
    async fn test_type<T>(
        redfish: &Box<dyn Redfish>,
        uri: UriType,
        vendor: RedfishVendor,
    ) -> Result<ResourceCollection<T>, anyhow::Error>
    where
        T: serde::de::DeserializeOwned + IsResource,
    {
        let id: ODataId = match uri {
            UriType::ODataId(x) => x,
            UriType::OptionODataId(x) => match x {
                Some(x) => x,
                None => return Err(anyhow!("Uri is none Option<ODataId>")),
            },
        };

        match redfish.get_collection(id).await.and_then(|c| c.try_get()) {
            Ok(x) => {
                verify_collection(&x, vendor);
                Ok(x)
            }
            Err(e) => return Err(anyhow!(e.to_string())),
        }
    }

    let service_root = redfish.get_service_root().await?;
    assert!(service_root.vendor().is_some());
    let vendor = service_root.vendor().unwrap();
    let _managers_rc = test_type::<Manager>(
        redfish,
        UriType::OptionODataId(service_root.managers.clone()),
        vendor,
    )
    .await?;
    let chassis_rc = test_type::<Chassis>(
        redfish,
        UriType::OptionODataId(service_root.chassis.clone()),
        vendor,
    )
    .await?;
    let _systems_rc = test_type::<ComputerSystem>(
        redfish,
        UriType::OptionODataId(service_root.systems.clone()),
        vendor,
    )
    .await?;

    let chassis_id: &str;
    match vendor {
        RedfishVendor::Lenovo | RedfishVendor::Supermicro | RedfishVendor::Hpe => chassis_id = "1",
        RedfishVendor::AMI => chassis_id = "DGX",
        RedfishVendor::Nvidia => chassis_id = "Card1",
        RedfishVendor::Dell => chassis_id = "System.Embedded.1",
        _ => return Err(anyhow!("Unknown vendor")),
    };
    if vendor != RedfishVendor::Nvidia {
        let ch = match chassis_rc
            .members
            .iter()
            .into_iter()
            .find(|c| c.id.clone().unwrap_or_default() == chassis_id)
        {
            Some(x) => x,
            None => return Err(anyhow!("Chassis with id {} not found", chassis_id)),
        };

        if let Some(pcie_devs_oid) = ch.pcie_devices.as_ref() {
            debug!("Testing pcie_devices");
            let _pcie_devs_rc = test_type::<PCIeDevice>(
                redfish,
                UriType::ODataId(pcie_devs_oid.to_owned()),
                vendor,
            )
            .await?;
        }

        if let Some(nw_adapters_oid) = ch.network_adapters.as_ref() {
            debug!("Testing network_adapters");
            let _nw_adapter_rc = test_type::<NetworkAdapter>(
                redfish,
                UriType::ODataId(nw_adapters_oid.to_owned()),
                vendor,
            )
            .await?;
        }

        let sys = redfish.get_system().await?;
        let sys2 = redfish
            .get_resource(sys.odata.odata_id.into())
            .await
            .and_then(|t| t.try_get::<ComputerSystem>())?;

        assert_eq!(sys.model.as_ref(), sys2.model.as_ref());
        assert_eq!(sys.id, sys2.id);

        if let Some(sys_ethernet_interfaces_id) = sys.ethernet_interfaces.as_ref() {
            debug!("Testing system.ethernet_interfaces");
            let nw_ethernet_rc = test_type::<EthernetInterface>(
                redfish,
                UriType::ODataId(sys_ethernet_interfaces_id.to_owned()),
                vendor,
            )
            .await?;
            debug!("{} ethernet_interfaces found", nw_ethernet_rc.count);
        }
    }
    Ok(())
}

/// Create a python virtualenv to install our requirements into.
/// Return the path of pip
fn create_python_venv() -> Result<PathBuf, anyhow::Error> {
    let venv_dir = env::temp_dir().join(PYTHON_VENV_DIR);
    let venv_out = Command::new("python3")
        .arg("-m")
        .arg("venv")
        .arg(&venv_dir)
        .output()
        .context("Is 'python3' on your $PATH?")?;
    if !venv_out.status.success() {
        eprintln!("*** Python virtual env creation failed:");
        eprintln!("\tSTDOUT: {}", String::from_utf8_lossy(&venv_out.stdout));
        eprintln!("\tSTDERR: {}", String::from_utf8_lossy(&venv_out.stderr));
        return Err(anyhow!(
            "Failed running 'python3 -m venv {}. Exit code {}",
            venv_dir.display(),
            venv_out.status.code().unwrap_or(-1),
        ));
    }

    Ok(venv_dir.join("bin/pip"))
}

fn install_python_requirements(pip: PathBuf) -> Result<(), anyhow::Error> {
    let req_path = PathBuf::from(ROOT_DIR)
        .join("tests")
        .join("requirements.txt");
    let output = Command::new(&pip)
        .arg("install")
        .arg("-q")
        .arg("--requirement")
        .arg(&req_path)
        .output()?;
    if !output.status.success() {
        eprintln!("*** pip install failed:");
        eprintln!("\tSTDOUT: {}", String::from_utf8_lossy(&output.stdout));
        eprintln!("\tSTDERR: {}", String::from_utf8_lossy(&output.stderr));
        return Err(anyhow!(
            "Failed running '{} install -q --requirement {}. Exit code {}",
            pip.display(),
            req_path.display(),
            output.status.code().unwrap_or(-1),
        ));
    }
    Ok(())
}

struct MockupServer {
    vendor_dir: &'static str,
    port: &'static str,
    python: PathBuf,

    process: Option<Child>,
}

impl Drop for MockupServer {
    fn drop(&mut self) {
        if self.process.is_none() {
            return;
        }
        self.process.take().unwrap().kill().unwrap();
        sleep(Duration::from_secs(1)); // let it stop
    }
}

impl MockupServer {
    fn start(&mut self) -> std::io::Result<()> {
        // For extra debugging edit redfishMockupServer.py change the log level at the top
        self.process = Some(
            Command::new(&self.python)
                .current_dir(PathBuf::from(ROOT_DIR).join("tests"))
                .arg("redfishMockupServer.py")
                .arg("--port")
                .arg(self.port)
                .arg("--dir")
                .arg(format!("mockups/{}/", self.vendor_dir))
                .arg("--ssl")
                .arg("--cert")
                .arg("cert.pem")
                .arg("--key")
                .arg("key.pem")
                .spawn()?,
        );
        sleep(Duration::from_secs(1)); // let it start
        Ok(())
    }
}
