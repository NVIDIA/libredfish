//! `rune` vendor — a scriptable BMC backed by a [Rune](https://rune-rs.github.io) script.
//!
//! Selected via the vendor-override file (`vendor` "Rune" plus a `script` path). Each
//! `Redfish` method dispatches to a same-named script function and falls back to
//! [`RedfishStandard`] when the script doesn't define one (methods with complex or
//! non-`Deserialize` arguments always delegate). A script reaches the BMC through a
//! [`RedfishCtx`] handle (`ctx`) plus a set of free helper functions called directly.
//!
//! On `ctx`: the HTTP verbs `get`/`patch`/`post`/`delete` and the accessors
//! `system_id()`/`manager_id()`/`variant()`/`bmc_address()`. Called directly, no `ctx`:
//! `sha256`/`sha512`, `b64_encode`/`b64_decode`, `json_encode`/`json_decode`, the host helpers
//! `read_file`/`read_env`, and the clock `unix_time`. The HTTP verbs return
//! `Ok(#{ status, headers, body })` and the fallible helpers return `Ok(..)`/`Err(msg)`, so a
//! script can branch with `match` or use `?` to fail the method (an `Err` surfaces to the
//! caller as a `RedfishError`).
//!
//! `read_file`/`read_env` reach the host filesystem and environment, and scripts run with the
//! process's privileges under no sandbox or resource limits — load trusted scripts only.
//!
//! The full script-facing surface — host API, the methods a script may override, and the
//! available language/std features — is catalogued in `tests/rune/README.md`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use reqwest::header::HeaderMap;
use reqwest::{Method, StatusCode};
use rune::runtime::{Args, Ref, RuntimeContext, Unit, Value, VmError, VmResult};
use rune::{
    Any, Context, ContextError, Diagnostics, FromValue, Module, Source, Sources, ToValue, Vm,
};
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256, Sha512};

use crate::model::account_service::ManagerAccount;
use crate::model::certificate::Certificate;
use crate::model::component_integrity::{CaCertificate, ComponentIntegrities, Evidence};
use crate::model::oem::nvidia_dpu::{HostPrivilegeLevel, NicMode};
use crate::model::power::Power;
use crate::model::secure_boot::SecureBoot;
use crate::model::sel::LogEntry;
use crate::model::sensor::GPUSensors;
use crate::model::service_root::ServiceRoot;
use crate::model::software_inventory::SoftwareInventory;
use crate::model::storage::Drives;
use crate::model::task::Task;
use crate::model::thermal::Thermal;
use crate::model::update_service::{ComponentType, TransferProtocolType, UpdateService};
use crate::model::{BootOption, ComputerSystem, Manager, ODataId};
use crate::network::RedfishHttpClient;
use crate::standard::RedfishStandard;
use crate::{
    Assembly, BiosProfileType, BiosProfileVendor, Boot, BootOptions, BootOverride, Chassis,
    Collection, EnabledDisabled, EthernetInterface, JobState, MachineSetupStatus, NetworkAdapter,
    NetworkDeviceFunction, NetworkPort, PCIeDevice, PowerState, Redfish, RedfishError,
    RedfishFuture, Resource, RoleId, Status, SystemPowerControl,
};

// Host API exposed to scripts.

/// Context handed to a Rune script. Holds the BMC HTTP client plus resolved ids and variant.
#[derive(Any, Clone)]
pub(crate) struct RedfishCtx {
    client: RedfishHttpClient,
    system_id: String,
    manager_id: String,
    variant: Option<String>,
}

fn vm_err<T>(msg: String) -> VmResult<T> {
    VmResult::err(VmError::panic(msg))
}

/// `GET {path}` → `Ok(#{ status, headers, body })` or `Err(message)`.
#[rune::function(instance)]
async fn get(ctx: Ref<RedfishCtx>, path: String) -> VmResult<Value> {
    http_call(&ctx, Method::GET, &path, None).await
}

/// `PATCH {path}` with JSON `body` → `Ok(#{ status, headers, body })` or `Err(message)`.
#[rune::function(instance)]
async fn patch(ctx: Ref<RedfishCtx>, path: String, body: Value) -> VmResult<Value> {
    http_call(&ctx, Method::PATCH, &path, Some(body)).await
}

/// `POST {path}` with JSON `body` → `Ok(#{ status, headers, body })` or `Err(message)`.
#[rune::function(instance)]
async fn post(ctx: Ref<RedfishCtx>, path: String, body: Value) -> VmResult<Value> {
    http_call(&ctx, Method::POST, &path, Some(body)).await
}

/// `DELETE {path}` → `Ok(#{ status, headers, body })` or `Err(message)`.
#[rune::function(instance)]
async fn delete(ctx: Ref<RedfishCtx>, path: String) -> VmResult<Value> {
    http_call(&ctx, Method::DELETE, &path, None).await
}

