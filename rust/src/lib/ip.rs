// SPDX-License-Identifier: Apache-2.0

use std::fmt::Write;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::str::FromStr;

use serde::{self, Deserialize, Deserializer, Serialize};

use crate::{
    BaseInterface, DnsClientState, ErrorKind, MergedInterface,
    MptcpAddressFlag, NmstateError, RouteRuleEntry,
};

const AF_INET: u8 = 2;
const AF_INET6: u8 = 10;
const IPV4_ADDR_LEN: usize = 32;
const IPV6_ADDR_LEN: usize = 128;
const FOREVER: &str = "forever";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[non_exhaustive]
#[serde(deny_unknown_fields)]
struct InterfaceIp {
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub enabled: Option<bool>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub dhcp: Option<bool>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        default,
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub autoconf: Option<bool>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "dhcp-client-id"
    )]
    pub dhcp_client_id: Option<Dhcpv4ClientId>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "dhcp-duid")]
    pub dhcp_duid: Option<Dhcpv6Duid>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "address")]
    pub addresses: Option<Vec<InterfaceIpAddr>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "auto-dns",
        default,
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub auto_dns: Option<bool>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "auto-gateway",
        default,
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub auto_gateway: Option<bool>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "auto-routes",
        default,
        deserialize_with = "crate::deserializer::option_bool_or_string"
    )]
    pub auto_routes: Option<bool>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "auto-route-table-id",
        default,
        deserialize_with = "crate::deserializer::option_u32_or_string"
    )]
    pub auto_table_id: Option<u32>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "auto-route-metric",
        default,
        deserialize_with = "crate::deserializer::option_u32_or_string"
    )]
    pub auto_route_metric: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "addr-gen-mode")]
    pub addr_gen_mode: Option<Ipv6AddrGenMode>,
    #[serde(
        default = "default_allow_extra_address",
        skip_serializing,
        rename = "allow-extra-address"
    )]
    pub allow_extra_address: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "dhcp-send-hostname"
    )]
    pub dhcp_send_hostname: Option<bool>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        rename = "dhcp-custom-hostname"
    )]
    pub dhcp_custom_hostname: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(into = "InterfaceIp")]
#[non_exhaustive]
/// IPv4 configuration of interface.
/// Example YAML output of interface holding static IPv4:
/// ```yaml
/// ---
/// interfaces:
/// - name: eth1
///   state: up
///   mtu: 1500
///   ipv4:
///     address:
///     - ip: 192.0.2.252
///       prefix-length: 24
///     - ip: 192.0.2.251
///       prefix-length: 24
///     dhcp: false
///     enabled: true
/// ```
pub struct InterfaceIpv4 {
    /// Whether IPv4 stack is enabled. When set to false, all IPv4 address will
    /// be removed from this interface.
    pub enabled: bool,
    pub(crate) prop_list: Vec<&'static str>,
    /// Whether DHCPv4 is enabled.
    pub dhcp: Option<bool>,
    /// DHCPv4 client ID.
    /// Serialize and deserialize to/from `dhcp-client-id`.
    pub dhcp_client_id: Option<Dhcpv4ClientId>,
    /// IPv4 addresses.
    /// When applying with `None`, current IP address will be preserved.
    /// When applying with `Some(Vec::new())`, all IP address will be removed.
    /// When switch from DHCP on to off with `addresses` set to None or all
    /// `addresses` are dynamic, nmstate will convert current dynamic IP
    /// address to static.
    /// The IP addresses will apply to kernel with the same order specified
    /// which result the IP addresses after first one holding the `secondary`
    /// flag.
    pub addresses: Option<Vec<InterfaceIpAddr>>,
    /// Whether to apply DNS resolver information retrieved from DHCP server.
    /// Serialize and deserialize to/from `auto-dns`.
    /// If you want to append static DNS name server before DHCP provided
    /// servers, please set `auto-dns: true` explicitly a along with static
    /// DNS server.
    pub auto_dns: Option<bool>,
    /// Whether to set default gateway retrieved from DHCP server.
    /// Serialize and deserialize to/from `auto-gateway`.
    pub auto_gateway: Option<bool>,
    /// Whether to set routes(including default gateway) retrieved from DHCP
    /// server.
    /// Serialize and deserialize to/from `auto-routes`.
    pub auto_routes: Option<bool>,
    /// The route table ID used to hold routes(including default gateway)
    /// retrieved from DHCP server.
    /// If not defined, the main(254) will be used.
    /// Serialize and deserialize to/from `auto-table-id`.
    pub auto_table_id: Option<u32>,
    /// By default(true), nmstate verification process allows extra IP address
    /// found as long as desired IP address matched.
    /// When set to false, the verification process of nmstate do exact equal
    /// check on IP address.
    /// Ignore when serializing.
    /// Deserialize from `allow-extra-address`
    pub allow_extra_address: bool,
    /// Metric for routes retrieved from DHCP server.
    /// Only available for DHCPv4 enabled interface.
    /// Deserialize from `auto-route-metric`
    pub auto_route_metric: Option<u32>,
    /// Whether to include hostname in DHCP request.
    /// If the hostname is FQDN, the `Fully Qualified Domain Name (FQDN)`
    /// option(81) defined in RFC 4702 will be used.
    /// If the hostname is not FQDN, the `Host Name` option(12) defined in RFC
    /// 2132 will be used.
    /// If not defined, set to True when DHCPv4 enabled.
    /// Deserialize from `dhcp-send-hostname`
    pub dhcp_send_hostname: Option<bool>,
    /// Custom string to override hostname used for DHCP request.
    /// If the hostname is FQDN, the `Fully Qualified Domain Name (FQDN)`
    /// option(81) defined in RFC 4702 will be used.
    /// If the hostname is not FQDN, the `Host Name` option(12) defined in RFC
    /// 2132 will be used.
    /// If not defined, current non-dynamic hostname will be used.
    /// Deserialize from `dhcp-custom-hostname`
    pub dhcp_custom_hostname: Option<String>,
    pub(crate) dns: Option<DnsClientState>,
    pub(crate) rules: Option<Vec<RouteRuleEntry>>,
}

