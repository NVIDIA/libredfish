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
use std::{any::type_name, collections::HashMap};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{value::RawValue, Value};
use tracing::debug;

use crate::{jsonmap, Chassis, RedfishError};

// A Resource is a single entity accessed at a specific URI. A resource collection is a
// set of resources that share the same schema definition. Both are defined in Redfish Spec to have the
// following propeties that we must capture.
//
//                  for Resource        for Resource-Collection
// @odata.id   -    mandatory           mandatory
// @odata.type -    mandatory           mandatory
// @odata.etag -    mandatory           optional
// @odata.context   optional            optional
//
// OData structure will capture all those 4 properties.
#[derive(Debug, Default, Serialize, Deserialize, Clone, Eq)]
pub struct OData {
    // Registry resources in a response may include an @odata.id property. All other resources and resource
    // collections in a response shall include an @odata.id property. The value of the identifier property shall
    // be the resource URI.
    #[serde(rename = "@odata.id")]
    pub odata_id: String,
    // All resources and resource collections in a response shall include an @odata.type type property. To
    // support generic OData clients, all structured properties in a response should include an @odata.type
    // type property
    #[serde(rename = "@odata.type")]
    pub odata_type: String,
    // ETags enable clients to conditionally retrieve or update a resource. Resources should include an
    // @odata.etag property.
    #[serde(rename = "@odata.etag")]
    pub odata_etag: Option<String>,
    // Responses for resources and resource collections may contain an @odata.context property that
    // describes the source of the payload
    #[serde(rename = "@odata.context")]
    pub odata_context: Option<String>,
}

impl PartialEq for OData {
    fn eq(&self, other: &OData) -> bool {
        self.odata_id == other.odata_id
    }
}

// This trait is used as a bound (constraint) in generic definitions
// Macros are provided to implement it.
pub trait IsResource {
    fn odata_id(&self) -> String;
    fn odata_type(&self) -> String;
}

// This is captures raw json of any resource
// This is what get_resource() returns
#[derive(Debug, Clone)]
pub struct Resource {
    pub url: String,
    pub raw: Box<RawValue>,
}

impl Resource {
    // Attemps to deserialize raw JSON to requested type T after first verifying that
    // @odata.type of the resource is same as T.
    pub fn try_get<U: DeserializeOwned + IsResource>(self) -> Result<U, RedfishError> {
        let requested_type = type_name::<U>().split("::").last().unwrap_or("unknown");
        let v = match serde_json::from_str::<Value>(self.raw.get()) {
            Ok(x) => x,
            Err(e) => {
                return Err(RedfishError::JsonDeserializeError {
                    url: self.url,
                    body: self.raw.get().to_string(),
                    source: e,
                })
            }
        };

        let odata_type: String;
        let resource_type = match v.get("@odata.type") {
            Some(x) => {
                odata_type = x.to_string();
                x.to_string()
                    .split('.')
                    .next_back()
                    .unwrap_or_default()
                    .to_string()
                    .replace('"', "")
            }
            None => {
                return Err(RedfishError::MissingKey {
                    key: "@odata.type".to_string(),
                    url: self.url,
                })
            }
        };
        if resource_type == requested_type {
            let res: U = match serde_json::from_str(self.raw.get()) {
                Ok(x) => x,
                Err(e) => {
                    debug!("try_get: from_str failed: expected Type >{}< resource tye {}. Err {}, str = {} ",
                             type_name::<U>().split("::").last().unwrap_or_default(), resource_type, e.to_string(), self.raw.get().to_string());
                    return Err(RedfishError::JsonDeserializeError {
                        url: self.url,
                        body: self.raw.get().to_string(),
                        source: e,
                    });
                }
            };
            return Ok(res);
        }
        Err(RedfishError::TypeMismatch {
            expected: requested_type.to_string(),
            actual: resource_type,
            resource_type: odata_type,
            resource_uri: self.url,
        })
    }
}

// Custom deserializer for Resource. Captures the raw JSON
impl<'de> serde::Deserialize<'de> for Resource {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use serde::de::Error;
        match Box::deserialize(deserializer) {
            Ok(x) => Ok(Resource {
                url: "".to_string(),
                raw: x,
            }),
            Err(e) => {
                debug!("Deserialize::deserialize error {}", e.to_string());
                Err(D::Error::custom(e))
            }
        }
    }
}

// This is captures raw json of any resource collection as a HashMap.
// This is what get_collection() returns
#[derive(Debug, Clone)]
pub struct Collection {
    pub url: String,
    pub body: HashMap<String, serde_json::Value>,
}

