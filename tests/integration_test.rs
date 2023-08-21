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
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::Once,
    thread::sleep,
    time::Duration,
};

use anyhow::anyhow;
use libredfish::Redfish;

const ROOT_DIR: &str = env!("CARGO_MANIFEST_DIR");

// Ports we hope are not in use
const DELL_PORT: &str = "8733";
const LENOVO_PORT: &str = "8734";
const NVIDIA_PORT: &str = "8735";

static SETUP_LOGGING: Once = Once::new();

#[test]
fn test_dell() -> Result<(), anyhow::Error> {
    run_integration_test("dell", DELL_PORT)
}

#[test]
fn test_lenovo() -> Result<(), anyhow::Error> {
    run_integration_test("lenovo", LENOVO_PORT)
}

#[test]
fn test_nvidia_dpu() -> Result<(), anyhow::Error> {
    run_integration_test("nvidia_dpu", NVIDIA_PORT)
}

fn nvidia_dpu_integration_test(redfish: &dyn Redfish) -> Result<(), anyhow::Error> {
    let members = redfish.get_software_inventories()?.members;
    assert!(!members.is_empty());
    let v: Vec<&str> = members[0].odata_id.split('/').collect();
    assert!(redfish.get_firmware(v.last().unwrap())?.version.is_some());
    let boot = redfish.get_system()?.boot;
    let mut boot_array = boot.boot_order.clone();
    assert!(boot_array.len() > 1);
    boot_array.swap(0, 1);
    redfish.change_boot_order(boot_array)?;

    Ok(())
}

fn run_integration_test(vendor_dir: &'static str, port: &'static str) -> Result<(), anyhow::Error> {
    SETUP_LOGGING.call_once(|| {
        use tracing_subscriber::fmt::Layer;
        use tracing_subscriber::prelude::*;
        use tracing_subscriber::{filter::LevelFilter, EnvFilter};
        tracing_subscriber::registry()
            .with(
                EnvFilter::builder()
                    .with_default_directive(LevelFilter::INFO.into())
                    .from_env_lossy(),
            )
            .with(
                Layer::default()
                    .compact()
                    .with_file(true)
                    .with_line_number(true)
                    .with_ansi(false),
            )
            .init();
    });
    let mut pip = PathBuf::new();
    let mut python = PathBuf::new();
    match std::env::var_os("CI") {
        Some(ci_env) => {
            println!("Running in a GitLab CI job {:?}", ci_env);
            if let Ok(python_path) = std::env::var("PYTHON_PATH") {
                python.push(python_path)
            } else {
                return Err(anyhow::Error::msg("`python` not found"));
            }

            if let Ok(pip_path) = std::env::var("PIP_PATH") {
                pip.push(pip_path)
            } else {
                return Err(anyhow::Error::msg("`pip` not found"));
            }
        }
        None => {
            println!("Not running in a GitLab CI job");
            pip = match find_path("pip") {
                Some(p) => p,
                None => return Err(anyhow::Error::msg("`pip` not found")),
            };
            python = match find_path("python") {
                Some(p) => p,
                None => return Err(anyhow::Error::msg("`python` not found")),
            };
        }
    }
    let mut mockup_server = match MockupServer::new(vendor_dir, port, pip, python) {
        Some(s) => s,
        None => {
            return Ok(());
        }
    };

    mockup_server.install_python_requirements()?;
    mockup_server.start()?; // stops on drop

    let endpoint = libredfish::Endpoint {
        host: format!("127.0.0.1:{port}"),
        ..Default::default()
    };

    let pool = libredfish::RedfishClientPool::builder().build()?;
    let redfish = pool.create_client(endpoint)?;

    if vendor_dir == "nvidia_dpu" {
        return nvidia_dpu_integration_test(redfish.as_ref());
    }

    assert_eq!(redfish.get_power_state()?, libredfish::PowerState::On);
    assert!(redfish.bios()?.len() > 10);

    redfish.power(libredfish::SystemPowerControl::GracefulShutdown)?;
    redfish.power(libredfish::SystemPowerControl::ForceOff)?;
    redfish.power(libredfish::SystemPowerControl::On)?;

    // A real BMC requires a reboot after every change, so pretend for accuracy.
    // Dell will 400 Bad Request if you make two consecutive changes.
    redfish.lockdown(libredfish::EnabledDisabled::Disabled)?;
    redfish.power(libredfish::SystemPowerControl::ForceRestart)?;
    if vendor_dir == "dell" {
        // we're testing against static files, so these don't change
        assert!(redfish.lockdown_status()?.is_fully_disabled());
    }

    redfish.setup_serial_console()?;
    redfish.power(libredfish::SystemPowerControl::ForceRestart)?;
    assert!(redfish.serial_console_status()?.is_fully_enabled());

    redfish.clear_tpm()?;
    // The mockup includes TPM clear pending operation
    assert!(!redfish.pending()?.is_empty());
    redfish.power(libredfish::SystemPowerControl::ForceRestart)?;

    redfish.boot_once(libredfish::Boot::Pxe)?;
    redfish.boot_first(libredfish::Boot::HardDisk)?;
    redfish.power(libredfish::SystemPowerControl::ForceRestart)?;

    redfish.lockdown(libredfish::EnabledDisabled::Enabled)?;
    redfish.power(libredfish::SystemPowerControl::GracefulRestart)?;
    if vendor_dir == "lenovo" {
        assert!(redfish.lockdown_status()?.is_fully_enabled());
    }

    Ok(())
}

struct MockupServer {
    vendor_dir: &'static str,
    port: &'static str,
    pip: PathBuf,
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
    // Creates a server if pip and python is present, otherwise returns None
    fn new(
        vendor_dir: &'static str,
        port: &'static str,
        pip: PathBuf,
        python: PathBuf,
    ) -> Option<MockupServer> {
        Some(MockupServer {
            vendor_dir,
            port,
            pip,
            python,
            process: None,
        })
    }

    fn install_python_requirements(&self) -> Result<(), anyhow::Error> {
        let req_path = PathBuf::from(ROOT_DIR)
            .join("tests")
            .join("requirements.txt");
        let exit_code = Command::new(&self.pip)
            .arg("install")
            .arg("-q")
            .arg("--requirement")
            .arg(&req_path)
            .status()?;
        if !exit_code.success() {
            return Err(anyhow!(
                "Failed running '{} install -q --requirement {}. Exit code {}",
                self.pip.display(),
                req_path.display(),
                exit_code
            ));
        }
        Ok(())
    }

    fn start(&mut self) -> std::io::Result<()> {
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

fn find_path<P>(bin: P) -> Option<PathBuf>
where
    P: AsRef<Path>,
{
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let full_path = dir.join(&bin);
            if full_path.is_file() {
                Some(full_path)
            } else {
                None
            }
        })
    })
}