impl Default for InterfaceIpv4 {
    fn default() -> Self {
        Self {
            enabled: false,
            prop_list: Vec::new(),
            dhcp: None,
            dhcp_client_id: None,
            addresses: None,
            dns: None,
            rules: None,
            auto_dns: None,
            auto_gateway: None,
            auto_routes: None,
            auto_table_id: None,
            allow_extra_address: default_allow_extra_address(),
            auto_route_metric: None,
            dhcp_send_hostname: None,
            dhcp_custom_hostname: None,
        }
    }
}

impl InterfaceIpv4 {
    /// Create [InterfaceIpv4] with IP disabled.
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn is_auto(&self) -> bool {
        self.enabled && self.dhcp == Some(true)
    }

    pub fn is_static(&self) -> bool {
        self.enabled
            && !self.is_auto()
            && !self.addresses.as_deref().unwrap_or_default().is_empty()
    }

    pub(crate) fn merge_ip(&mut self, current: &Self) {
        if !self.prop_list.contains(&"enabled") {
            self.enabled = current.enabled;
        }
        if self.dhcp.is_none() && self.enabled {
            self.dhcp = current.dhcp;
        }
        // Normally, we expect backend to preserve configuration which not
        // mentioned in desire or all auto ip address, but when DHCP switch from
        // ON to OFF, the design of nmstate is expecting dynamic IP address goes
        // static. This should be done by top level code.
        if current.is_auto()
            && current.addresses.is_some()
            && self.enabled
            && !self.is_auto()
            && is_ip_addrs_none_or_all_auto(self.addresses.as_deref())
        {
            self.addresses = current.addresses.clone();
            if let Some(addrs) = self.addresses.as_mut() {
                addrs.as_mut_slice().iter_mut().for_each(|a| {
                    a.valid_life_time = None;
                    a.preferred_life_time = None;
                });
            }
        }
    }

    // Special action for generating merged state from desired and current.
    pub(crate) fn special_merge(&mut self, desired: &Self, current: &Self) {
        if !desired.prop_list.contains(&"enabled") {
            self.enabled = current.enabled;
        }
        if desired.dhcp.is_none() && self.enabled {
            self.dhcp = current.dhcp;
        }

        // Normally, we expect backend to preserve configuration which not
        // mentioned in desire, but when DHCP switch from ON to OFF, the design
        // of nmstate is expecting dynamic IP address goes static. This should
        // be done by top level code.
        if current.is_auto()
            && current.addresses.is_some()
            && self.enabled
            && !self.is_auto()
            && is_ip_addrs_none_or_all_auto(self.addresses.as_deref())
        {
            self.addresses = current.addresses.clone();
        }
        self.sanitize(false).ok();
    }

