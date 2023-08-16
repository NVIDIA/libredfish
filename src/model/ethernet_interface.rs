use serde::{Deserialize, Serialize};

use super::{ODataId, ODataLinks, LinkStatus, ResourceStatus};


/// http://redfish.dmtf.org/schemas/v1/EthernetInterfaceCollection.json
/// The EthernetInterfaceCollection schema contains a collection of Ethernet interfaces instances.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct EthernetInterfaceCollection {
    #[serde(flatten)]
    pub odata: Option<ODataLinks>,
    #[serde(default)]
    pub members: Vec<ODataId>,
}

/// http://redfish.dmtf.org/schemas/v1/EthernetInterface.v1_6_0.json
/// The EthernetInterface schema contains an inventory of Ethernet interface components.  
/// This can include Network Device parameters such as current IP addresses, MAC address, link status, etc.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct EthernetInterface {
    #[serde(flatten)]
    pub odata: Option<ODataLinks>,
    #[serde(rename = "DHCPv4")]
    pub dhcpv4: Option<DHCPv4>,
    #[serde(rename = "DHCPv6")]
    pub description: Option<String>,
    pub dhcpv6: Option<DHCPv6>,
    #[serde(rename = "FQDN")]
    pub fqdn: Option<String>,
    pub host_name: Option<String>,
    #[serde(rename = "IPv4Addresses", skip_serializing_if = "Option::is_none")]
    pub ipv4_addresses: Option<Vec<IPv4Address>>,
    #[serde(rename = "IPv4StaticAddresses", default)]
    pub ipv4_static_addresses: Vec<ODataId>,
    #[serde(rename = "IPv6AddressPolicyTable", default)]
    pub ipv6_address_policy_table: Vec<ODataId>,
    #[serde(rename = "IPv6Addresses", default)]
    pub ipv6_addresses: Vec<IPv6Address>,
    #[serde(rename = "IPv6DefaultGateway")]
    pub ipv6_default_gateway: Option<String>,
    #[serde(rename = "IPv6StaticAddresses", default)]
    pub ipv6_static_addresses: Vec<ODataId>,
    pub id: Option<String>,
    pub interface_enabled: Option<bool>,
    pub link_status: Option<LinkStatus>,
    #[serde(rename = "MACAddress")]
    pub mac_address: Option<String>,
    #[serde(rename = "MTUSize")]
    pub mtu_size: Option<i32>,
    pub name: Option<String>,
    #[serde(default)]
    pub name_servers: Vec<String>,
    pub speed_mbps: Option<i32>,
    #[serde(default)]
    pub static_name_servers: Vec<ODataId>,
    pub status: Option<ResourceStatus>,
    #[serde(rename = "VLANs")]
    pub vlans: Option<ODataId>,
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum IPv4AddressOrigin {
    Static,
    DHCP,
    BOOTP,
    IPv4LinkLocal
}

#[derive(Debug, Serialize, Deserialize, Copy, Clone, Eq, PartialEq)]
pub enum IPv6AddressOrigin {
    Static,
    DHCPv6,
    LinkLocal,
    SLAAC
}

/// http://redfish.dmtf.org/schemas/v1/IPAddresses.v1_0_10.json
/// The IPAddresses schema contains an inventory of IP Address components.  
/// This can include IP Address parameters such as IP address, address origin, etc.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct IPv4Address {
    #[serde(flatten)]
    pub address: Option<String>,
    pub address_origin: Option<IPv4AddressOrigin>,
    pub gateway: Option<String>,
    pub subnet_mask: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct IPv6Address {
    #[serde(flatten)]
    pub address: Option<String>,
    pub address_origin: Option<IPv6AddressOrigin>,
    pub address_state: Option<ODataId>,
    pub prefix_length: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct DHCPv4 {
    #[serde(flatten)]
    #[serde(rename = "DHCPEnabled")]
    pub dhcp_enabled: Option<bool>,
    #[serde(rename = "UseDNSServers")]
    pub use_dns_servers: Option<bool>,
    pub use_domain_name: Option<bool>,
    #[serde(rename = "UseNTPServers")]
    pub use_ntp_servers: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "PascalCase")]
pub struct DHCPv6 {
    #[serde(flatten)]
    pub operating_mode: Option<String>,
    #[serde(rename = "UseDNSServers")]
    pub use_dns_servers: Option<bool>,
    pub use_domain_name: Option<bool>,
    #[serde(rename = "UseNTPServers")]
    pub use_ntp_servers: Option<bool>,
}