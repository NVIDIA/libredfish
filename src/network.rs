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
use std::{collections::HashMap, fs::File, time::Duration};
use std::string::{String, ToString};
use std::vec::Vec;

use reqwest::{
    header::{HeaderName, HeaderValue, ACCEPT, CONTENT_TYPE},
    Client as HttpClient, ClientBuilder as HttpClientBuilder, Method, StatusCode,
};
use serde::{de::DeserializeOwned, Serialize};
use tracing::debug;

use crate::model::InvalidValueError;
pub use crate::RedfishError;
use crate::{standard::RedfishStandard, Redfish};

pub const REDFISH_ENDPOINT: &str = "redfish/v1";
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(20);
const MIN_UPLOAD_BANDWIDTH: u64 = 10_000;

#[derive(Debug)]
pub struct RedfishClientPoolBuilder {
    timeout: Duration,
    accept_invalid_certs: bool,
}

impl RedfishClientPoolBuilder {
    /// Prevents the Redfish Client from accepting self signed certificates
    /// and other invalid certificates.
    ///
    /// By default self signed certificates will be accepted, since BMCs usually
    /// use those.
    pub fn reject_invalid_certs(mut self) -> RedfishClientPoolBuilder {
        self.accept_invalid_certs = false;
        self
    }

    /// Overwrites the timeout that will be applied to every request
    pub fn timeout(mut self, timeout: Duration) -> RedfishClientPoolBuilder {
        self.timeout = timeout;
        self
    }

    /// Builds a Redfish Client Network Configuration
    pub fn build(&self) -> Result<RedfishClientPool, RedfishError> {
        let builder = HttpClientBuilder::new();
        let http_client = builder
            .danger_accept_invalid_certs(self.accept_invalid_certs)
            .timeout(self.timeout)
            .build()
            .unwrap();
        let pool = RedfishClientPool { http_client };

        Ok(pool)
    }
}

/// The endpoint that the redfish client connects to
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    /// Hostname or IP address of BMC
    pub host: String,
    /// BMC port. If absent the default HTTPS port 443 will be used
    pub port: Option<u16>,
    /// BMC username
    pub user: Option<String>,
    /// BMC password
    pub password: Option<String>,
}