    // * Remove link-local address
    // * Set auto_dns, auto_gateway and auto_routes to true if DHCP enabled and
    //   those options is None
    // * Disable DHCP and remove address if enabled: false
    // * Remove auto IP address.
    // * Set DHCP options to None if DHCP is false
    // * Remove mptcp_flags is they are for query only
    pub(crate) fn sanitize(
        &mut self,
        is_desired: bool,
    ) -> Result<(), NmstateError> {
        if self.is_auto() {
            if self.auto_dns.is_none() {
                self.auto_dns = Some(true);
            }
            if self.auto_routes.is_none() {
                self.auto_routes = Some(true);
            }
            if self.auto_gateway.is_none() {
                self.auto_gateway = Some(true);
            }
            if let Some(addrs) = self.addresses.as_ref() {
                if is_desired {
                    for addr in addrs {
                        log::info!(
                            "Static addresses {} defined when dynamic \
                            IP is enabled",
                            addr
                        );
                    }
                }
            }
        } else if self.addresses.as_ref().map_or(false, Vec::is_empty)
            && self.enabled
        {
            // Empty address should equal to disabled IPv4 stack
            if is_desired {
                log::info!("Empty IPv4 address is considered as IPv4 disabled");
            }
            self.enabled = false;
        }

        if let Some(addrs) = self.addresses.as_mut() {
            if is_desired {
                for addr in addrs.as_slice().iter().filter(|a| a.is_auto()) {
                    log::info!("Ignoring Auto IP address {}", addr);
                }
                if let Some(addr) =
                    addrs.as_slice().iter().find(|a| a.ip.is_ipv6())
                {
                    return Err(NmstateError::new(
                        ErrorKind::InvalidArgument,
                        format!(
                            "Got IPv6 address {} in ipv4 config section",
                            addr
                        ),
                    ));
                }
                if let Some(addr) = addrs
                    .iter()
                    .find(|a| a.prefix_length as usize > IPV4_ADDR_LEN)
                {
                    return Err(NmstateError::new(
                        ErrorKind::InvalidArgument,
                        format!(
                            "Invalid IPv4 network prefix length '{}', \
                            should be in the range of 0 to {IPV4_ADDR_LEN}",
                            addr.prefix_length
                        ),
                    ));
                }
            }
            addrs.retain(|a| !a.is_auto());
            addrs.iter_mut().for_each(|a| {
                a.valid_life_time = None;
                a.preferred_life_time = None
            });
        }

        if !self.enabled {
            self.dhcp = None;
            self.addresses = None;
        }

        if self.dhcp != Some(true) {
            self.auto_dns = None;
            self.auto_gateway = None;
            self.auto_routes = None;
            self.auto_table_id = None;
            self.auto_route_metric = None;
            if is_desired && self.dhcp_client_id.is_some() {
                log::warn!(
                    "Ignoring `dhcp-client-id` setting when DHCPv4 is \
                    disabled"
                );
            }
            self.dhcp_client_id = None;
            self.dhcp_send_hostname = None;
            self.dhcp_custom_hostname = None;
        }
        if self.dhcp_send_hostname == Some(false) {
            if is_desired {
                if let Some(custom_hostname) =
                    self.dhcp_custom_hostname.as_deref()
                {
                    if !custom_hostname.is_empty() {
                        log::warn!(
                            "Ignoring `dhcp-custom-hostname: \
                            {custom_hostname}` as `dhcp-send-hostname` is \
                            disabled"
                        );
                    }
                }
            }
            self.dhcp_custom_hostname = None;
        }
        if let Some(addrs) = self.addresses.as_mut() {
            for addr in addrs.iter_mut() {
                addr.mptcp_flags = None;
            }
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for InterfaceIpv4 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = serde_json::Value::deserialize(deserializer)?;

        let prop_list = if let Some(v_map) = v.as_object() {
            get_ip_prop_list(v_map)
        } else {
            Vec::new()
        };
        if prop_list.contains(&"autoconf") {
            return Err(serde::de::Error::custom(
                "autoconf is not allowed for IPv4",
            ));
        }
        if prop_list.contains(&"dhcp_duid") {
            return Err(serde::de::Error::custom(
                "dhcp-duid is not allowed for IPv4",
            ));
        }

        let ip: InterfaceIp = match serde_json::from_value(v) {
            Ok(i) => i,
            Err(e) => {
                return Err(serde::de::Error::custom(format!("{e}")));
            }
        };
        let mut ret = Self::from(ip);
        ret.prop_list = prop_list;
        Ok(ret)
    }
}

impl From<InterfaceIp> for InterfaceIpv4 {
    fn from(ip: InterfaceIp) -> Self {
        Self {
            enabled: ip.enabled.unwrap_or_default(),
            dhcp: ip.dhcp,
            addresses: ip.addresses,
            dhcp_client_id: ip.dhcp_client_id,
            auto_dns: ip.auto_dns,
            auto_routes: ip.auto_routes,
            auto_gateway: ip.auto_gateway,
            auto_table_id: ip.auto_table_id,
            allow_extra_address: ip.allow_extra_address,
            auto_route_metric: ip.auto_route_metric,
            dhcp_send_hostname: ip.dhcp_send_hostname,
            dhcp_custom_hostname: ip.dhcp_custom_hostname,
            ..Default::default()
        }
    }
}

impl From<InterfaceIpv4> for InterfaceIp {
    fn from(ip: InterfaceIpv4) -> Self {
        let enabled = if ip.prop_list.contains(&"enabled") {
            Some(ip.enabled)
        } else {
            None
        };
        Self {
            enabled,
            dhcp: ip.dhcp,
            addresses: ip.addresses,
            dhcp_client_id: ip.dhcp_client_id,
            auto_dns: ip.auto_dns,
            auto_routes: ip.auto_routes,
            auto_gateway: ip.auto_gateway,
            auto_table_id: ip.auto_table_id,
            allow_extra_address: ip.allow_extra_address,
            auto_route_metric: ip.auto_route_metric,
            dhcp_send_hostname: ip.dhcp_send_hostname,
            dhcp_custom_hostname: ip.dhcp_custom_hostname,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
#[serde(into = "InterfaceIp")]
/// IPv6 configurations of interface.
/// Example output of interface holding automatic IPv6 settings:
/// ```yaml
/// ---
/// interfaces:
/// - name: eth1
///   state: up
///   mtu: 1500
///   ipv4:
///     enabled: false
///   ipv6:
///     address:
///       - ip: 2001:db8:2::1
///         prefix-length: 64
///       - ip: 2001:db8:1::1
///         prefix-length: 64
///       - ip: fe80::1ec1:cff:fe32:3bd3
///         prefix-length: 64
///     autoconf: true
///     dhcp: true
///     enabled: true
/// ```
pub struct InterfaceIpv6 {
    /// Whether IPv6 stack is enable. When set to false, the IPv6 stack is
    /// disabled with IPv6 link-local address purged also.
    pub enabled: bool,
    pub(crate) prop_list: Vec<&'static str>,
    /// Whether DHCPv6 enabled.
    pub dhcp: Option<bool>,
    /// DHCPv6 Unique Identifier
    /// Serialize and deserialize to/from `dhcp-duid`.
    pub dhcp_duid: Option<Dhcpv6Duid>,
    /// Whether autoconf via IPv6 router announcement enabled.
    pub autoconf: Option<bool>,
    /// IPv6 address generation mode.
    /// Serialize and deserialize to/from `addr-gen-mode`.
    pub addr_gen_mode: Option<Ipv6AddrGenMode>,
    /// IPv6 addresses. Will be ignored when applying with
    /// DHCPv6 or autoconf is enabled.
    /// When applying with `None`, current IP address will be preserved.
    /// When applying with `Some(Vec::new())`, all IP address will be removed.
    /// The IP addresses will apply to kernel with the same order specified.
    pub addresses: Option<Vec<InterfaceIpAddr>>,
    /// Whether to apply DNS resolver information retrieved from DHCPv6 or
    /// autoconf.
    /// Serialize and deserialize to/from `auto-dns`.
    /// If you want to append static DNS name server before DHCP/Autoconf
    /// provided servers, please set `auto-dns: true` explicitly a along with
    /// static DNS server.
    pub auto_dns: Option<bool>,
    /// Whether to set default gateway retrieved from autoconf.
    /// Serialize and deserialize to/from `auto-gateway`.
    pub auto_gateway: Option<bool>,
    /// Whether to set routes(including default gateway) retrieved from
    /// autoconf.
    /// Serialize and deserialize to/from `auto-routes`.
    pub auto_routes: Option<bool>,
    /// The route table ID used to hold routes(including default gateway)
    /// retrieved from autoconf.
    /// If not defined, the main(254) will be used.
    /// Serialize and deserialize to/from `auto-table-id`.
    pub auto_table_id: Option<u32>,
    /// By default(true), nmstate verification process allows extra IP address
    /// found as long as desired IP address matched.
    /// When set to false, the verification process of nmstate do exact equal
    /// check on IP address.
    /// Ignored when serializing.
    /// Deserialize from `allow-extra-address`.
    pub allow_extra_address: bool,
    /// Metric for routes retrieved from DHCP server.
    /// Only available for autoconf enabled interface.
    /// Deserialize from `auto-route-metric`.
    pub auto_route_metric: Option<u32>,
    /// IETF draft(expired) Tokenised IPv6 Identifiers. Should be only
    /// containing the tailing 64 bites for IPv6 address.
    pub token: Option<String>,
    /// Whether to include hostname in DHCP request in
    /// `Fully Qualified Domain Name (FQDN)` option(39) defined in RFC 4704.
    /// If not defined, set to True when DHCPv6 enabled.
    /// Deserialize from `dhcp-send-hostname`
    pub dhcp_send_hostname: Option<bool>,
    /// Custom string to override hostname used for DHCP request in
    /// `Fully Qualified Domain Name (FQDN)` option(29) defined in RFC 4704.
    /// If not defined, current non-dynamic hostname will be used.
    /// Deserialize from `dhcp-custom-hostname`
    pub dhcp_custom_hostname: Option<String>,

    pub(crate) dns: Option<DnsClientState>,
    pub(crate) rules: Option<Vec<RouteRuleEntry>>,
}

impl Default for InterfaceIpv6 {
    fn default() -> Self {
        Self {
            enabled: false,
            prop_list: Vec::new(),
            dhcp: None,
            dhcp_duid: None,
            autoconf: None,
            addr_gen_mode: None,
            addresses: None,
            dns: None,
            rules: None,
            auto_dns: None,
            auto_gateway: None,
            auto_routes: None,
            auto_table_id: None,
            allow_extra_address: default_allow_extra_address(),
            auto_route_metric: None,
            token: None,
            dhcp_send_hostname: None,
            dhcp_custom_hostname: None,
        }
    }
}

impl InterfaceIpv6 {
    /// New [InterfaceIpv6] with IP disabled.
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn is_auto(&self) -> bool {
        self.enabled && (self.dhcp == Some(true) || self.autoconf == Some(true))
    }

    pub fn is_static(&self) -> bool {
        self.enabled
            && !self.is_auto()
            && !self.addresses.as_deref().unwrap_or_default().is_empty()
    }

    // * Set auto_dns, auto_gateway and auto_routes to true if DHCP enabled and
    //   those options is None
    // * Disable DHCP and remove address if enabled: false
    // * Set DHCP options to None if DHCP is false
    // * Remove `mptcp_flags` as they are for query only
    pub(crate) fn sanitize(
        &mut self,
        is_desired: bool,
    ) -> Result<(), NmstateError> {
        if let Some(addrs) = self.addresses.as_mut() {
            if is_desired {
                for addr in addrs.as_slice().iter().filter(|a| a.is_auto()) {
                    log::info!("Ignoring Auto IP address {}", addr);
                }
                if let Some(addr) = addrs.iter().find(|a| a.ip.is_ipv4()) {
                    return Err(NmstateError::new(
                        ErrorKind::InvalidArgument,
                        format!(
                            "Got IPv4 address {} in ipv6 config section",
                            addr
                        ),
                    ));
                }
                if let Some(addr) = addrs
                    .iter()
                    .find(|a| a.prefix_length as usize > IPV6_ADDR_LEN)
                {
                    return Err(NmstateError::new(
                        ErrorKind::InvalidArgument,
                        format!(
                            "Invalid IPv6 network prefix length '{}', \
                            should be in the range of 0 to {IPV6_ADDR_LEN}",
                            addr.prefix_length
                        ),
                    ));
                }
            }
            addrs.retain(|a| !a.is_auto());
            addrs.iter_mut().for_each(|a| {
                a.valid_life_time = None;
                a.preferred_life_time = None
            });
        }

        if self.is_auto() {
            if self.auto_dns.is_none() {
                self.auto_dns = Some(true);
            }
            if self.auto_routes.is_none() {
                self.auto_routes = Some(true);
            }
            if self.auto_gateway.is_none() {
                self.auto_gateway = Some(true);
            }
            if let Some(addrs) = self.addresses.as_ref() {
                if is_desired {
                    for addr in addrs {
                        log::info!(
                            "Static addresses {} defined when dynamic \
                            IP is enabled",
                            addr
                        );
                    }
                }
            }
        }

        if let Some(addrs) = self.addresses.as_mut() {
            addrs.retain(|addr| {
                if let IpAddr::V6(ip_addr) = addr.ip {
                    if is_ipv6_unicast_link_local(&ip_addr) {
                        if is_desired {
                            log::warn!(
                                "Ignoring IPv6 link local address {}/{}",
                                &addr.ip,
                                addr.prefix_length
                            );
                        }
                        false
                    } else {
                        true
                    }
                } else {
                    false
                }
            })
        };

        if !self.enabled {
            self.dhcp = None;
            self.autoconf = None;
            self.addresses = None;
        }

        if !self.is_auto() {
            self.auto_dns = None;
            self.auto_gateway = None;
            self.auto_routes = None;
            self.auto_table_id = None;
            self.auto_route_metric = None;
            self.dhcp_send_hostname = None;
            self.dhcp_custom_hostname = None;
        }
        if let Some(addrs) = self.addresses.as_mut() {
            for addr in addrs.iter_mut() {
                addr.mptcp_flags = None;
            }
        }
        if let Some(token) = self.token.as_mut() {
            if is_desired
                && self.autoconf == Some(false)
                && !(token.is_empty() || token == "::")
            {
                return Err(NmstateError::new(
                    ErrorKind::InvalidArgument,
                    format!(
                        "Desired IPv6 token '{token}' cannot \
                        be applied with IPv6 autoconf disabled, \
                        you may remove IPv6 token by setting as \
                        empty string or `::`"
                    ),
                ));
            }
            sanitize_ipv6_token_to_string(token)?;
        }
        if self.dhcp_send_hostname == Some(false) {
            if is_desired {
                if let Some(custom_hostname) =
                    self.dhcp_custom_hostname.as_deref()
                {
                    if !custom_hostname.is_empty() {
                        log::warn!(
                            "Ignoring `dhcp-custom-hostname: \
                            {custom_hostname}` as `dhcp-send-hostname` is \
                            disabled"
                        );
                    }
                }
            }
            self.dhcp_custom_hostname = None;
        }
        Ok(())
    }

    // Special action for generating merged state from desired and current.
    pub(crate) fn special_merge(&mut self, desired: &Self, current: &Self) {
        if !desired.prop_list.contains(&"enabled") {
            self.enabled = current.enabled;
        }
        if desired.dhcp.is_none() && self.enabled {
            self.dhcp = current.dhcp;
        }
        if desired.autoconf.is_none() && self.enabled {
            self.autoconf = current.autoconf;
        }
        // Normally, we expect backend to preserve configuration which not
        // mentioned in desire, but when DHCP switch from ON to OFF, the design
        // of nmstate is expecting dynamic IP address goes static. This should
        // be done by top level code.
        if current.is_auto()
            && current.addresses.is_some()
            && self.enabled
            && !self.is_auto()
            && is_ip_addrs_none_or_all_auto(desired.addresses.as_deref())
        {
            self.addresses = current.addresses.clone();
        }
        self.sanitize(false).ok();
    }

    pub(crate) fn merge_ip(&mut self, current: &Self) {
        if !self.prop_list.contains(&"enabled") {
            self.enabled = current.enabled;
        }
        if self.dhcp.is_none() && self.enabled {
            self.dhcp = current.dhcp;
        }
        if self.autoconf.is_none() && self.enabled {
            self.autoconf = current.autoconf;
        }
        // Normally, we expect backend to preserve configuration which not
        // mentioned in desire, but when DHCP switch from ON to OFF, the design
        // of nmstate is expecting dynamic IP address goes static. This should
        // be done by top level code.
        if current.is_auto()
            && current.addresses.is_some()
            && self.enabled
            && !self.is_auto()
            && is_ip_addrs_none_or_all_auto(self.addresses.as_deref())
        {
            self.addresses = current.addresses.clone();
            if let Some(addrs) = self.addresses.as_mut() {
                addrs.as_mut_slice().iter_mut().for_each(|a| {
                    a.valid_life_time = None;
                    a.preferred_life_time = None;
                });
            }
        }
    }
}

impl<'de> Deserialize<'de> for InterfaceIpv6 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v = serde_json::Value::deserialize(deserializer)?;

        let prop_list = if let Some(v_map) = v.as_object() {
            get_ip_prop_list(v_map)
        } else {
            Vec::new()
        };
        if prop_list.contains(&"dhcp_client_id") {
            return Err(serde::de::Error::custom(
                "dhcp-client-id is not allowed for IPv6",
            ));
        }
        let ip: InterfaceIp = match serde_json::from_value(v) {
            Ok(i) => i,
            Err(e) => {
                return Err(serde::de::Error::custom(format!("{e}")));
            }
        };
        let mut ret = Self::from(ip);
        ret.prop_list = prop_list;
        Ok(ret)
    }
}

impl From<InterfaceIp> for InterfaceIpv6 {
    fn from(ip: InterfaceIp) -> Self {
        Self {
            enabled: ip.enabled.unwrap_or_default(),
            dhcp: ip.dhcp,
            autoconf: ip.autoconf,
            addresses: ip.addresses,
            dhcp_duid: ip.dhcp_duid,
            auto_dns: ip.auto_dns,
            auto_routes: ip.auto_routes,
            auto_gateway: ip.auto_gateway,
            auto_table_id: ip.auto_table_id,
            addr_gen_mode: ip.addr_gen_mode,
            allow_extra_address: ip.allow_extra_address,
            auto_route_metric: ip.auto_route_metric,
            token: ip.token,
            dhcp_send_hostname: ip.dhcp_send_hostname,
            dhcp_custom_hostname: ip.dhcp_custom_hostname,
            ..Default::default()
        }
    }
}

impl From<InterfaceIpv6> for InterfaceIp {
    fn from(ip: InterfaceIpv6) -> Self {
        let enabled = if ip.prop_list.contains(&"enabled") {
            Some(ip.enabled)
        } else {
            None
        };
        Self {
            enabled,
            dhcp: ip.dhcp,
            autoconf: ip.autoconf,
            addresses: ip.addresses,
            dhcp_duid: ip.dhcp_duid,
            auto_dns: ip.auto_dns,
            auto_routes: ip.auto_routes,
            auto_gateway: ip.auto_gateway,
            auto_table_id: ip.auto_table_id,
            addr_gen_mode: ip.addr_gen_mode,
            allow_extra_address: ip.allow_extra_address,
            auto_route_metric: ip.auto_route_metric,
            token: ip.token,
            dhcp_send_hostname: ip.dhcp_send_hostname,
            dhcp_custom_hostname: ip.dhcp_custom_hostname,
            ..Default::default()
        }
    }
}

#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize,
)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[non_exhaustive]
pub struct InterfaceIpAddr {
    /// IP address.
    pub ip: IpAddr,
    #[serde(deserialize_with = "crate::deserializer::u8_or_string")]
    /// Prefix length.
    /// Serialize and deserialize to/from `prefix-length`.
    pub prefix_length: u8,
    #[serde(skip_serializing_if = "is_none_or_empty_mptcp_flags", default)]
    /// MPTCP flag on this IP address.
    /// Ignored when applying as nmstate does not support support IP address
    /// specific MPTCP flags. You should apply MPTCP flags at interface level
    /// via [BaseInterface.mptcp].
    pub mptcp_flags: Option<Vec<MptcpAddressFlag>>,
    /// Remaining time for IP address been valid. The output format is
    /// "32sec" or "forever".
    /// This property is query only, it will be ignored when applying.
    /// Serialize to `valid-life-time`.
    /// Deserialize from `valid-life-time` or `valid-left` or `valid-lft`.
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "valid-left",
        alias = "valid-lft"
    )]
    pub valid_life_time: Option<String>,
    /// Remaining time for IP address been preferred. The output format is
    /// "32sec" or "forever".
    /// This property is query only, it will be ignored when applying.
    /// Serialize to `preferred-life-time`.
    /// Deserialize from `preferred-life-time` or `preferred-left` or
    /// `preferred-lft`.
    #[serde(
        skip_serializing_if = "Option::is_none",
        alias = "preferred-left",
        alias = "preferred-lft"
    )]
    pub preferred_life_time: Option<String>,
}