impl Collection {
    // Attempts to desrialize raw JSON to ResourceCollection<T>
    // try_get verifies types do match.
    // Make sure that all mandatory properties of redfish resource collections are present
    // First will attempt to deserialize members json into Vec<T>. If it fails we will try
    // deserializing individually.
    pub fn try_get<T: DeserializeOwned + IsResource>(
        mut self,
    ) -> Result<ResourceCollection<T>, RedfishError> {
        let otype: String = jsonmap::extract(&mut self.body, "@odata.type", &self.url)?;
        // Make sure that we have a collection.
        if !otype.ends_with("Collection") {
            return Err(RedfishError::TypeMismatch {
                expected: "Collection".to_string(),
                actual: otype,
                resource_type: "".to_string(),
                resource_uri: self.url,
            });
        }
        let members_json_value = self
            .body
            .remove("Members")
            .ok_or(RedfishError::MissingKey {
                key: "Members".to_string(),
                url: self.url.clone(),
            })?;

        let name = jsonmap::extract(&mut self.body, "Name", &self.url)?;
        let count = jsonmap::extract(&mut self.body, "Members@odata.count", &self.url)?;
        let id = jsonmap::extract(&mut self.body, "@odata.id", &self.url)?;
        let etag = jsonmap::extract(&mut self.body, "@odata.etag", &self.url).unwrap_or_default();
        let context =
            jsonmap::extract(&mut self.body, "@odata.context", &self.url).unwrap_or_default();
        let description =
            jsonmap::extract(&mut self.body, "Description", &self.url).unwrap_or_default();

        let odata = OData {
            odata_id: id,
            odata_type: otype,
            odata_etag: etag,
            odata_context: context,
        };

        debug!("json >{}<", members_json_value.to_string());
        let expected_type_name = type_name::<T>()
            .split("::")
            .last()
            .unwrap_or("unknown")
            .to_string();
        let actual_type = odata
            .odata_type
            .split('.')
            .next_back()
            .unwrap_or_default()
            .replace("Collection", "");
        if expected_type_name == actual_type {
            let mut collection = ResourceCollection::<T> {
                odata: odata.clone(),
                name,
                count,
                failed_to_deserialize_count: 0,
                description,
                members: vec![],
            };
            match serde_json::from_value::<Vec<T>>(members_json_value.clone()) {
                Ok(x) => {
                    collection.members = x;
                    Ok(collection)
                }
                Err(e) => {
                    debug!("collection deserialization failed for type {}. Error: {}. Attempting individually", type_name::<T>(), e.to_string() );
                    if !members_json_value.is_array() {
                        return Err(RedfishError::GenericError {
                            error: format!("json value is not an Array. {}", members_json_value),
                        });
                    }
                    let array = match members_json_value.as_array() {
                        Some(a) => a,
                        None => {
                            return Err(RedfishError::GenericError {
                                error: format!("json value array is none {}", members_json_value),
                            });
                        }
                    };
                    for a in array {
                        let res = serde_json::from_value::<T>(a.to_owned());
                        match res {
                            Err(e) => {
                                debug!(
                                    "Failed to deserialize to {}. json: {}, error: {}",
                                    type_name::<T>(),
                                    a.to_owned(),
                                    e.to_string()
                                );
                                collection.failed_to_deserialize_count += 1;
                                continue;
                            }
                            Ok(r) => {
                                debug!("{} found {}", type_name::<T>(), r.odata_id());
                                collection.members.push(r);
                            }
                        }
                    }

                    if collection.members.is_empty() && collection.count != 0 {
                        // we failed to serialize any; return error
                        return Err(RedfishError::JsonDeserializeError {
                            url: odata.odata_id.to_string(),
                            body: members_json_value.to_string(),
                            source: e,
                        });
                    }
                    Ok(collection)
                }
            }
        } else {
            Err(RedfishError::TypeMismatch {
                expected: expected_type_name,
                actual: actual_type,
                resource_type: odata.odata_type,
                resource_uri: odata.odata_id,
            })
        }
    }
}

// This represents a typed resource collection as defined by redfish spec.
pub struct ResourceCollection<T>
where
    T: DeserializeOwned,
{
    pub odata: super::OData,
    pub name: String,
    pub count: i32,
    pub description: Option<String>,
    pub members: Vec<T>,
    // This tells how many members we fail to deserialize
    // This is not a Redfish property
    pub failed_to_deserialize_count: i32,
}

// Macro to implement IsResource
#[macro_export]
macro_rules! impl_is_resource {
    ($t:ty) => {
        impl IsResource for $t {
            fn odata_id(&self) -> String {
                self.odata.odata_id.clone()
            }
            fn odata_type(&self) -> String {
                self.odata.odata_type.clone()
            }
        }
    };
}
pub use impl_is_resource;

// Macro to implement IsResource
macro_rules! impl_is_resource_for_option_odatalinks {
    ($t:ty) => {
        impl IsResource for $t {
            fn odata_id(&self) -> String {
                match self.odata.clone() {
                    Some(x) => x.odata_id,
                    None => "".to_string(),
                }
            }
            fn odata_type(&self) -> String {
                match self.odata.clone() {
                    Some(x) => x.odata_type,
                    None => "".to_string(),
                }
            }
        }
    };
}

impl_is_resource_for_option_odatalinks!(Chassis);
impl_is_resource_for_option_odatalinks!(crate::NetworkDeviceFunction);
impl_is_resource_for_option_odatalinks!(crate::EthernetInterface);

impl_is_resource!(crate::model::PCIeDevice);
impl_is_resource!(crate::model::PCIeFunction);
impl_is_resource!(crate::model::ComputerSystem);
impl_is_resource!(crate::NetworkAdapter);
impl_is_resource!(crate::model::sensor::Sensor);
impl_is_resource!(crate::model::Manager);
impl_is_resource!(crate::model::BootOption);
impl_is_resource!(crate::model::account_service::ManagerAccount);
impl_is_resource!(crate::model::storage::Storage);