/// Run an HTTP request and hand the script a `Result` value (built via `ToValue`) so
/// scripts can `match` or `?` it instead of the VM unwinding.
async fn http_call(
    ctx: &RedfishCtx,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> VmResult<Value> {
    result_to_value(do_http(ctx, method, path, body).await, "http")
}

/// The request itself. Returns `Ok(#{status, headers, body})` on a completed HTTP
/// exchange and `Err(message)` on a transport or encode failure.
async fn do_http(
    ctx: &RedfishCtx,
    method: Method,
    path: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let json_body: Option<serde_json::Value> = match body {
        Some(v) => Some(
            serde_json::to_value(&v).map_err(|e| format!("{method} {path}: encode body: {e}"))?,
        ),
        None => None,
    };
    match ctx
        .client
        .req::<serde_json::Value, serde_json::Value>(
            method.clone(),
            path,
            json_body,
            None,
            None,
            Vec::new(),
        )
        .await
    {
        Ok((status, body_opt, headers_opt)) => {
            let resp = response_json(status, headers_opt, body_opt);
            serde_json::from_value::<Value>(resp)
                .map_err(|e| format!("{method} {path}: decode response: {e}"))
        }
        Err(e) => Err(format!("{method} {path}: {e}")),
    }
}

/// Build the `#{ status, headers, body }` response object scripts receive.
fn response_json(
    status: StatusCode,
    headers: Option<HeaderMap>,
    body: Option<serde_json::Value>,
) -> serde_json::Value {
    let mut hdrs = serde_json::Map::new();
    if let Some(h) = headers {
        for (name, value) in h.iter() {
            hdrs.insert(
                name.as_str().to_string(),
                serde_json::Value::String(value.to_str().unwrap_or_default().to_string()),
            );
        }
    }
    serde_json::json!({
        "status": status.as_u16(),
        "headers": serde_json::Value::Object(hdrs),
        "body": body.unwrap_or(serde_json::Value::Null),
    })
}

/// Bridge a rune `Value` to `T` via serde_json.
fn bridge<T: DeserializeOwned>(value: &Value, name: &str) -> Result<T, RedfishError> {
    let json = serde_json::to_value(value).map_err(|e| RedfishError::GenericError {
        error: format!("rune {name}: result encode: {e}"),
    })?;
    serde_json::from_value::<T>(json).map_err(|e| RedfishError::GenericError {
        error: format!("rune {name}: result -> {}: {e}", std::any::type_name::<T>()),
    })
}

/// Stringify the payload `Value` of a script `Err`.
fn value_to_string(v: &Value) -> String {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => s,
        Ok(other) => other.to_string(),
        Err(_) => "<unprintable rune error>".to_string(),
    }
}

/// Interpret a script's return value. A top level `Err(..)`, from `?` on a failed HTTP
/// call or an explicit `return Err(..)`, becomes a [`RedfishError`]. An `Ok(v)` is
/// unwrapped. Any other value is bridged directly. This is the script error channel.
fn interpret<T: DeserializeOwned>(value: Value, name: &str) -> Result<T, RedfishError> {
    match <Result<Value, Value>>::from_value(value.clone()) {
        Ok(Ok(inner)) => bridge(&inner, name),
        Ok(Err(e)) => Err(RedfishError::GenericError {
            error: format!("rune {name}: script error: {}", value_to_string(&e)),
        }),
        Err(_) => bridge(&value, name),
    }
}

#[rune::function(instance)]
fn system_id(ctx: &RedfishCtx) -> String {
    ctx.system_id.clone()
}

#[rune::function(instance)]
fn manager_id(ctx: &RedfishCtx) -> String {
    ctx.manager_id.clone()
}

#[rune::function(instance)]
fn variant(ctx: &RedfishCtx) -> Option<String> {
    ctx.variant.clone()
}

/// `ctx.bmc_address()` → the BMC host this client targets: hostname or IP, no scheme,
/// port, or path. This is the same address the vendor-override file matched on (its
/// `addr` key), so a script can use it to key per-host behavior.
#[rune::function(instance)]
fn bmc_address(ctx: &RedfishCtx) -> String {
    ctx.client.host().to_string()
}

/// `sha256(data)` → lowercase-hex SHA-256 of `data`'s UTF-8 bytes. Free function.
#[rune::function]
fn sha256(data: String) -> String {
    hex_lower(Sha256::digest(data.as_bytes()))
}

/// `sha512(data)` → lowercase-hex SHA-512 of `data`'s UTF-8 bytes. Free function.
#[rune::function]
fn sha512(data: String) -> String {
    hex_lower(Sha512::digest(data.as_bytes()))
}