impl Default for Endpoint {
    fn default() -> Self {
        Endpoint {
            host: "".to_string(),
            port: None,
            user: None,
            password: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RedfishClientPool {
    http_client: HttpClient,
}

impl RedfishClientPool {
    /// Returns Builder for configuring a Redfish HTTP connection pool
    pub fn builder() -> RedfishClientPoolBuilder {
        RedfishClientPoolBuilder {
            timeout: DEFAULT_TIMEOUT,
            // BMCs often have a self-signed cert, so usually this has to be true
            accept_invalid_certs: true,
        }
    }

    /// Creates a Redfish BMC client for a certain endpoint
    ///
    /// Creating the client will immediately start a HTTP requests
    /// to set system_id, manager_id and vendor type.
    pub async fn create_client(
        &self,
        endpoint: Endpoint,
    ) -> Result<Box<dyn crate::Redfish>, RedfishError> {
        let client = RedfishHttpClient::new(self.http_client.clone(), endpoint);
        let mut s = RedfishStandard::new(client)?;
        let service_root = s.get_service_root().await?;
        let systems = s.get_systems().await?;
        let managers = s.get_managers().await?;
        let system_id = systems.first().unwrap();
        let manager_id = managers.first().unwrap();
        s.set_system_id(system_id)?;
        s.set_manager_id(manager_id)?;
        // call set_system_id always before calling set_vendor
        s.set_vendor(&service_root.vendor().unwrap_or("".to_string()))
    }

    /// Creates a Redfish BMC client for a certain endpoint
    ///
    /// Creating the standard client will not start any HTTP calls.
    pub fn create_standard_client(
        &self,
        endpoint: Endpoint,
    ) -> Result<Box<RedfishStandard>, RedfishError> {
        let client = RedfishHttpClient::new(self.http_client.clone(), endpoint);
        let s = RedfishStandard::new(client)?;
        Ok(Box::new(s))
    }
}

/// A HTTP client which targets a single libredfish endpoint
#[derive(Clone)]
pub struct RedfishHttpClient {
    endpoint: Endpoint,
    http_client: HttpClient,
}

impl RedfishHttpClient {
    pub fn new(http_client: HttpClient, endpoint: Endpoint) -> Self {
        Self {
            endpoint,
            http_client,
        }
    }

    pub async fn get<T>(&self, api: &str) -> Result<(StatusCode, T), RedfishError>
    where
        T: DeserializeOwned + ::std::fmt::Debug,
    {
        let (status_code, resp_opt) = self
            .req::<T, String>(Method::GET, api, None, None, None, Vec::new())
            .await?;
        match resp_opt {
            Some(response_body) => Ok((status_code, response_body)),
            None => Err(RedfishError::NoContent),
        }
    }

    pub async fn post(
        &self,
        api: &str,
        data: HashMap<&str, String>,
    ) -> Result<StatusCode, RedfishError> {
        let (status_code, _resp_body): (_, Option<HashMap<String, serde_json::Value>>) = self
            .req(Method::POST, api, Some(data), None, None, Vec::new())
            .await?;
        Ok(status_code)
    }

    pub async fn post_file<T>(
        &self,
        api: &str,
        file: tokio::fs::File,
    ) -> Result<(StatusCode, T), RedfishError>
    where
        T: DeserializeOwned + ::std::fmt::Debug,
    {
        let body_option: Option<HashMap<&str, String>> = None;
        let timeout = DEFAULT_TIMEOUT
            + file.metadata().await.map_or_else(
                |_err| DEFAULT_TIMEOUT,
                |m| Duration::from_secs(m.len() / MIN_UPLOAD_BANDWIDTH),
            );
        let (status_code, resp_opt) = self
            .req::<T, _>(
                Method::POST,
                api,
                body_option,
                Some(timeout),
                Some(file),
                Vec::new(),
            )
            .await?;
        match resp_opt {
            Some(response_body) => Ok((status_code, response_body)),
            None => Err(RedfishError::NoContent),
        }
    }

    pub async fn patch<T>(&self, api: &str, data: T) -> Result<StatusCode, RedfishError>
    where
        T: Serialize + ::std::fmt::Debug,
    {
        let (status_code, _resp_body): (_, Option<HashMap<String, serde_json::Value>>) = self
            .req(Method::PATCH, api, Some(data), None, None, Vec::new())
            .await?;
        Ok(status_code)
    }

    // Various parts of Redfish do use DELETE, but we don't implement any of those yet,
    // hence allow dead_code.
    #[allow(dead_code)]
    pub async fn delete(&self, api: &str) -> Result<StatusCode, RedfishError> {
        let (status_code, _resp_body): (_, Option<HashMap<String, serde_json::Value>>) = self
            .req::<_, String>(Method::DELETE, api, None, None, None, Vec::new())
            .await?;
        Ok(status_code)
    }

    // All the HTTP requests happen from here.
    pub async fn req<T, B>(
        &self,
        method: Method,
        api: &str,
        body: Option<B>,
        override_timeout: Option<Duration>,
        file: Option<tokio::fs::File>,
        custom_headers: Vec<(HeaderName, String)>,
    ) -> Result<(StatusCode, Option<T>), RedfishError>
    where
        T: DeserializeOwned + ::std::fmt::Debug,
        B: Serialize + ::std::fmt::Debug,
    {
        let url = match self.endpoint.port {
            Some(p) => format!(
                "https://{}:{}/{}/{}",
                self.endpoint.host, p, REDFISH_ENDPOINT, api
            ),
            None => format!(
                "https://{}/{}/{}",
                self.endpoint.host, REDFISH_ENDPOINT, api
            ),
        };
        let body_enc = match body {
            Some(b) => {
                let url: String = url.clone();
                let body_enc =
                    serde_json::to_string(&b).map_err(|e| RedfishError::JsonSerializeError {
                        url,
                        object_debug: format!("{b:?}"),
                        source: e,
                    })?;
                Some(body_enc)
            }
            None => None,
        };
        debug!(
            "TX {} {} {}",
            method,
            url,
            body_enc.as_deref().unwrap_or_default()
        );

        let mut req_b = match method {
            Method::GET => self.http_client.get(&url),
            Method::POST => self.http_client.post(&url),
            Method::PATCH => self.http_client.patch(&url),
            Method::DELETE => self.http_client.delete(&url),
            _ => unreachable!("Only GET, POST, PATCH and DELETE http methods are used."),
        };
        req_b = req_b.header(ACCEPT, HeaderValue::from_static("application/json"));

        if file.is_some() {
            req_b = req_b.header(
                CONTENT_TYPE,
                HeaderValue::from_static("application/octet-stream"),
            );
        } else {
            req_b = req_b.header(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        }

        for (key, val) in custom_headers.iter() {
            let value = match HeaderValue::from_str(val) {
                Ok(x) => x,
                Err(e) => {
                    return Err(RedfishError::InvalidValue {
                        url: url.to_string(),
                        field: "0".to_string(),
                        err: InvalidValueError(format!(
                            "Invalid custom header {} value: {}, error: {}",
                            key, val, e
                        )),
                    })
                }
            };
            req_b = req_b.header(key, value);
        }

        if let Some(user) = &self.endpoint.user {
            req_b = req_b.basic_auth(user, self.endpoint.password.as_ref());
        }
        if let Some(t) = override_timeout {
            req_b = req_b.timeout(t);
        }
        if let Some(b) = body_enc {
            req_b = req_b.body(b);
        }
        if let Some(f) = file {
            req_b = req_b.body(f);
        }
        let response = req_b.send().await.map_err(|e| RedfishError::NetworkError {
            url: url.clone(),
            source: e,
        })?;
        let status_code = response.status();
        if status_code == StatusCode::CONFLICT {
            // 409 No Content is how Dell responds if we try to turn off a system that's already off, etc.
            // Note that Lenovo accepts these unnecessary operations and returns '204 No Content'.
            return Err(RedfishError::UnnecessaryOperation);
        }
        // read the body even if not status 2XX, because BMCs give useful error messages as JSON
        let response_body = response
            .text()
            .await
            .map_err(|e| RedfishError::NetworkError {
                url: url.clone(),
                source: e,
            })?;
        debug!("RX {status_code} {}", truncate(&response_body, 1500));

        if !status_code.is_success() {
            return Err(RedfishError::HTTPErrorCode {
                url,
                status_code,
                response_body,
            });
        }

        let mut res = None;
        if !response_body.is_empty() {
            match serde_json::from_str(&response_body) {
                Ok(v) => res.insert(v),
                Err(e) => {
                    return Err(RedfishError::JsonDeserializeError {
                        url,
                        body: response_body,
                        source: e,
                    });
                }
            };
        }
        Ok((status_code, res))
    }
}

fn truncate(s: &str, len: usize) -> &str {
    &s[..len.min(s.len())]
}

#[test]
fn test_truncate() {
    assert_eq!(truncate("", 1500), "");

    let big = "a".repeat(2000);
    assert_eq!(truncate(&big, 1500).len(), 1500);
}
