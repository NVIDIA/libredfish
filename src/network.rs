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
use std::{collections::HashMap, path::Path, time::Duration};

use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue, ACCEPT, CONTENT_TYPE, IF_MATCH},
    multipart::{Form, Part},
    Client as HttpClient, ClientBuilder as HttpClientBuilder, Method, Proxy, StatusCode,
};
use serde::{de::DeserializeOwned, Serialize};
use tracing::debug;

use crate::model::service_root::RedfishVendor;
use crate::{model::InvalidValueError, standard::RedfishStandard, Redfish, RedfishError};

pub const REDFISH_ENDPOINT: &str = "redfish/v1";
const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
const MIN_UPLOAD_BANDWIDTH: u64 = 10_000;

#[derive(Debug)]
pub struct RedfishClientPoolBuilder {
    connect_timeout: Duration,
    timeout: Duration,
    accept_invalid_certs: bool,
    proxy: Option<String>,
}

impl RedfishClientPoolBuilder {
    /// Prevents the Redfish Client from accepting self signed certificates
    /// and other invalid certificates.
    ///
    /// By default self signed certificates will be accepted, since BMCs usually
    /// use those.
    pub fn reject_invalid_certs(mut self) -> Self {
        self.accept_invalid_certs = false;
        self
    }

    /// Overwrites the timeout for establishing a connection
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Overwrites the timeout that will be applied to every request
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    pub fn proxy(mut self, proxy: Option<String>) -> Self {
        self.proxy = proxy;
        self
    }

    /// Builds a Redfish Client Network Configuration
    pub fn build(&self) -> Result<RedfishClientPool, RedfishError> {
        let mut builder = HttpClientBuilder::new();
        if let Some(proxy) = self.proxy.as_ref() {
            let p = Proxy::https(proxy)?;
            builder = builder.proxy(p);
        }

        let http_client = builder
            .danger_accept_invalid_certs(self.accept_invalid_certs)
            .connect_timeout(self.connect_timeout)
            .timeout(self.timeout)
            .build()
            .map_err(|e| RedfishError::GenericError {
                error: format!("Failed to build RedfishClientPool HTTP client: {}", e),
            })?;
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
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            timeout: DEFAULT_TIMEOUT,
            // BMCs often have a self-signed cert, so usually this has to be true
            accept_invalid_certs: true,
            proxy: None,
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
        self.create_client_with_custom_headers(endpoint, Vec::default())
            .await
    }

    /// Creates a Redfish BMC client for a certain endpoint and adds custom headers to subsequent requests.
    ///
    /// Creating the client will immediately start a HTTP requests
    /// to set system_id, manager_id and vendor type.
    /// `custom_headers` will be added to any headers used by vendor specific implementations or the http client.
    pub async fn create_client_with_custom_headers(
        &self,
        endpoint: Endpoint,
        custom_headers: Vec<(HeaderName, String)>,
    ) -> Result<Box<dyn crate::Redfish>, RedfishError> {
        let client = RedfishHttpClient::new(self.http_client.clone(), endpoint, custom_headers);
        let mut s = RedfishStandard::new(client);
        let service_root = s.get_service_root().await?;
        let systems = s.get_systems().await?;
        let managers = s.get_managers().await?;
        let system_id = systems.first().ok_or_else(|| RedfishError::GenericError {
            error: "No systems found in service root".to_string(),
        })?;
        let manager_id = managers.first().ok_or_else(|| RedfishError::GenericError {
            error: "No managers found in service root".to_string(),
        })?;
        let chassis = s.get_chassis_all().await?;

        s.set_system_id(system_id)?;
        // call set_system_id always before calling set_vendor
        s.set_manager_id(manager_id)?;
        s.set_service_root(service_root.clone())?;

        let Some(mut vendor) = service_root.vendor() else {
            return Err(RedfishError::MissingVendor);
        };
        if vendor == RedfishVendor::P3809 {
            if chassis.contains(&"MGX_NVSwitch_0".to_string()) {
                vendor = RedfishVendor::NvidiaGBSwitch;
            } else {
                vendor = RedfishVendor::NvidiaGH200;
            }
        }
        // returns the vendor specific object
        s.set_vendor(vendor)
    }

