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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum TransferProtocolType {
    FTP,
    SFTP,
    HTTP,
    HTTPS,
    SCP,
    TFTP,
    OEM,
    NFS,
}

#[derive(Debug, clap::ValueEnum, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub enum ComponentType {
    BMC,
    UEFI,
    EROTBMC,
    EROTBIOS,
    CPLMID,
    CPLDMB,
    #[clap(skip)]
    PSU {
        num: u32,
    },
    #[clap(skip)]
    PCIeSwitch {
        num: u32,
    },
    #[clap(skip)]
    PCIeRetimer {
        num: u32,
    },
    HGXBMC,
    #[clap(skip)]
    Unknown,
}