/// Lowercase-hex encode bytes (backs the `sha256`/`sha512` script helpers).
fn hex_lower(bytes: impl AsRef<[u8]>) -> String {
    use std::fmt::Write as _;
    let bytes = bytes.as_ref();
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Hand a `Result<T, String>` to a script as a matchable rune `Ok(..)`/`Err(..)` value (the
/// same convention the HTTP verbs use), or raise a VM error if the value can't be encoded.
fn result_to_value<T: ToValue>(result: Result<T, String>, name: &str) -> VmResult<Value> {
    match result.to_value() {
        Ok(v) => VmResult::Ok(v),
        Err(e) => vm_err(format!("rune {name}: encode result: {e}")),
    }
}

/// `b64_encode(data)` → standard-alphabet, padded base64 of `data`'s UTF-8 bytes. Free function.
#[rune::function]
fn b64_encode(data: String) -> String {
    BASE64.encode(data.as_bytes())
}

/// `b64_decode(data)` → `Ok(text)` when `data` is valid standard base64 that decodes to UTF-8,
/// else `Err(message)`. Match it or `?` it like the HTTP verbs. Free function.
#[rune::function]
fn b64_decode(data: String) -> VmResult<Value> {
    result_to_value(do_b64_decode(&data), "b64_decode")
}

fn do_b64_decode(data: &str) -> Result<String, String> {
    let bytes = BASE64
        .decode(data.as_bytes())
        .map_err(|e| format!("b64_decode: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("b64_decode: invalid utf-8: {e}"))
}

/// `json_encode(value)` → `Ok(json_text)` for any serializable value, else `Err(message)`.
/// Free function.
#[rune::function]
fn json_encode(value: Value) -> VmResult<Value> {
    let encoded = serde_json::to_string(&value).map_err(|e| format!("json_encode: {e}"));
    result_to_value(encoded, "json_encode")
}

/// `json_decode(text)` → `Ok(value)` for valid JSON (object/array/scalar), else `Err(message)`.
/// Free function.
#[rune::function]
fn json_decode(data: String) -> VmResult<Value> {
    let decoded = serde_json::from_str::<Value>(&data).map_err(|e| format!("json_decode: {e}"));
    result_to_value(decoded, "json_decode")
}

/// `read_file(path)` → `Ok(contents)` reading `path` as UTF-8 text, else `Err(message)`.
/// Reaches the host filesystem with the process's privileges; for trusted scripts only.
/// Free function.
#[rune::function]
fn read_file(path: String) -> VmResult<Value> {
    let read = std::fs::read_to_string(&path).map_err(|e| format!("read_file {path}: {e}"));
    result_to_value(read, "read_file")
}

/// `read_env(name)` → the value of environment variable `name`, or `None` if it is unset or
/// not valid UTF-8. Reads the process environment; for trusted scripts only. Free function.
#[rune::function]
fn read_env(name: String) -> Option<String> {
    std::env::var(name).ok()
}

/// `unix_time()` → current wall-clock Unix time in whole seconds since the epoch (0 if the
/// clock is set before 1970). Free function.
#[rune::function]
fn unix_time() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The libredfish host module registered into the Rune context.
fn module() -> Result<Module, ContextError> {
    let mut m = Module::new();
    m.ty::<RedfishCtx>()?;
    m.function_meta(get)?;
    m.function_meta(patch)?;
    m.function_meta(post)?;
    m.function_meta(delete)?;
    m.function_meta(system_id)?;
    m.function_meta(manager_id)?;
    m.function_meta(variant)?;
    m.function_meta(bmc_address)?;
    m.function_meta(sha256)?;
    m.function_meta(sha512)?;
    m.function_meta(b64_encode)?;
    m.function_meta(b64_decode)?;
    m.function_meta(json_encode)?;
    m.function_meta(json_decode)?;
    m.function_meta(read_file)?;
    m.function_meta(read_env)?;
    m.function_meta(unix_time)?;
    Ok(m)
}

// Compilation / runtime.

fn ctx_err(e: impl std::fmt::Display) -> RedfishError {
    RedfishError::GenericError {
        error: format!("rune context: {e}"),
    }
}

/// Build a compile and runtime context from the default std modules plus our host module.
fn build_context() -> Result<Context, RedfishError> {
    let mut context = Context::with_default_modules().map_err(ctx_err)?;
    context
        .install(module().map_err(ctx_err)?)
        .map_err(ctx_err)?;
    Ok(context)
}

/// Shared runtime context, built once from the same module set the units compile against.
fn shared_runtime() -> Result<Arc<RuntimeContext>, RedfishError> {
    static RT: OnceLock<Arc<RuntimeContext>> = OnceLock::new();
    if let Some(rt) = RT.get() {
        return Ok(rt.clone());
    }
    let context = build_context()?;
    let rt = Arc::new(context.runtime().map_err(ctx_err)?);
    let _ = RT.set(rt.clone());
    Ok(rt)
}

/// Cache of compiled units, keyed by script path (invalidated by mtime).
type UnitCache = HashMap<PathBuf, (SystemTime, Arc<Unit>)>;

/// Compile a script to a `Unit`, cached by path + mtime (recompiled on change).
fn compile(path: &str) -> Result<Arc<Unit>, RedfishError> {
    static CACHE: OnceLock<Mutex<UnitCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let p = PathBuf::from(path);

    let mtime = std::fs::metadata(&p)
        .and_then(|m| m.modified())
        .map_err(|e| RedfishError::FileError(format!("rune script {path}: {e}")))?;
    if let Some((t, u)) = cache.lock().unwrap().get(&p) {
        if *t == mtime {
            return Ok(u.clone());
        }
    }

    let src = std::fs::read_to_string(&p)
        .map_err(|e| RedfishError::FileError(format!("rune script {path}: {e}")))?;
    let context = build_context()?;
    let mut sources = Sources::new();
    sources
        .insert(
            Source::new(path, src)
                .map_err(|e| RedfishError::FileError(format!("rune source {path}: {e}")))?,
        )
        .map_err(|e| RedfishError::FileError(format!("rune sources {path}: {e}")))?;
    let mut diagnostics = Diagnostics::new();
    let unit = rune::prepare(&mut sources)
        .with_context(&context)
        .with_diagnostics(&mut diagnostics)
        .build()
        .map_err(|e| RedfishError::FileError(format!("rune compile {path}: {e}")))?;

    let unit = Arc::new(unit);
    cache.lock().unwrap().insert(p, (mtime, unit.clone()));
    Ok(unit)
}

// The vendor.

pub(crate) struct Bmc {
    s: RedfishStandard,
    unit: Arc<Unit>,
    runtime: Arc<RuntimeContext>,
}

impl Bmc {
    pub(crate) fn new(s: RedfishStandard) -> Result<Bmc, RedfishError> {
        let path = s.vendor_script().ok_or_else(|| {
            RedfishError::FileError(
                "Rune vendor selected but no script set (override entry needs a \"script\" path)"
                    .to_string(),
            )
        })?;
        let unit = compile(path)?;
        let runtime = shared_runtime()?;
        Ok(Bmc { s, unit, runtime })
    }

    /// Build a fresh script context. Cheap, just clones the client handle.
    fn ctx(&self) -> RedfishCtx {
        RedfishCtx {
            client: self.s.client.clone(),
            system_id: self.s.system_id().to_string(),
            manager_id: self.s.manager_id().to_string(),
            variant: self.s.vendor_variant().map(str::to_string),
        }
    }

    /// True if the script defines a top-level function `name`.
    fn has(&self, name: &str) -> bool {
        Vm::new(self.runtime.clone(), self.unit.clone())
            .lookup_function([name])
            .is_ok()
    }

    /// Call script function `name` with `args` (Send native values), deserialize the result.
    async fn call<A, T>(&self, name: &str, args: A) -> Result<T, RedfishError>
    where
        A: Args + Send,
        T: DeserializeOwned,
    {
        let execution = Vm::new(self.runtime.clone(), self.unit.clone())
            .send_execute([name], args)
            .map_err(|e| RedfishError::GenericError {
                error: format!("rune {name}: {e}"),
            })?;
        let value = execution
            .async_complete()
            .await
            .into_result()
            .map_err(|e| RedfishError::GenericError {
                error: format!("rune {name}: {e}"),
            })?;
        interpret::<T>(value, name)
    }
}

// Per-method dispatch generators (script if defined, else `self.s`). The macros own
// the `'a` lifetime. Entries are separated by `;` so return types can contain commas.
macro_rules! dispatch_noarg {
    ($($name:ident -> $ret:ty);* $(;)?) => {$(
        fn $name<'a>(&'a self) -> RedfishFuture<'a, Result<$ret, RedfishError>> {
            if self.has(stringify!($name)) {
                Box::pin(async move {
                    self.call::<_, $ret>(stringify!($name), (self.ctx(),)).await
                })
            } else {
                self.s.$name()
            }
        }
    )*};
}

macro_rules! dispatch_noarg_boxed {
    ($($name:ident -> $ret:ty);* $(;)?) => {$(
        fn $name<'a>(&'a self) -> RedfishFuture<'a, Result<$ret, RedfishError>> {
            if self.has(stringify!($name)) {
                Box::pin(async move {
                    self.call::<_, $ret>(stringify!($name), (self.ctx(),)).await
                })
            } else {
                Box::pin(self.s.$name())
            }
        }
    )*};
}