    /// Creates a Redfish BMC client for a certain endpoint
    ///
    /// Creating the standard client will not start any HTTP calls.
    pub fn create_standard_client(
        &self,
        endpoint: Endpoint,
    ) -> Result<Box<RedfishStandard>, RedfishError> {
        self.create_standard_client_with_custom_headers(endpoint, Vec::default())
    }

    /// Creates a Redfish BMC client for a certain endpoint, with custom headers injected into each request
    ///
    /// Creating the standard client will not start any HTTP calls.
    pub fn create_standard_client_with_custom_headers(
        &self,
        endpoint: Endpoint,
        custom_headers: Vec<(HeaderName, String)>,
    ) -> Result<Box<RedfishStandard>, RedfishError> {
        let client = RedfishHttpClient::new(self.http_client.clone(), endpoint, custom_headers);
        let s = RedfishStandard::new(client);
        Ok(Box::new(s))
    }
}

/// A HTTP client which targets a single libredfish endpoint
#[derive(Clone)]
pub struct RedfishHttpClient {
    endpoint: Endpoint,
    http_client: HttpClient,
    custom_headers: Vec<(HeaderName, String)>,
}

impl RedfishHttpClient {
    pub fn new(
        http_client: HttpClient,
        endpoint: Endpoint,
        custom_headers: Vec<(HeaderName, String)>,
    ) -> Self {
        Self {
            endpoint,
            http_client,
            custom_headers,
        }
    }

    pub async fn get<T>(&self, api: &str) -> Result<(StatusCode, T), RedfishError>
    where
        T: DeserializeOwned + ::std::fmt::Debug,
    {
        self.get_with_timeout(api, None).await
    }
    pub async fn get_with_timeout<T>(
        &self,
        api: &str,
        timeout: Option<Duration>,
    ) -> Result<(StatusCode, T), RedfishError>
    where
        T: DeserializeOwned + ::std::fmt::Debug,
    {
        let (status_code, resp_opt, _resp_headers) = self
            .req::<T, String>(Method::GET, api, None, timeout, None, Vec::new())
            .await?;
        match resp_opt {
            Some(response_body) => Ok((status_code, response_body)),
            None => Err(RedfishError::NoContent),
        }
    }
    pub async fn post<B>(
        &self,
        api: &str,
        data: B,
    ) -> Result<(StatusCode, Option<HeaderMap>), RedfishError>
    where
        B: Serialize + ::std::fmt::Debug,
    {
        self.post_with_headers(api, data, None).await
    }

