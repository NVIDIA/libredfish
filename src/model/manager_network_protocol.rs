use serde::{Deserialize, Serialize};

use crate::model::ODataLinks;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct Protocol {
    pub port: Option<i64>,
    pub protocol_enabled: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct ManagerNetworkProtocol {
    #[serde(flatten)]
    pub odata: ODataLinks,
    pub name: Option<String>,
    #[serde(rename = "DHCP")]
    pub dhcp: Option<Protocol>,
    #[serde(rename = "DHCPv6")]
    pub dhcpv6: Option<Protocol>,
    pub description: Option<String>,
    #[serde(rename = "FQDN")]
    pub fqdn: Option<String>,
    #[serde(rename = "HTTP")]
    pub http: Option<Protocol>,
    pub host_name: Option<String>,
    #[serde(rename = "IPMI")]
    pub ipmi: Option<Protocol>,
    pub id: Option<String>,
    #[serde(rename = "KVMIP")]
    pub kvmip: Option<Protocol>,
    pub rdp: Option<Protocol>,
    #[serde(rename = "RFB")]
    pub rfb: Option<Protocol>,
    pub ssh: Option<Protocol>,
    #[serde(rename = "SNMP")]
    pub snmp: Option<Protocol>,
    pub telnet: Option<Protocol>,
    pub virtual_media: Option<Protocol>,
}