macro_rules! dispatch_str {
    ($($name:ident ( $($arg:ident),+ ) -> $ret:ty);* $(;)?) => {$(
        fn $name<'a>(&'a self $(, $arg: &'a str)+) -> RedfishFuture<'a, Result<$ret, RedfishError>> {
            if self.has(stringify!($name)) {
                Box::pin(async move {
                    self.call::<_, $ret>(stringify!($name), (self.ctx(), $($arg.to_string()),+)).await
                })
            } else {
                self.s.$name($($arg),+)
            }
        }
    )*};
}

/// The Redfish `BootSourceOverrideTarget` string for a `Boot` value.
fn boot_target_str(target: Boot) -> &'static str {
    match target {
        Boot::Pxe => "Pxe",
        Boot::HardDisk => "Hdd",
        Boot::UefiHttp => "UefiHttp",
    }
}

impl Redfish for Bmc {
    dispatch_noarg! {
        get_accounts -> Vec<ManagerAccount>;
        get_software_inventories -> Vec<String>;
        get_tasks -> Vec<String>;
        get_power_state -> PowerState;
        get_service_root -> ServiceRoot;
        get_systems -> Vec<String>;
        get_system -> ComputerSystem;
        get_managers -> Vec<String>;
        get_manager -> Manager;
        get_secure_boot -> SecureBoot;
        disable_secure_boot -> ();
        enable_secure_boot -> ();
        bmc_reset -> ();
        bmc_reset_to_defaults -> ();
        get_system_event_log -> Vec<LogEntry>;
        set_machine_password_policy -> ();
        setup_serial_console -> ();
        clear_tpm -> ();
        pcie_devices -> Vec<PCIeDevice>;
        bios -> HashMap<String, serde_json::Value>;
        reset_bios -> ();
        pending -> HashMap<String, serde_json::Value>;
        clear_pending -> ();
        get_chassis_all -> Vec<String>;
        get_manager_ethernet_interfaces -> Vec<String>;
        get_system_ethernet_interfaces -> Vec<String>;
        get_update_service -> UpdateService;
        get_base_mac_address -> Option<String>;
        is_ipmi_over_lan_enabled -> bool;
        enable_rshim_bmc -> ();
        clear_nvram -> ();
        get_nic_mode -> Option<NicMode>;
        enable_infinite_boot -> ();
        is_infinite_boot_enabled -> Option<bool>;
        get_host_rshim -> Option<EnabledDisabled>;
        get_boss_controller -> Option<String>;
        get_component_integrities -> ComponentIntegrities;
        set_utc_timezone -> ();
    }