impl Default for InterfaceIpAddr {
    fn default() -> Self {
        Self {
            ip: IpAddr::V6(std::net::Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1)),
            prefix_length: 128,
            mptcp_flags: None,
            valid_life_time: None,
            preferred_life_time: None,
        }
    }
}

impl std::fmt::Display for InterfaceIpAddr {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_auto() {
            write!(
                f,
                "{}/{} valid_life_time {} preferred_lft {}",
                self.ip,
                self.prefix_length,
                self.valid_life_time.as_deref().unwrap_or(FOREVER),
                self.preferred_life_time.as_deref().unwrap_or(FOREVER)
            )
        } else {
            write!(f, "{}/{}", self.ip, self.prefix_length)
        }
    }
}

impl InterfaceIpAddr {
    pub(crate) fn is_auto(&self) -> bool {
        self.valid_life_time.is_some()
            && self.valid_life_time.as_deref() != Some(FOREVER)
    }
}

pub(crate) fn is_ipv6_addr(addr: &str) -> bool {
    addr.contains(':')
}

// Copy from Rust official std::net::Ipv6Addr::is_unicast_link_local() which
// is experimental.
pub(crate) fn is_ipv6_unicast_link_local(ip: &Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xffc0) == 0xfe80
}

impl std::convert::TryFrom<&str> for InterfaceIpAddr {
    type Error = NmstateError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let mut addr: Vec<&str> = value.split('/').collect();
        addr.resize(2, "");
        let ip = IpAddr::from_str(addr[0]).map_err(|e| {
            let e = NmstateError::new(
                ErrorKind::InvalidArgument,
                format!("Invalid IP address {}: {e}", addr[0]),
            );
            log::error!("{}", e);
            e
        })?;

