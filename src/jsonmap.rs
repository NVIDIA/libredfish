/*
 * SPDX-FileCopyrightText: Copyright (c) 2025 NVIDIA CORPORATION & AFFILIATES. All rights reserved.
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

// jsonmap.rs
// This provides helper functions for extracting values from JSON maps,
// which as you can imagine happens a lot in a Redfish library.

use std::any::type_name;
use std::collections::HashMap;

use serde::de::DeserializeOwned;

use crate::RedfishError;

// JsonMap is a trait that abstracts over serde_json::Map and HashMap,
// allowing us to write generic functions that work with both.
pub trait JsonMap {
    // get_value retrieves a reference to a JSON value by key.
    fn get_value(&self, key: &str) -> Option<&serde_json::Value>;

    // remove_value removes and returns a JSON value by key.
    fn remove_value(&mut self, key: &str) -> Option<serde_json::Value>;
}

// Implement JsonMap for JSON maps.
impl JsonMap for serde_json::Map<String, serde_json::Value> {
    fn get_value(&self, key: &str) -> Option<&serde_json::Value> {
        self.get(key)
    }

    fn remove_value(&mut self, key: &str) -> Option<serde_json::Value> {
        self.remove(key)
    }
}

// Implement JsonMap for HashMaps.
impl JsonMap for HashMap<String, serde_json::Value> {
    fn get_value(&self, key: &str) -> Option<&serde_json::Value> {
        self.get(key)
    }

    fn remove_value(&mut self, key: &str) -> Option<serde_json::Value> {
        self.remove(key)
    }
}

// missing_key_error creates a RedfishError::MissingKey error.
fn missing_key_error(key: &str, url: &str) -> RedfishError {
    RedfishError::MissingKey {
        key: key.to_string(),
        url: url.to_string(),
    }
}

// invalid_type_error creates a RedfishError::InvalidKeyType error.
fn invalid_type_error(key: &str, expected_type: &str, url: &str) -> RedfishError {
    RedfishError::InvalidKeyType {
        key: key.to_string(),
        expected_type: expected_type.to_string(),
        url: url.to_string(),
    }
}

// get_value retrieves a JSON value from a map, returning MissingKey
// error if the key is not found. This is useful for directly fetching
// a raw Value for further processing, or for callers who want to
// fetch a value and attempt to convert it to a specific type.
pub fn get_value<'a, M: JsonMap>(
    map: &'a M,
    key: &str,
    url: &str,
) -> Result<&'a serde_json::Value, RedfishError> {
    map.get_value(key)
        .ok_or_else(|| missing_key_error(key, url))
}

// get_str extracts a string value from a JSON map, returning appropriate
// errors if the key is missing or the value is not a string.
pub fn get_str<'a, M: JsonMap>(map: &'a M, key: &str, url: &str) -> Result<&'a str, RedfishError> {
    get_value(map, key, url)?
        .as_str()
        .ok_or_else(|| invalid_type_error(key, "string", url))
}

// get_object extracts an object (Map) from a JSON map, returning
// appropriate errors if the key is missing or the value is not an object.
pub fn get_object<'a, M: JsonMap>(
    map: &'a M,
    key: &str,
    url: &str,
) -> Result<&'a serde_json::Map<String, serde_json::Value>, RedfishError> {
    get_value(map, key, url)?
        .as_object()
        .ok_or_else(|| invalid_type_error(key, "object", url))
}

// get_bool extracts a boolean value from a JSON map, returning
// appropriate errors if the key is missing or the value is not a boolean.
pub fn get_bool<M: JsonMap>(map: &M, key: &str, url: &str) -> Result<bool, RedfishError> {
    get_value(map, key, url)?
        .as_bool()
        .ok_or_else(|| invalid_type_error(key, "boolean", url))
}

// get_i64 extracts an integer value from a JSON map, returning
// appropriate errors if the key is missing or the value is not an integer.
#[allow(dead_code)]
pub fn get_i64<M: JsonMap>(map: &M, key: &str, url: &str) -> Result<i64, RedfishError> {
    get_value(map, key, url)?
        .as_i64()
        .ok_or_else(|| invalid_type_error(key, "integer", url))
}

// get_f64 extracts a floating-point value from a JSON map, returning
// appropriate errors if the key is missing or the value is not a number.
#[allow(dead_code)]
pub fn get_f64<M: JsonMap>(map: &M, key: &str, url: &str) -> Result<f64, RedfishError> {
    get_value(map, key, url)?
        .as_f64()
        .ok_or_else(|| invalid_type_error(key, "number", url))
}

// extract removes a key from a map and deserializes the
// value to type T. Returns an error if the key is missing or
// deserialization fails.
pub fn extract<T, M: JsonMap>(map: &mut M, key: &str, url: &str) -> Result<T, RedfishError>
where
    T: DeserializeOwned,
{
    let json = map
        .remove_value(key)
        .ok_or_else(|| missing_key_error(key, url))?;
    serde_json::from_value::<T>(json).map_err(|_| invalid_type_error(key, type_name::<T>(), url))
}

// extract_object removes a key from a HashMap and returns it
// as a JSON Map. Returns an error if the key is missing or the
// value is not an object.
pub fn extract_object<M: JsonMap>(
    map: &mut M,
    key: &str,
    url: &str,
) -> Result<serde_json::Map<String, serde_json::Value>, RedfishError> {
    extract(map, key, url).map_err(|e| match e {
        RedfishError::InvalidKeyType { key, url, .. } => invalid_type_error(&key, "object", &url),
        e => e,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // test_get_str_success tests that get_str correctly extracts a string value.
    #[test]
    fn test_get_str_success() {
        let value = json!({
            "Name": "TestName",
            "Id": "123"
        });
        let map = value.as_object().unwrap();

        let result = get_str(map, "Name", "http://test/url");
        assert_eq!(result.unwrap(), "TestName");
    }

    // test_get_str_with_hashmap tests that get_str works with HashMap.
    #[test]
    fn test_get_str_with_hashmap() {
        let mut map: HashMap<String, serde_json::Value> = HashMap::new();
        map.insert("Name".to_string(), json!("TestName"));

        let result = get_str(&map, "Name", "http://test/url");
        assert_eq!(result.unwrap(), "TestName");
    }

    // test_get_str_missing_key tests that get_str returns MissingKey error when key doesn't exist.
    #[test]
    fn test_get_str_missing_key() {
        let value = json!({
            "Name": "TestName"
        });
        let map = value.as_object().unwrap();

        let result = get_str(map, "Missing", "http://test/url");
        assert!(matches!(result, Err(RedfishError::MissingKey { .. })));
    }

    // test_get_str_wrong_type tests that get_str returns InvalidKeyType when value is not a string.
    #[test]
    fn test_get_str_wrong_type() {
        let value = json!({
            "Count": 42
        });
        let map = value.as_object().unwrap();

        let result = get_str(map, "Count", "http://test/url");
        assert!(matches!(result, Err(RedfishError::InvalidKeyType { .. })));
    }

    // test_get_object_success tests that get_object correctly extracts an object.
    #[test]
    fn test_get_object_success() {
        let value = json!({
            "Nested": {
                "Inner": "value"
            }
        });
        let map = value.as_object().unwrap();

        let result = get_object(map, "Nested", "http://test/url");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap().get("Inner").unwrap().as_str().unwrap(),
            "value"
        );
    }

    // test_get_bool_success tests that get_bool correctly extracts a boolean value.
    #[test]
    fn test_get_bool_success() {
        let value = json!({
            "Enabled": true,
            "Disabled": false
        });
        let map = value.as_object().unwrap();

        assert_eq!(get_bool(map, "Enabled", "http://test/url").unwrap(), true);
        assert_eq!(get_bool(map, "Disabled", "http://test/url").unwrap(), false);
    }

    // test_get_i64_success tests that get_i64 correctly extracts an integer value.
    #[test]
    fn test_get_i64_success() {
        let value = json!({
            "Count": 42
        });
        let map = value.as_object().unwrap();

        assert_eq!(get_i64(map, "Count", "http://test/url").unwrap(), 42);
    }

    // test_extract_success tests that extract correctly extracts and
    // removes a value.
    #[test]
    fn test_extract_success() {
        let mut map: HashMap<String, serde_json::Value> = HashMap::new();
        map.insert("Name".to_string(), json!("TestName"));

        let result: Result<String, _> = extract(&mut map, "Name", "http://test/url");
        assert_eq!(result.unwrap(), "TestName");
        assert!(map.is_empty());
    }

    // test_extract_object_success tests extract_object with a HashMap.
    #[test]
    fn test_extract_object_success() {
        let mut map: HashMap<String, serde_json::Value> = HashMap::new();
        map.insert("Nested".to_string(), json!({"Inner": "value"}));

        let result = extract_object(&mut map, "Nested", "http://test/url");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap().get("Inner").unwrap().as_str().unwrap(),
            "value"
        );
        assert!(map.is_empty());
    }
}
