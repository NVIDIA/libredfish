//! Optional vendor override from a local JSON file.
//!
//! When `LIBREDFISH_VENDOR_OVERRIDE_FILE` is set, it points to a JSON file pinning
//! the Redfish vendor (and optional `variant`/`script`) per endpoint, keyed by
//! remote address and optionally manager id. It is consulted in [`crate::network`]
//! before auto-detection, so it wins over both an explicit and a detected vendor.
//!
//! Inert when unset; if set but the file is missing/unreadable/malformed, client
//! creation fails with [`RedfishError::FileError`] (fail-closed).
//!
//! File format (`vendor` is a [`RedfishVendor`] variant; `manager`/`variant`/`script`
//! optional):
//!
//! ```json
//! [
//!   { "addr": "10.42.0.5", "vendor": "Rune", "script": "/etc/bmc.rn", "variant": "model-x" },
//!   { "addr": "10.42.0.6", "manager": "1", "vendor": "Dell" }
//! ]
//! ```

use serde::Deserialize;

use crate::model::service_root::RedfishVendor;
use crate::RedfishError;

/// Environment variable holding the path to the vendor-override JSON file.
pub(crate) const ENV: &str = "LIBREDFISH_VENDOR_OVERRIDE_FILE";

#[derive(Debug, Deserialize)]
struct Entry {
    /// BMC remote address; matched against `Endpoint::host`.
    addr: String,
    /// Optional manager id; when present, the entry only matches that manager.
    #[serde(default)]
    manager: Option<String>,
    vendor: RedfishVendor,
    /// Optional free-form discriminator handed to the vendor implementation.
    #[serde(default)]
    variant: Option<String>,
    /// Optional path to a Rune script implementing the vendor (used by `Rune`).
    #[serde(default)]
    script: Option<String>,
}

/// A matched vendor override: the forced vendor plus an optional variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VendorOverride {
    pub vendor: RedfishVendor,
    pub variant: Option<String>,
    pub script: Option<String>,
}

fn parse(contents: &str) -> Result<Vec<Entry>, serde_json::Error> {
    serde_json::from_str(contents)
}

/// Pick the override for `host`/`manager_id`. An entry naming both the address
/// and the manager wins over an address-only entry (which acts as the
/// any-manager default for that address).
fn select(entries: &[Entry], host: &str, manager_id: &str) -> Option<VendorOverride> {
    entries
        .iter()
        .find(|e| e.addr == host && e.manager.as_deref() == Some(manager_id))
        .or_else(|| {
            entries
                .iter()
                .find(|e| e.addr == host && e.manager.is_none())
        })
        .map(|e| VendorOverride {
            vendor: e.vendor,
            variant: e.variant.clone(),
            script: e.script.clone(),
        })
}

/// Resolve a vendor override for the given endpoint.
///
/// Returns `Ok(None)` when the env var is unset or no entry matches; returns
/// `Err(FileError)` when the env var is set but the file cannot be read or
/// parsed (fail-closed).
pub(crate) fn resolve(
    host: &str,
    manager_id: &str,
) -> Result<Option<VendorOverride>, RedfishError> {
    let Ok(path) = std::env::var(ENV) else {
        return Ok(None);
    };
    let contents = std::fs::read_to_string(&path)
        .map_err(|e| RedfishError::FileError(format!("vendor override {path}: {e}")))?;
    let entries = parse(&contents)
        .map_err(|e| RedfishError::FileError(format!("vendor override {path}: {e}")))?;
    Ok(select(&entries, host, manager_id))
}

#[cfg(test)]
mod test {
    use super::*;

    const JSON: &str = r#"[
        { "addr": "10.42.0.5", "vendor": "Rune", "variant": "model-x" },
        { "addr": "10.42.0.6", "manager": "1", "vendor": "Dell" },
        { "addr": "10.42.0.6", "vendor": "NvidiaDpu" }
    ]"#;

    fn entries() -> Vec<Entry> {
        parse(JSON).unwrap()
    }

    #[test]
    fn addr_only_match_returns_vendor_and_variant() {
        let m = select(&entries(), "10.42.0.5", "1").unwrap();
        assert_eq!(m.vendor, RedfishVendor::Rune);
        assert_eq!(m.variant.as_deref(), Some("model-x"));
    }

    #[test]
    fn addr_plus_manager_beats_addr_only() {
        // 10.42.0.6 has a manager-"1" entry (Dell) and an addr-only entry (NvidiaDpu).
        let with_mgr = select(&entries(), "10.42.0.6", "1").unwrap();
        assert_eq!(with_mgr.vendor, RedfishVendor::Dell);
        assert_eq!(with_mgr.variant, None);

        // A different manager falls back to the addr-only default.
        let other_mgr = select(&entries(), "10.42.0.6", "2").unwrap();
        assert_eq!(other_mgr.vendor, RedfishVendor::NvidiaDpu);
    }

    #[test]
    fn no_match_returns_none() {
        assert!(select(&entries(), "10.0.0.99", "1").is_none());
    }

    #[test]
    fn malformed_or_unknown_vendor_errors() {
        assert!(parse("{ not json").is_err());
        // An unknown RedfishVendor variant name also fails to deserialize.
        assert!(parse(r#"[{"addr":"x","vendor":"Nope"}]"#).is_err());
    }
}
