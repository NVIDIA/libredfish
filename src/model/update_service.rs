use serde::{Deserialize, Serialize};

/// https://redfish.dmtf.org/schemas/v1/UpdateService.v1_14_0.json
/// Service for Software Update
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase", default)]
pub struct UpdateService {
    pub http_push_uri: String,
    pub max_image_size_bytes: i32,
    pub multipart_http_push_uri: String,
}