        let prefix_length = if addr[1].is_empty() {
            if ip.is_ipv6() {
                128
            } else {
                32
            }
        } else {
            addr[1].parse::<u8>().map_err(|parse_error| {
                let e = NmstateError::new(
                    ErrorKind::InvalidArgument,
                    format!("Invalid IP address {value}: {parse_error}"),
                );
                log::error!("{}", e);
                e
            })?
        };
        Ok(Self {
            ip,
            prefix_length,
            mptcp_flags: None,
            valid_life_time: None,
            preferred_life_time: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(from = "String", into = "String")]
/// DHCPv4 client ID
pub enum Dhcpv4ClientId {
    /// Use link layer address as DHCPv4 client ID.
    /// Serialize and deserialize to/from `ll`.
    LinkLayerAddress,
    /// RFC 4361 type 255, 32 bits IAID followed by DUID.
    /// Serialize and deserialize to/from `iaid+duid`.
    IaidPlusDuid,
    /// hex string or backend specific client id type
    Other(String),
}

impl std::fmt::Display for Dhcpv4ClientId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from(self.clone()))
    }
}

impl From<String> for Dhcpv4ClientId {
    fn from(s: String) -> Self {
        return match s.as_str() {
            "ll" | "LL" => Self::LinkLayerAddress,
            "iaid+duid" | "IAID+DUID" => Self::IaidPlusDuid,
            _ => Self::Other(s),
        };
    }
}