    pub async fn post_with_headers<B>(
        &self,
        api: &str,
        data: B,
        headers: Option<Vec<(HeaderName, String)>>,
    ) -> Result<(StatusCode, Option<HeaderMap>), RedfishError>
    where
        B: Serialize + ::std::fmt::Debug,
    {
        let (status_code, _resp_body, resp_headers): (
            _,
            Option<HashMap<String, serde_json::Value>>,
            Option<HeaderMap>,
        ) = self
            .req(
                Method::POST,
                api,
                Some(data),
                None,
                None,
                headers.unwrap_or_default(),
            )
            .await?;
        Ok((status_code, resp_headers))
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
        let (status_code, resp_opt, _resp_headers) = self
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

    pub async fn patch<T>(
        &self,
        api: &str,
        data: T,
    ) -> Result<(StatusCode, Option<HeaderMap>), RedfishError>
    where
        T: Serialize + ::std::fmt::Debug,
    {
        let (status_code, _resp_body, resp_headers): (
            _,
            Option<HashMap<String, serde_json::Value>>,
            Option<HeaderMap>,
        ) = self
            .req(Method::PATCH, api, Some(data), None, None, Vec::new())
            .await?;
        Ok((status_code, resp_headers))
    }

    pub async fn patch_with_if_match<B>(&self, api: &str, data: B) -> Result<(), RedfishError>
    where
        B: Serialize + ::std::fmt::Debug,
    {
        let timeout = Duration::from_secs(60);
        let headers: Vec<(HeaderName, String)> = vec![(IF_MATCH, "*".to_string())];
        let (status_code, resp_body, _): (
            _,
            Option<HashMap<String, serde_json::Value>>,
            Option<HeaderMap>,
        ) = self
            .req(Method::PATCH, api, Some(data), Some(timeout), None, headers)
            .await?;
        match status_code {
            StatusCode::NO_CONTENT => Ok(()),
            _ => Err(RedfishError::HTTPErrorCode {
                url: api.to_string(),
                status_code,
                response_body: format!("{:?}", resp_body.unwrap_or_default()),
            }),
        }
    }

    pub async fn delete(&self, api: &str) -> Result<StatusCode, RedfishError> {
        let (status_code, _resp_body, _resp_headers): (
            _,
            Option<HashMap<String, serde_json::Value>>,
            Option<HeaderMap>,
        ) = self
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
        mut custom_headers: Vec<(HeaderName, String)>,
    ) -> Result<(StatusCode, Option<T>, Option<HeaderMap>), RedfishError>
    where
        T: DeserializeOwned + ::std::fmt::Debug,
        B: Serialize + ::std::fmt::Debug,
    {
        custom_headers.extend_from_slice(&self.custom_headers);

        let is_file = file.is_some();
        match self
            ._req(&method, api, &body, override_timeout, file, &custom_headers)
            .await
        {
            Ok(x) => Ok(x),
            // HPE sends RST in case same connection is reused. To avoid that let's retry.
            Err(a) if matches!(a, RedfishError::NetworkError { .. }) => {
                // Handling of post_file failure must be done manually. The seek is moved and we
                // can't reuse file by cloning. Clone shares read, writes and seek.
                if is_file {
                    return Err(a);
                }
                self._req(&method, api, &body, override_timeout, None, &custom_headers)
                    .await
            }
            Err(x) => Err(x),
        }
    }

    // All the HTTP requests happen from here.
    #[tracing::instrument(name = "libredfish::request", skip_all, fields(uri=api), level = tracing::Level::DEBUG)]
    async fn _req<T, B>(
        &self,
        method: &Method,
        api: &str,
        body: &Option<B>,
        override_timeout: Option<Duration>,
        file: Option<tokio::fs::File>,
        custom_headers: &[(HeaderName, String)],
    ) -> Result<(StatusCode, Option<T>, Option<HeaderMap>), RedfishError>
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
                    serde_json::to_string(b).map_err(|e| RedfishError::JsonSerializeError {
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
        let mut req_b = match *method {
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
        debug!("RX {status_code}");

        let mut res_headers = None;
        if !response.headers().is_empty() {
            res_headers = Some(response.headers().clone());
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
            if status_code == StatusCode::FORBIDDEN && !response_body.is_empty() {
                // If PasswordChangeRequired is in the response, return a PasswordChangeRequired error.
                if let Ok(err) = serde_json::from_str::<crate::model::error::Error>(&response_body)
                {
                    if err
                        .error
                        .extended
                        .iter()
                        // TODO(ajf) The actual message ID is specified in DTMF RedFish 9.5.11.2 so we
                        // should properly parse it into a type since the error may come from different
                        // MessageRegistries
                        .any(|ext| ext.message_id.ends_with("PasswordChangeRequired"))
                    {
                        return Err(RedfishError::PasswordChangeRequired);
                    }
                }
                // If we can't decode the error JSON, just return the normal HTTPErrorCode. Some
                // misbehaved BMCs will return an XHTML document for forbidden responses, for
                // instance.
            }
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

        Ok((status_code, res, res_headers))
    }

    // req_multipart_firmware_upload does a Redfish request for a multipart based firmware upload.
    pub async fn req_update_firmware_multipart(
        &self,
        filename: &Path,
        file: tokio::fs::File,
        parameters: String,
        api: &str,
        drop_redfish_url_part: bool,
        timeout: Duration,
    ) -> Result<(StatusCode, Option<String>, String), RedfishError> {
        let user = match &self.endpoint.user {
            Some(user) => user,
            None => return Err(RedfishError::NotSupported("User not specified".to_string())),
        };

        let basename = match Path::file_name(filename) {
            Some(x) => x.to_string_lossy().to_string(),
            None => {
                return Err(RedfishError::FileError("Bad filename".to_string()));
            }
        };

        // Some vendors, but not all, have a prefix at the start of the given endpoint.
        let api_str = api.to_string();
        let api = api_str.strip_prefix("/").unwrap_or(api);
        // Some (Lenovo, perhaps others) vendors have nonstandard endpoint names for multipart upload.
        let with_redfish_endpoint = if drop_redfish_url_part {
            api.to_string()
        } else {
            format!("{}/{}", REDFISH_ENDPOINT, api)
        };
        let url = match self.endpoint.port {
            Some(p) => format!(
                "https://{}:{}/{}",
                self.endpoint.host, p, with_redfish_endpoint
            ),
            None => format!("https://{}/{}", self.endpoint.host, with_redfish_endpoint),
        };

        let length = filename
            .metadata()
            .map_err(|e| RedfishError::FileError(e.to_string()))?
            .len();
        let response = self
            .http_client
            .post(url.clone())
            .timeout(timeout)
            .multipart(
                Form::new()
                    // The spec is for two parts to the form: UpdateParameters, which is JSON encoded metadata,
                    // and UpdateFile, which is the file itself.  Exact details of UpdateParameters end up being implementation specific.
                    .part(
                        "UpdateParameters",
                        reqwest::multipart::Part::text(parameters)
                            // mime_str_to_part parses the MIME type. Technically this is
                            // infallible for known MIME types, including application/json,
                            // but still check for an error instead of unwrapping.
                            .mime_str("application/json")
                            .map_err(|e| RedfishError::GenericError {
                                error: format!("Invalid MIME type 'application/json': {}", e),
                            })?,
                    )
                    .part(
                        "UpdateFile",
                        Part::stream_with_length(file, length)
                            // mime_str_to_part parses the MIME type. Technically this is
                            // infallible for known MIME types, including application/octet-stream,
                            // but still check for an error instead of unwrapping.
                            .mime_str("application/octet-stream")
                            .map_err(|e| RedfishError::GenericError {
                                error: format!(
                                    "Invalid MIME type 'application/octet-stream': {}",
                                    e
                                ),
                            })?
                            // Yes, the filename passed does matter for some reason, at least for Dells, and it has to be the basename.
                            .file_name(basename.clone()),
                    ),
            )
            .basic_auth(user, self.endpoint.password.as_ref())
            .send()
            .await
            .map_err(|e| RedfishError::NetworkError {
                url: url.to_string(),
                source: e,
            })?;

        let status_code = response.status();
        debug!("RX {status_code}");

        // Some (or all?) implementations will return the task ID in the Location header, with an empty body.
        let loc = response
            .headers()
            .get("Location")
            .map(|x| x.to_str().unwrap_or_default().to_string());

        // read the body even if not status 2XX, because BMCs give useful error messages as JSON
        let response_body = response
            .text()
            .await
            .map_err(|e| RedfishError::NetworkError {
                url: url.to_string(),
                source: e,
            })?;
        debug!("RX {status_code} {}", truncate(&response_body, 1500));

        if !status_code.is_success() {
            return Err(RedfishError::HTTPErrorCode {
                url: url.to_string(),
                status_code,
                response_body,
            });
        }

        Ok((status_code, loc, response_body))
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