    dispatch_noarg_boxed! {
        get_power_metrics -> Power;
        get_thermal_metrics -> Thermal;
        get_drives_metrics -> Vec<Drives>;
        get_boot_options -> BootOptions;
    }

    dispatch_str! {
        delete_user(username) -> ();
        get_firmware(id) -> SoftwareInventory;
        get_task(id) -> Task;
        get_secure_boot_certificate(database_id, certificate_id) -> Certificate;
        get_secure_boot_certificates(database_id) -> Vec<String>;
        add_secure_boot_certificate(pem_cert, database_id) -> Task;
        get_boot_option(option_id) -> BootOption;
        get_network_device_functions(chassis_id) -> Vec<String>;
        get_chassis(id) -> Chassis;
        get_chassis_assembly(chassis_id) -> Assembly;
        get_chassis_network_adapters(chassis_id) -> Vec<String>;
        get_chassis_network_adapter(chassis_id, id) -> NetworkAdapter;
        get_base_network_adapters(system_id) -> Vec<String>;
        get_base_network_adapter(system_id, id) -> NetworkAdapter;
        get_ports(chassis_id, network_adapter) -> Vec<String>;
        get_port(chassis_id, network_adapter, id) -> NetworkPort;
        get_manager_ethernet_interface(id) -> EthernetInterface;
        get_system_ethernet_interface(id) -> EthernetInterface;
        change_username(old_name, new_name) -> ();
        change_password(username, new_pass) -> ();
        change_password_by_id(account_id, new_pass) -> ();
        change_uefi_password(current_uefi_password, new_uefi_password) -> Option<String>;
        clear_uefi_password(current_uefi_password) -> Option<String>;
        get_job_state(job_id) -> JobState;
        get_firmware_for_component(component_integrity_id) -> SoftwareInventory;
        get_component_ca_certificate(url) -> CaCertificate;
        trigger_evidence_collection(url, nonce) -> Task;
        get_evidence(url) -> Evidence;
        decommission_storage_controller(controller_id) -> Option<String>;
        create_storage_volume(controller_id, volume_name) -> Option<String>;
    }

