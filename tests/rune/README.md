# Rune vendor — what a script can use

The `rune` vendor (`src/rune_vendor.rs`) runs a [Rune](https://rune-rs.github.io)
script as a BMC backend. This is the reference for everything a script can call,
override, and rely on.

> **Trust:** scripts run with the host process's privileges, no sandbox, and no
> resource limits. `read_file`/`read_env` reach the real filesystem and
> environment. Only load scripts you trust.

## Selecting a script

Set `LIBREDFISH_VENDOR_OVERRIDE_FILE` to a JSON file that pins the `Rune` vendor
and points at the script, keyed by BMC address (and optionally manager id):

```json
[
  { "addr": "10.42.0.5", "vendor": "Rune", "script": "/etc/bmc.rn", "variant": "model-x" }
]
```

`variant` is optional free-form text the script reads via `ctx.variant()`.

## How a script hooks in

For each `Redfish` trait method, the vendor looks for a **top-level function with
the same name**. If the script defines it, it is called; otherwise the call falls
back to the standard Redfish implementation. So a script only implements what it
needs to change.

```rune
// Overrides get_power_state; everything else uses the standard behavior.
pub async fn get_power_state(ctx) {
    match ctx.get(`Systems/${ctx.system_id()}`).await {
        Ok(resp) => resp["body"]["PowerState"],
        Err(_) => "Unknown",
    }
}
```

Override functions should be `pub async fn` and take `ctx` first, followed by any
string arguments the method passes (see the tables below).

### Return & error conventions

A function's return value is bridged into the method's Rust return type via JSON,
so return shapes that match the type (a string for `PowerState`, an object for
`bios`, `()` for actions, `None`/a value for `Option`, etc.).

- `return Ok(v)` / a bare `v` → the method succeeds with `v`.
- `return Err(msg)`, or `?` on a failed call → the method fails with a
  `RedfishError` carrying `msg`.

## The `ctx` handle

`ctx` is the BMC handle passed to every override. Its methods need BMC state, so
they live on `ctx`.

| Call | Returns | Notes |
|------|---------|-------|
| `ctx.get(path).await` | `Ok(#{status, headers, body})` / `Err(msg)` | `path` is relative to `redfish/v1/`, no leading `/` |
| `ctx.post(path, body).await` | `Ok(#{...})` / `Err(msg)` | `body` is any value (encoded as JSON) |
| `ctx.patch(path, body).await` | `Ok(#{...})` / `Err(msg)` | |
| `ctx.delete(path).await` | `Ok(#{...})` / `Err(msg)` | |
| `ctx.system_id()` | `String` | first system id resolved at client creation |
| `ctx.manager_id()` | `String` | first manager id |
| `ctx.variant()` | `Option<String>` | the override file's `variant`, if any |
| `ctx.bmc_address()` | `String` | BMC host/IP (the override file's `addr` key) |

The response object is `#{ status: <int>, headers: #{..}, body: <json or ()> }`.
Header names are lowercased.

## Free functions (called directly, no `ctx`)

These are pure/host helpers, so they are plain functions — call them by name.

| Call | Returns | Notes |
|------|---------|-------|
| `sha256(text)` | `String` | lowercase-hex SHA-256 of the UTF-8 bytes |
| `sha512(text)` | `String` | lowercase-hex SHA-512 |
| `b64_encode(text)` | `String` | standard base64, padded |
| `b64_decode(text)` | `Ok(text)` / `Err(msg)` | errors on bad base64 or non-UTF-8 |
| `json_encode(value)` | `Ok(text)` / `Err(msg)` | serialize any value to JSON text |
| `json_decode(text)` | `Ok(value)` / `Err(msg)` | parse JSON text to a value |
| `read_file(path)` | `Ok(text)` / `Err(msg)` | read a file as UTF-8 (host privileges) |
| `read_env(name)` | `Option<String>` | env var value, or `None` if unset |
| `unix_time()` | `i64` | wall-clock seconds since the Unix epoch |

The ones returning `Ok/Err` can be matched or `?`-ed exactly like the HTTP verbs.

## Methods a script may override

Define a function with one of these names to take over that method. Arguments
shown are what the script receives (always `ctx` first).

### No arguments — `pub async fn name(ctx)`

```
get_accounts            get_software_inventories  get_tasks
get_power_state         get_service_root          get_systems
get_system              get_managers              get_manager
get_secure_boot         disable_secure_boot       enable_secure_boot
bmc_reset               bmc_reset_to_defaults     get_system_event_log
set_machine_password_policy  setup_serial_console clear_tpm
pcie_devices            bios                      reset_bios
pending                 clear_pending             get_chassis_all
get_manager_ethernet_interfaces  get_system_ethernet_interfaces
get_update_service      get_base_mac_address      is_ipmi_over_lan_enabled
enable_rshim_bmc        clear_nvram               get_nic_mode
enable_infinite_boot    is_infinite_boot_enabled  get_host_rshim
get_boss_controller     get_component_integrities set_utc_timezone
get_power_metrics       get_thermal_metrics       get_drives_metrics
get_boot_options
```

### String arguments — `pub async fn name(ctx, arg1, ...)`

| Function | Args |
|----------|------|
| `delete_user` | `username` |
| `get_firmware` | `id` |
| `get_task` | `id` |
| `get_secure_boot_certificate` | `database_id, certificate_id` |
| `get_secure_boot_certificates` | `database_id` |
| `add_secure_boot_certificate` | `pem_cert, database_id` |
| `get_boot_option` | `option_id` |
| `get_network_device_functions` | `chassis_id` |
| `get_chassis` | `id` |
| `get_chassis_assembly` | `chassis_id` |
| `get_chassis_network_adapters` | `chassis_id` |
| `get_chassis_network_adapter` | `chassis_id, id` |
| `get_base_network_adapters` | `system_id` |
| `get_base_network_adapter` | `system_id, id` |
| `get_ports` | `chassis_id, network_adapter` |
| `get_port` | `chassis_id, network_adapter, id` |
| `get_manager_ethernet_interface` | `id` |
| `get_system_ethernet_interface` | `id` |
| `change_username` | `old_name, new_name` |
| `change_password` | `username, new_pass` |
| `change_password_by_id` | `account_id, new_pass` |
| `change_uefi_password` | `current_uefi_password, new_uefi_password` |
| `clear_uefi_password` | `current_uefi_password` |
| `get_job_state` | `job_id` |
| `get_firmware_for_component` | `component_integrity_id` |
| `get_component_ca_certificate` | `url` |
| `trigger_evidence_collection` | `url, nonce` |
| `get_evidence` | `url` |
| `decommission_storage_controller` | `controller_id` |
| `create_storage_volume` | `controller_id, volume_name` |

### Enum/struct arguments marshaled as strings

| Function | Args (script side) |
|----------|--------------------|
| `power` | `action` — e.g. `"On"`, `"ForceOff"`, `"GracefulRestart"` |
| `boot_once` | `target` — `"Pxe"`, `"Hdd"`, or `"UefiHttp"` |
| `boot_first` | `target` — same set |
| `set_boot_override` | `target, enabled, mode, uri` (`mode`/`uri` may be `None`) |

### Extra Rust-only arguments are dropped (script gets just `ctx`)

`machine_setup`, `is_bios_setup`, `set_boot_order_dpu_first`, `is_boot_order_setup`.

### Always delegate — cannot be overridden from a script

These take non-`Deserialize`/complex arguments or return non-`Deserialize` types,
so they always run the standard implementation:

```
get_gpu_sensors        lockdown_status            serial_console_status
create_user            chassis_reset              get_bmc_event_log
machine_setup_status   lockdown                   change_boot_order
update_firmware        update_firmware_multipart  update_firmware_simple_update
set_bios               get_network_device_function get_resource
get_collection         lockdown_bmc               enable_ipmi_over_lan
set_nic_mode           set_host_rshim             set_idrac_lockdown
set_host_privilege_level   ac_powercycle_supported_by_power   set_ntp_servers
```

## Language & standard library

Scripts are ordinary Rune (0.14). Language: `let`, `if`/`else`, `match`, `for`,
`while`, `loop`, closures (`|x| ...`), `async`/`await`, the `?` operator, ranges
(`a..b`), template strings (`` `text ${expr}` ``), object literals
(`#{ key: value }`), vectors (`[1, 2]`), and tuples.

The tables below are the script-callable standard library from
`Context::with_default_modules()`. Instance methods are `value.method(...)`; free
functions/constructors are bare (`min(a, b)`, `String::new()`); macros end in `!`.

**Globals & macros**

- Free fns: `min(a,b)`, `max(a,b)`, `clone(x)`, `drop(x)`, `print(x)`, `println(x)`,
  `panic(msg)`, `range(a,b)`
- Macros: `format!`, `println!`, `print!`, `panic!`, `assert!`, `assert_eq!`,
  `stringify!`, `file!`, `line!`

**Iterators** (chain off `.iter()`, a range, or any iterable)

`map`, `filter`, `filter_map`, `flat_map`, `enumerate`, `chain`, `skip`, `take`,
`peekable`, `rev`, `fold`, `reduce`, `find`, `any`, `all`, `count`, `sum`,
`product`, `nth`, `next`, `collect::<Vec>()` / `collect::<VecDeque>()`

**String** — `len`, `is_empty`, `capacity`, `char_at`, `chars`, `bytes`, `lines`,
`get`, `contains`, `starts_with`, `ends_with`, `find`, `split`, `split_once`,
`split_str`, `trim`, `trim_end`, `replace`, `to_lowercase`, `to_uppercase`,
`push`, `push_str`, `clear`, `reserve`, `as_bytes`, `into_bytes`,
`is_char_boundary`, `parse::<i64>()`/`parse::<f64>()`/`parse::<char>()`; ctors
`String::new`/`with_capacity`/`from`/`from_utf8`.

**Vec / `[...]`** — `len`, `is_empty`, `capacity`, `push`, `pop`, `insert`,
`remove`, `get`, `clear`, `extend`, `resize`, `sort`, `sort_by`, `iter`; ctors
`Vec::new`/`with_capacity`; iterate `for x in v`.

**Object / `#{...}`** — `get`, `contains_key`, `remove`; index `obj["key"]`;
iterate `for (k, v) in obj`.

**Option** — `is_some`, `is_none`, `unwrap`, `unwrap_or`, `unwrap_or_else`,
`expect`, `map`, `and_then`, `ok_or`, `ok_or_else`, `take`, `transpose`, `iter`.

**Result** — `is_ok`, `is_err`, `ok`, `unwrap`, `unwrap_or`, `unwrap_or_else`,
`expect`, `map`, `and_then`; plus `?`.

**Numbers**

- int (i64/u64): `abs`, `signum`, `pow`, `min`, `max`, `to_float`, `to_string`,
  `parse`, `is_positive`, `is_negative`, `checked_add/sub/mul/div/rem`,
  `saturating_*`, `wrapping_*`
- f64: `abs`, `ceil`, `floor`, `round`, `sqrt`, `powi`, `powf`, `is_nan`,
  `is_finite`, `is_infinite`, `is_normal`, `is_subnormal`, `to::<i64>()`, `parse`
- operators `+ - * / %` and comparisons work via protocols

**char** — `is_alphabetic`, `is_alphanumeric`, `is_numeric`, `is_whitespace`,
`is_control`, `is_uppercase`, `is_lowercase`, `to_digit`, `to_i64`; ctor
`char::from_i64`.

**Tuple** — `len`, `is_empty`, `get`, `iter`.

**Bytes** — `len`, `is_empty`, `push`, `pop`, `insert`, `remove`, `first`, `last`,
`extend`, `extend_str`, `as_vec`, `into_vec`, `clear`, `capacity`, `reserve`;
ctors `Bytes::new`/`with_capacity`/`from_vec`.

**Collections** (under `std::collections` — need a `use` or full path)

- `HashMap`: `new`, `with_capacity`, `from_iter`, `insert`, `get`, `remove`,
  `contains_key`, `keys`, `values`, `iter`, `len`, `is_empty`, `clear`,
  `capacity`, `extend`
- `HashSet`: `new`, `with_capacity`, `from_iter`, `insert`, `remove`, `contains`,
  `union`, `intersection`, `difference`, `iter`, `len`, `is_empty`, `clear`,
  `capacity`, `extend`
- `VecDeque`: `new`, `with_capacity`, `from_iter`, `from::<Vec>`, `push_back`,
  `push_front`, `pop_back`, `pop_front`, `front`, `back`, `insert`, `remove`,
  `rotate_left`, `rotate_right`, `iter`, `len`, `reserve`, `extend`

**Async & misc** — `future::join(..)` to await several futures; `cmp::Ordering`
(`Less`/`Equal`/`Greater`); `mem::drop`.

**Boundary:** that is the whole of "pure computation." Rune's std has **no**
filesystem, network, process, clock, or environment access — a script reaches the
outside world only through the `ctx` methods and the free functions above, all
registered by libredfish.

See `http_stub.rn` in this directory for runnable HTTP examples.