impl From<Dhcpv4ClientId> for String {
    fn from(v: Dhcpv4ClientId) -> Self {
        match v {
            Dhcpv4ClientId::LinkLayerAddress => "ll".to_string(),
            Dhcpv4ClientId::IaidPlusDuid => "iaid+duid".to_string(),
            Dhcpv4ClientId::Other(s) => s,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(from = "String", into = "String")]
/// DHCPv6 Unique Identifier
pub enum Dhcpv6Duid {
    /// DUID Based on Link-Layer Address Plus Time
    /// Serialize and deserialize to/from `llt`.
    LinkLayerAddressPlusTime,
    /// DUID Assigned by Vendor Based on Enterprise Number
    /// Serialize and deserialize to/from `en`.
    EnterpriseNumber,
    /// DUID Assigned by Vendor Based on Enterprise Number
    /// Serialize and deserialize to/from `ll`.
    LinkLayerAddress,
    /// DUID Based on Universally Unique Identifier
    /// Serialize and deserialize to/from `uuid`.
    Uuid,
    /// Backend specific
    Other(String),
}

impl std::fmt::Display for Dhcpv6Duid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", String::from(self.clone()))
    }
}

impl From<String> for Dhcpv6Duid {
    fn from(s: String) -> Self {
        return match s.as_str() {
            "llt" | "LLT" => Self::LinkLayerAddressPlusTime,
            "en" | "EN" => Self::EnterpriseNumber,
            "ll" | "LL" => Self::LinkLayerAddress,
            "uuid" | "UUID" => Self::Uuid,
            _ => Self::Other(s),
        };
    }
}