    // dispatched (enum arg marshaled as a string)
    fn power<'a>(
        &'a self,
        action: SystemPowerControl,
    ) -> RedfishFuture<'a, Result<(), RedfishError>> {
        if self.has("power") {
            Box::pin(async move {
                self.call::<_, ()>("power", (self.ctx(), action.to_string()))
                    .await
            })
        } else {
            self.s.power(action)
        }
    }

    // return types without `Deserialize` can't use the script return bridge, so delegate
    fn get_gpu_sensors<'a>(&'a self) -> RedfishFuture<'a, Result<Vec<GPUSensors>, RedfishError>> {
        self.s.get_gpu_sensors()
    }

    fn lockdown_status<'a>(&'a self) -> RedfishFuture<'a, Result<Status, RedfishError>> {
        self.s.lockdown_status()
    }

    fn serial_console_status<'a>(&'a self) -> RedfishFuture<'a, Result<Status, RedfishError>> {
        self.s.serial_console_status()
    }

    // Complex or vendor args delegate to standard (scriptable later).
    fn create_user<'a>(
        &'a self,
        username: &'a str,
        password: &'a str,
        role_id: RoleId,
    ) -> RedfishFuture<'a, Result<(), RedfishError>> {
        self.s.create_user(username, password, role_id)
    }

    fn chassis_reset<'a>(
        &'a self,
        chassis_id: &'a str,
        reset_type: SystemPowerControl,
    ) -> RedfishFuture<'a, Result<(), RedfishError>> {
        self.s.chassis_reset(chassis_id, reset_type)
    }

    fn get_bmc_event_log<'a>(
        &'a self,
        from: Option<chrono::DateTime<chrono::Utc>>,
    ) -> RedfishFuture<'a, Result<Vec<LogEntry>, RedfishError>> {
        self.s.get_bmc_event_log(from)
    }

    fn machine_setup<'a>(
        &'a self,
        boot_interface: Option<crate::BootInterfaceRef<'a>>,
        bios_profiles: &'a BiosProfileVendor,
        selected_profile: BiosProfileType,
        oem_manager_profiles: &'a BiosProfileVendor,
    ) -> RedfishFuture<'a, Result<Option<String>, RedfishError>> {
        if self.has("machine_setup") {
            Box::pin(async move {
                self.call::<_, Option<String>>("machine_setup", (self.ctx(),))
                    .await
            })
        } else {
            self.s.machine_setup(
                boot_interface,
                bios_profiles,
                selected_profile,
                oem_manager_profiles,
            )
        }
    }

    fn machine_setup_status<'a>(
        &'a self,
        boot_interface: Option<crate::BootInterfaceRef<'a>>,
    ) -> RedfishFuture<'a, Result<MachineSetupStatus, RedfishError>> {
        self.s.machine_setup_status(boot_interface)
    }

    fn is_bios_setup<'a>(
        &'a self,
        boot_interface: Option<crate::BootInterfaceRef<'a>>,
    ) -> RedfishFuture<'a, Result<bool, RedfishError>> {
        if self.has("is_bios_setup") {
            Box::pin(async move { self.call::<_, bool>("is_bios_setup", (self.ctx(),)).await })
        } else {
            self.s.is_bios_setup(boot_interface)
        }
    }

    fn lockdown<'a>(
        &'a self,
        target: EnabledDisabled,
    ) -> RedfishFuture<'a, Result<(), RedfishError>> {
        self.s.lockdown(target)
    }

    fn boot_once<'a>(&'a self, target: Boot) -> RedfishFuture<'a, Result<(), RedfishError>> {
        if self.has("boot_once") {
            let t = boot_target_str(target).to_string();
            Box::pin(async move { self.call::<_, ()>("boot_once", (self.ctx(), t)).await })
        } else {
            self.s.boot_once(target)
        }
    }

    fn boot_first<'a>(&'a self, target: Boot) -> RedfishFuture<'a, Result<(), RedfishError>> {
        if self.has("boot_first") {
            let t = boot_target_str(target).to_string();
            Box::pin(async move { self.call::<_, ()>("boot_first", (self.ctx(), t)).await })
        } else {
            self.s.boot_first(target)
        }
    }

    fn set_boot_override<'a>(
        &'a self,
        settings: BootOverride,
    ) -> RedfishFuture<'a, Result<Option<String>, RedfishError>> {
        if self.has("set_boot_override") {
            let target = settings.target.to_string();
            let enabled = settings.enabled.to_string();
            let mode = settings.mode.as_ref().map(|m| m.to_string());
            let uri = settings.http_boot_uri.clone();
            Box::pin(async move {
                self.call::<_, Option<String>>(
                    "set_boot_override",
                    (self.ctx(), target, enabled, mode, uri),
                )
                .await
            })
        } else {
            self.s.set_boot_override(settings)
        }
    }

    fn change_boot_order<'a>(
        &'a self,
        boot_array: Vec<String>,
    ) -> RedfishFuture<'a, Result<(), RedfishError>> {
        self.s.change_boot_order(boot_array)
    }

    fn set_ntp_servers<'a>(
        &'a self,
        servers: &'a [String],
    ) -> RedfishFuture<'a, Result<(), RedfishError>> {
        self.s.set_ntp_servers(servers)
    }

    fn update_firmware<'a>(
        &'a self,
        filename: tokio::fs::File,
    ) -> RedfishFuture<'a, Result<Task, RedfishError>> {
        self.s.update_firmware(filename)
    }

    fn update_firmware_multipart<'a>(
        &'a self,
        firmware: &'a Path,
        reboot: bool,
        timeout: Duration,
        component_type: ComponentType,
    ) -> RedfishFuture<'a, Result<String, RedfishError>> {
        self.s
            .update_firmware_multipart(firmware, reboot, timeout, component_type)
    }

    fn update_firmware_simple_update<'a>(
        &'a self,
        image_uri: &'a str,
        targets: Vec<String>,
        transfer_protocol: TransferProtocolType,
    ) -> RedfishFuture<'a, Result<Task, RedfishError>> {
        self.s
            .update_firmware_simple_update(image_uri, targets, transfer_protocol)
    }

    fn set_bios<'a>(
        &'a self,
        values: HashMap<String, serde_json::Value>,
    ) -> RedfishFuture<'a, Result<(), RedfishError>> {
        self.s.set_bios(values)
    }

    fn get_network_device_function<'a>(
        &'a self,
        chassis_id: &'a str,
        id: &'a str,
        port: Option<&'a str>,
    ) -> RedfishFuture<'a, Result<NetworkDeviceFunction, RedfishError>> {
        self.s.get_network_device_function(chassis_id, id, port)
    }

    fn get_resource<'a>(
        &'a self,
        id: ODataId,
    ) -> RedfishFuture<'a, Result<Resource, RedfishError>> {
        self.s.get_resource(id)
    }

    fn get_collection<'a>(
        &'a self,
        id: ODataId,
    ) -> RedfishFuture<'a, Result<Collection, RedfishError>> {
        self.s.get_collection(id)
    }

    fn set_boot_order_dpu_first<'a>(
        &'a self,
        boot_interface: crate::BootInterfaceRef<'a>,
    ) -> RedfishFuture<'a, Result<Option<String>, RedfishError>> {
        if self.has("set_boot_order_dpu_first") {
            Box::pin(async move {
                self.call::<_, Option<String>>("set_boot_order_dpu_first", (self.ctx(),))
                    .await
            })
        } else {
            self.s.set_boot_order_dpu_first(boot_interface)
        }
    }

    fn lockdown_bmc<'a>(
        &'a self,
        target: EnabledDisabled,
    ) -> RedfishFuture<'a, Result<(), RedfishError>> {
        self.s.lockdown_bmc(target)
    }

    fn enable_ipmi_over_lan<'a>(
        &'a self,
        target: EnabledDisabled,
    ) -> RedfishFuture<'a, Result<(), RedfishError>> {
        self.s.enable_ipmi_over_lan(target)
    }

    fn set_nic_mode<'a>(&'a self, mode: NicMode) -> RedfishFuture<'a, Result<(), RedfishError>> {
        self.s.set_nic_mode(mode)
    }

    fn set_host_rshim<'a>(
        &'a self,
        enabled: EnabledDisabled,
    ) -> RedfishFuture<'a, Result<(), RedfishError>> {
        self.s.set_host_rshim(enabled)
    }

    fn set_idrac_lockdown<'a>(
        &'a self,
        enabled: EnabledDisabled,
    ) -> RedfishFuture<'a, Result<(), RedfishError>> {
        self.s.set_idrac_lockdown(enabled)
    }

    fn is_boot_order_setup<'a>(
        &'a self,
        boot_interface: crate::BootInterfaceRef<'a>,
    ) -> RedfishFuture<'a, Result<bool, RedfishError>> {
        if self.has("is_boot_order_setup") {
            Box::pin(async move {
                self.call::<_, bool>("is_boot_order_setup", (self.ctx(),))
                    .await
            })
        } else {
            self.s.is_boot_order_setup(boot_interface)
        }
    }

    fn set_host_privilege_level<'a>(
        &'a self,
        level: HostPrivilegeLevel,
    ) -> RedfishFuture<'a, Result<(), RedfishError>> {
        self.s.set_host_privilege_level(level)
    }

    fn ac_powercycle_supported_by_power(&self) -> bool {
        self.s.ac_powercycle_supported_by_power()
    }
}

#[cfg(test)]
mod test {
    use super::{
        compile, do_b64_decode, interpret, response_json, shared_runtime, RedfishCtx,
        RedfishHttpClient,
    };
    use reqwest::header::HeaderMap;
    use reqwest::StatusCode;
    use rune::runtime::Value;
    use rune::Vm;

    const STUB: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/rune/http_stub.rn");

    // Run a no-arg async fn from the committed stub and return its raw rune Value.
    async fn run(name: &str) -> Value {
        let unit = compile(STUB).unwrap();
        Vm::new(shared_runtime().unwrap(), unit)
            .send_execute([name], ())
            .unwrap()
            .async_complete()
            .await
            .into_result()
            .unwrap()
    }

    // Compile a script, run an async fn, and bridge the result back through serde_json.
    #[tokio::test]
    async fn script_runs_and_result_bridges() {
        let path = std::env::temp_dir().join("libredfish_rune_ok.rn");
        std::fs::write(&path, "pub async fn answer() { 42 }").unwrap();
        let unit = compile(path.to_str().unwrap()).unwrap();
        let runtime = shared_runtime().unwrap();
        let value = Vm::new(runtime, unit)
            .send_execute(["answer"], ())
            .unwrap()
            .async_complete()
            .await
            .into_result()
            .unwrap();
        let n: i64 = serde_json::from_value(serde_json::to_value(&value).unwrap()).unwrap();
        assert_eq!(n, 42);
    }

    #[test]
    fn compile_error_is_file_error() {
        let path = std::env::temp_dir().join("libredfish_rune_bad.rn");
        std::fs::write(&path, "pub async fn x( {").unwrap();
        let err = compile(path.to_str().unwrap()).unwrap_err();
        assert!(
            matches!(err, crate::RedfishError::FileError(_)),
            "expected FileError, got {err:?}"
        );
    }

    #[test]
    fn json_value_bridge_roundtrips() {
        let j = serde_json::json!({"PowerState":"On","n":3,"list":[1,2],"nil":null});
        let v: Value = serde_json::from_value(j.clone()).unwrap();
        let back = serde_json::to_value(&v).unwrap();
        assert_eq!(j, back);
    }

    // Unit returns (from every dispatched method that yields `()`) must bridge.
    #[tokio::test]
    async fn unit_return_bridges() {
        let path = std::env::temp_dir().join("libredfish_rune_unit.rn");
        std::fs::write(&path, "pub async fn nothing() { () }").unwrap();
        let unit = compile(path.to_str().unwrap()).unwrap();
        let value = Vm::new(shared_runtime().unwrap(), unit)
            .send_execute(["nothing"], ())
            .unwrap()
            .async_complete()
            .await
            .into_result()
            .unwrap();
        let _: () = serde_json::from_value(serde_json::to_value(&value).unwrap()).unwrap();
    }

    // A script `Err(..)` (say from `?` on a failed call) becomes a RedfishError.
    #[tokio::test]
    async fn script_err_becomes_redfish_error() {
        let v = run("returns_err").await;
        let r = interpret::<()>(v, "returns_err");
        assert!(
            matches!(r, Err(crate::RedfishError::GenericError { .. })),
            "expected GenericError, got {r:?}"
        );
    }

    // A top level `Ok(v)` is unwrapped before bridging.
    #[tokio::test]
    async fn script_ok_is_unwrapped() {
        let v = run("returns_ok").await;
        let n: i64 = interpret::<i64>(v, "returns_ok").unwrap();
        assert_eq!(n, 42);
    }