impl From<Dhcpv6Duid> for String {
    fn from(v: Dhcpv6Duid) -> Self {
        match v {
            Dhcpv6Duid::LinkLayerAddressPlusTime => "llt".to_string(),
            Dhcpv6Duid::EnterpriseNumber => "en".to_string(),
            Dhcpv6Duid::LinkLayerAddress => "ll".to_string(),
            Dhcpv6Duid::Uuid => "uuid".to_string(),
            Dhcpv6Duid::Other(s) => s,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(from = "String", into = "String")]
/// IPv6 address generation mode
pub enum Ipv6AddrGenMode {
    /// EUI-64 format defined by RFC 4862
    /// Serialize and deserialize to/from `eui64`.
    Eui64,
    /// Semantically Opaque Interface Identifiers defined by RFC 7217
    /// Serialize and deserialize to/from `stable-privacy`.
    StablePrivacy,
    /// Backend specific
    Other(String),
}

impl From<String> for Ipv6AddrGenMode {
    fn from(s: String) -> Self {
        return match s.as_str() {
            "eui64" | "EUI64" => Self::Eui64,
            "stable-privacy" | "STABLE-PRIVACY" => Self::StablePrivacy,
            _ => Self::Other(s),
        };
    }
}

impl From<Ipv6AddrGenMode> for String {
    fn from(v: Ipv6AddrGenMode) -> Self {
        match v {
            Ipv6AddrGenMode::Eui64 => "eui64".to_string(),
            Ipv6AddrGenMode::StablePrivacy => "stable-privacy".to_string(),
            Ipv6AddrGenMode::Other(s) => s,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
#[non_exhaustive]
/// Which IP stack should network backend wait before considering the interface
/// activation finished.
pub enum WaitIp {
    /// The activation is considered done once IPv4 stack or IPv6 stack is
    /// configure
    /// Serialize and deserialize to/from `any`.
    Any,
    /// The activation is considered done once IPv4 stack is configured.
    /// Serialize and deserialize to/from `ipv4`.
    Ipv4,
    /// The activation is considered done once IPv6 stack is configured.
    /// Serialize and deserialize to/from `ipv6`.
    Ipv6,
    /// The activation is considered done once both IPv4 and IPv6 stack are
    /// configured.
    /// Serialize and deserialize to/from `ipv4+ipv6`.
    #[serde(rename = "ipv4+ipv6")]
    Ipv4AndIpv6,
}

impl std::fmt::Display for WaitIp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Any => "any",
                Self::Ipv4 => "ipv4",
                Self::Ipv6 => "ipv6",
                Self::Ipv4AndIpv6 => "ipv4+ipv6",
            }
        )
    }
}

fn validate_wait_ip(base_iface: &BaseInterface) -> Result<(), NmstateError> {
    if let Some(wait_ip) = base_iface.wait_ip.as_ref() {
        if (wait_ip == &WaitIp::Ipv4 || wait_ip == &WaitIp::Ipv4AndIpv6)
            && !base_iface
                .ipv4
                .as_ref()
                .map(|i| i.enabled)
                .unwrap_or_default()
        {
            let e = NmstateError::new(
                ErrorKind::InvalidArgument,
                format!(
                    "Cannot set 'wait-ip: {}' with IPv4 disabled. \
                    Interface: {}({})",
                    wait_ip, &base_iface.name, &base_iface.iface_type
                ),
            );
            log::error!("{}", e);
            return Err(e);
        }
        if (wait_ip == &WaitIp::Ipv6 || wait_ip == &WaitIp::Ipv4AndIpv6)
            && !base_iface
                .ipv6
                .as_ref()
                .map(|i| i.enabled)
                .unwrap_or_default()
        {
            let e = NmstateError::new(
                ErrorKind::InvalidArgument,
                format!(
                    "Cannot set 'wait-ip: {}' with IPv6 disabled. \
                    Interface: {}({})",
                    wait_ip, &base_iface.name, &base_iface.iface_type
                ),
            );
            log::error!("{}", e);
            return Err(e);
        }
    }

    Ok(())
}

fn get_ip_prop_list(
    map: &serde_json::Map<String, serde_json::Value>,
) -> Vec<&'static str> {
    let mut ret = Vec::new();

    if map.contains_key("enabled") {
        ret.push("enabled")
    }
    if map.contains_key("dhcp") {
        ret.push("dhcp")
    }
    if map.contains_key("autoconf") {
        ret.push("autoconf")
    }
    if map.contains_key("dhcp-client-id") {
        ret.push("dhcp_client_id")
    }
    if map.contains_key("dhcp-duid") {
        ret.push("dhcp_duid")
    }
    if map.contains_key("address") {
        ret.push("addresses")
    }
    if map.contains_key("auto-dns") {
        ret.push("auto_dns")
    }
    if map.contains_key("auto-gateway") {
        ret.push("auto_gateway")
    }
    if map.contains_key("auto-routes") {
        ret.push("auto_routes")
    }
    if map.contains_key("auto-route-table-id") {
        ret.push("auto_table_id")
    }
    if map.contains_key("addr-gen-mode") {
        ret.push("addr_gen_mode")
    }
    if map.contains_key("dhcp-send-hostname") {
        ret.push("dhcp_send_hostname")
    }
    if map.contains_key("dhcp-custom-hostname") {
        ret.push("dhcp_custom_hostname")
    }
    ret
}

pub(crate) fn sanitize_ip_network(
    ip_net: &str,
) -> Result<String, NmstateError> {
    let ip_nets: Vec<&str> = ip_net.split('/').collect();
    match ip_nets.len() {
        0 => Err(NmstateError::new(
            ErrorKind::InvalidArgument,
            "Invalid IP network string, got empty string".into(),
        )),
        1 => {
            let ip = IpAddr::from_str(ip_nets[0]).map_err(|e| {
                NmstateError::new(
                    ErrorKind::InvalidArgument,
                    format!("Invalid IP network '{ip_net}': {e}",),
                )
            })?;
            Ok(if ip.is_ipv6() {
                format!("{ip}/{IPV6_ADDR_LEN}")
            } else {
                format!("{ip}/{IPV4_ADDR_LEN}")
            })
        }
        2 => {
            let prefix_len = ip_nets[1].parse::<usize>().map_err(|e| {
                NmstateError::new(
                    ErrorKind::InvalidArgument,
                    format!(
                        "Invalid IP network prefix length \
                        '{}' in '{ip_net}': {e}",
                        ip_nets[1]
                    ),
                )
            })?;
            let ip =
                apply_ip_prefix_len(IpAddr::from_str(ip_nets[0])?, prefix_len);
            if ip.is_ipv6() {
                if prefix_len > IPV6_ADDR_LEN {
                    Err(NmstateError::new(
                        ErrorKind::InvalidArgument,
                        format!(
                            "Invalid IPv6 network prefix length '{}' \
                            in '{ip_net}', should be smaller than \
                            {IPV6_ADDR_LEN}'",
                            ip_nets[1],
                        ),
                    ))
                } else {
                    Ok(format!("{ip}/{prefix_len}"))
                }
            } else if prefix_len > IPV4_ADDR_LEN {
                Err(NmstateError::new(
                    ErrorKind::InvalidArgument,
                    format!(
                        "Invalid IPv4 network prefix length '{}' \
                        in '{ip_net}', should be smaller than \
                        {IPV4_ADDR_LEN}'",
                        ip_nets[1],
                    ),
                ))
            } else {
                Ok(format!("{ip}/{prefix_len}"))
            }
        }
        _ => Err(NmstateError::new(
            ErrorKind::InvalidArgument,
            format!(
                "Invalid IP network string: '{ip_net}', expecting 'ip/prefix' \
                or 'ip' format, for example: 192.0.2.0/24 or \
                2001:db8:1::/64 or 192.0.2.1"
            ),
        )),
    }
}

fn is_none_or_empty_mptcp_flags(v: &Option<Vec<MptcpAddressFlag>>) -> bool {
    if let Some(v) = v {
        v.is_empty()
    } else {
        true
    }
}

// Allow extra IP by default
fn default_allow_extra_address() -> bool {
    true
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum AddressFamily {
    IPv4,
    IPv6,
    Unknown,
}

impl From<u8> for AddressFamily {
    fn from(d: u8) -> Self {
        match d {
            AF_INET => AddressFamily::IPv4,
            AF_INET6 => AddressFamily::IPv6,
            _ => AddressFamily::Unknown,
        }
    }
}

impl Default for AddressFamily {
    fn default() -> Self {
        Self::IPv4
    }
}

impl std::fmt::Display for AddressFamily {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::IPv4 => "ipv4",
                Self::IPv6 => "ipv6",
                Self::Unknown => "unknown",
            }
        )
    }
}

impl MergedInterface {
    // * Merge `enabled: true` from current if not mentioned in desired.
    pub(crate) fn post_inter_ifaces_process_ip(
        &mut self,
    ) -> Result<(), NmstateError> {
        if let (Some(current), Some(apply_iface), Some(verify_iface)) = (
            self.current.as_ref().map(|i| i.base_iface()),
            self.for_apply.as_mut().map(|i| i.base_iface_mut()),
            self.for_verify.as_mut().map(|i| i.base_iface_mut()),
        ) {
            if let (Some(des_ipv4), Some(cur_ipv4)) =
                (apply_iface.ipv4.as_mut(), current.ipv4.as_ref())
            {
                des_ipv4.merge_ip(cur_ipv4);
            }
            if let (Some(des_ipv6), Some(cur_ipv6)) =
                (apply_iface.ipv6.as_mut(), current.ipv6.as_ref())
            {
                des_ipv6.merge_ip(cur_ipv6);
            }
            if let (Some(des_ipv4), Some(cur_ipv4)) =
                (verify_iface.ipv4.as_mut(), current.ipv4.as_ref())
            {
                des_ipv4.merge_ip(cur_ipv4);
            }
            if let (Some(des_ipv6), Some(cur_ipv6)) =
                (verify_iface.ipv6.as_mut(), current.ipv6.as_ref())
            {
                des_ipv6.merge_ip(cur_ipv6);
            }
            validate_wait_ip(apply_iface)?;
            if !apply_iface.can_have_ip() {
                apply_iface.wait_ip = None;
                verify_iface.wait_ip = None;
            }
        }

        Ok(())
    }
}

// User might define IPv6 token in the format of `::0.0.250.193`, which should
// be sanitize to `::fac1`.
fn sanitize_ipv6_token_to_string(
    token: &mut String,
) -> Result<(), NmstateError> {
    // Empty token means reverting to default "::"
    if token.is_empty() {
        write!(token, "::").ok();
    } else {
        match Ipv6Addr::from_str(token.as_str()) {
            Ok(ip) => {
                if ip.octets()[..8] != [0; 8] {
                    return Err(NmstateError::new(
                        ErrorKind::InvalidArgument,
                        format!(
                            "Desired IPv6 token should be lead \
                            by 64 bits 0. But got {token}"
                        ),
                    ));
                }
                // The Ipv6Addr::to_string() will convert
                //  ::fac1 to ::0.0.250.193
                // Which is no ideal in this case
                // To workaround that, we set leading 64 bits to '2001:db8::',
                // and then trip it out from string.
                let mut segments = ip.segments();
                segments[0] = 0x2001;
                segments[1] = 0xdb8;
                let new_ip = Ipv6Addr::from(segments);
                token.clear();
                write!(token, "{}", &new_ip.to_string()["2001:db8".len()..])
                    .ok();
            }
            Err(e) => {
                return Err(NmstateError::new(
                    ErrorKind::InvalidArgument,
                    format!(
                        "Desired IPv6 token '{token}' is not a \
                        valid IPv6 address: {e}"
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn is_ip_addrs_none_or_all_auto(addrs: Option<&[InterfaceIpAddr]>) -> bool {
    addrs.map_or(true, |addrs| {
        addrs.iter().all(|a| {
            if let IpAddr::V6(ip_addr) = a.ip {
                is_ipv6_unicast_link_local(&ip_addr) || a.is_auto()
            } else {
                a.is_auto()
            }
        })
    })
}

fn apply_ip_prefix_len(ip: IpAddr, prefix_length: usize) -> IpAddr {
    if prefix_length == 0 {
        return if ip.is_ipv6() {
            IpAddr::V6(0.into())
        } else {
            IpAddr::V4(0.into())
        };
    }

    if (ip.is_ipv6() && prefix_length >= IPV6_ADDR_LEN)
        || (ip.is_ipv4() && prefix_length >= IPV4_ADDR_LEN)
    {
        return ip;
    }

    match ip {
        IpAddr::V6(i) => Ipv6Addr::from(
            u128::from(i) & (u128::MAX << (IPV6_ADDR_LEN - prefix_length)),
        )
        .into(),
        IpAddr::V4(i) => Ipv4Addr::from(
            u32::from(i) & (u32::MAX << (IPV4_ADDR_LEN - prefix_length)),
        )
        .into(),
    }
}