    // A bare return that isn't a Result bridges directly (backward compatible).
    #[tokio::test]
    async fn bare_value_passes_through() {
        let v = run("returns_bare").await;
        let n: i64 = interpret::<i64>(v, "returns_bare").unwrap();
        assert_eq!(n, 42);
    }

    // The committed stub's HTTP functions must compile against the host module.
    #[test]
    fn stub_http_functions_compile() {
        let unit = compile(STUB).unwrap();
        let rt = shared_runtime().unwrap();
        assert!(Vm::new(rt.clone(), unit.clone())
            .lookup_function(["power_state"])
            .is_ok());
        assert!(Vm::new(rt, unit)
            .lookup_function(["reset_and_wait"])
            .is_ok());
    }

    // The response object handed to scripts has status, headers, and body.
    #[test]
    fn response_json_shape() {
        let mut h = HeaderMap::new();
        h.insert(
            "location",
            "/redfish/v1/TaskService/Tasks/3".parse().unwrap(),
        );
        let body = Some(serde_json::json!({ "PowerState": "On" }));
        let j = response_json(StatusCode::ACCEPTED, Some(h), body);
        assert_eq!(j["status"], 202);
        assert_eq!(j["headers"]["location"], "/redfish/v1/TaskService/Tasks/3");
        assert_eq!(j["body"]["PowerState"], "On");
    }

    // The free helpers (sha/b64/json/read_*/unix_time) register and resolve when called bare,
    // and `bmc_address` rides on `ctx`. Fully offline: sha2/base64/json are pure; read_file
    // hits a temp file and read_env a uniquely-named var set here.
    #[tokio::test]
    async fn host_helpers_register_and_run() {
        std::env::set_var("LIBREDFISH_RUNE_TEST_ENVVAR", "present");
        let file_path = std::env::temp_dir().join("libredfish_rune_readfile.txt");
        std::fs::write(&file_path, "hello-from-file").unwrap();

        let endpoint = crate::Endpoint {
            host: "bmc.example".to_string(),
            port: None,
            user: None,
            password: None,
        };
        let client = RedfishHttpClient::new(reqwest::Client::new(), endpoint, Vec::new());
        let ctx = RedfishCtx {
            client,
            system_id: "1".to_string(),
            manager_id: "1".to_string(),
            variant: None,
        };

        let script = r#"pub async fn probe(ctx, file_path, env_present, env_missing) {
    let decoded_b64 = match b64_decode("YWJj") { Ok(t) => t, Err(e) => e };
    let obj = match json_decode("{\"PowerState\":\"On\",\"n\":3}") { Ok(v) => v, Err(_) => #{} };
    let file = match read_file(file_path) { Ok(t) => t, Err(e) => e };
    let encoded = match json_encode(#{ "a": 1 }) { Ok(s) => s, Err(e) => e };
    #{
        "addr": ctx.bmc_address(),
        "sha256_abc": sha256("abc"),
        "sha512_abc": sha512("abc"),
        "b64_abc": b64_encode("abc"),
        "b64_roundtrip": decoded_b64,
        "json_power": obj["PowerState"],
        "json_n": obj["n"],
        "json_encoded": encoded,
        "file": file,
        "env_present": read_env(env_present),
        "env_missing": read_env(env_missing),
        "unix_time": unix_time()
    }
}"#;
        let path = std::env::temp_dir().join("libredfish_rune_host_helpers.rn");
        std::fs::write(&path, script).unwrap();

        let unit = compile(path.to_str().unwrap()).unwrap();
        let value = Vm::new(shared_runtime().unwrap(), unit)
            .send_execute(
                ["probe"],
                (
                    ctx,
                    file_path.to_string_lossy().to_string(),
                    "LIBREDFISH_RUNE_TEST_ENVVAR".to_string(),
                    "LIBREDFISH_RUNE_DEFINITELY_UNSET_9f3b".to_string(),
                ),
            )
            .unwrap()
            .async_complete()
            .await
            .into_result()
            .unwrap();
        let out: serde_json::Value = serde_json::to_value(&value).unwrap();

        assert_eq!(out["addr"], "bmc.example");
        // NIST SHA-256/512("abc") vectors — proves the digest, not just registration.
        assert_eq!(
            out["sha256_abc"],
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            out["sha512_abc"],
            "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"
        );
        assert_eq!(out["b64_abc"], "YWJj");
        assert_eq!(out["b64_roundtrip"], "abc");
        assert_eq!(out["json_power"], "On");
        assert_eq!(out["json_n"], 3);
        assert_eq!(out["json_encoded"], "{\"a\":1}");
        assert_eq!(out["file"], "hello-from-file");
        assert_eq!(out["env_present"], "present");
        assert!(out["env_missing"].is_null());
        assert!(
            out["unix_time"].as_i64().unwrap() > 1_700_000_000,
            "unix_time should be a recent epoch second, got {:?}",
            out["unix_time"]
        );
    }

    // `b64_decode` round-trips valid input and reports an error on non-base64 (the Err the
    // script can `match`/`?`).
    #[test]
    fn b64_decode_roundtrips_and_rejects_invalid() {
        assert_eq!(do_b64_decode("YWJj").unwrap(), "abc");
        assert!(do_b64_decode("*** not base64 ***").is_err());
    }
}
