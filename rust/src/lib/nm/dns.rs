// SPDX-License-Identifier: Apache-2.0

use crate::{
    dns::parse_dns_ipv6_link_local_srv, ip::is_ipv6_addr,
    nm::settings::SUPPORTED_NM_KERNEL_IFACE_TYPES,
};
use crate::{
    DnsClientState, ErrorKind, Interface, InterfaceType, MergedInterface,
    MergedInterfaces, MergedNetworkState, NmstateError,
};

use super::nm_dbus::{
    NmActiveConnection, NmDevice, NmDeviceState,
    NM_ACTIVATION_STATE_FLAG_EXTERNAL,
};

const DEFAULT_DNS_PRIORITY: i32 = 40;

pub(crate) fn store_dns_config_to_iface(
    merged_state: &mut MergedNetworkState,
    nm_acs: &[NmActiveConnection],
    nm_devs: &[NmDevice],
) -> Result<(), NmstateError> {
    if merged_state.dns.is_changed()
        || !cur_dns_ifaces_still_valid_for_dns(&merged_state.interfaces)
    {
        let srvs = merged_state.dns.servers.as_slice();

        if srvs.len() > 2 && is_mixed_dns_servers(srvs) {
            return Err(NmstateError::new(
                ErrorKind::NotImplementedError,
                "Placing IPv4/IPv6 nameserver in the middle of IPv6/IPv4 \
                nameservers is not supported yet"
                    .to_string(),
            ));
        }

        let (cur_v4_ifaces, cur_v6_ifaces) =
            get_cur_dns_ifaces(&merged_state.interfaces);
        log::debug!(
            "Current DNS interface is \
            v4 {cur_v4_ifaces:?} v6 {cur_v6_ifaces:?}"
        );
        let (v4_iface_name, v6_iface_name) = reselect_dns_ifaces(
            merged_state,
            cur_v4_ifaces.as_slice(),
            cur_v6_ifaces.as_slice(),
            nm_acs,
            nm_devs,
        );
        log::debug!(
            "Re-selected DNS interfaces are v4 {v4_iface_name:?}, \
            v6 {v6_iface_name:?}"
        );

        purge_dns_config(false, cur_v4_ifaces.as_slice(), merged_state)?;
        purge_dns_config(true, cur_v6_ifaces.as_slice(), merged_state)?;
        save_dns_to_iface(&v4_iface_name, &v6_iface_name, merged_state)?;
    }
    Ok(())
}

// If DNS changed or desired, we find out interface to hold the DNS entry in the
// order of:
//  * If current interface which holding the DNS still valid for DNS also listed
//    in desire state
//  * Interfaces in desired with manual IP stack enabled or `auto_dns: false`
//  * Interfaces in desired with IP enabled
//  * Interfaces in current with manual IP stack enabled or `auto_dns: false`
//    and current interface is not marked as external managed or unmanaged.
//  * TODO: loopback interface
pub(crate) fn reselect_dns_ifaces(
    merged_state: &MergedNetworkState,
    cur_v4_ifaces: &[String],
    cur_v6_ifaces: &[String],
    nm_acs: &[NmActiveConnection],
    nm_devs: &[NmDevice],
) -> (String, String) {
    let ipv4_iface = find_dns_iface(
        false,
        &merged_state.interfaces,
        cur_v4_ifaces,
        nm_acs,
        nm_devs,
    )
    .unwrap_or_default();

    let ipv6_iface = extract_ipv6_link_local_iface_from_dns_srv(
        merged_state.dns.servers.as_slice(),
    )
    .or_else(|| {
        find_dns_iface(
            true,
            &merged_state.interfaces,
            cur_v6_ifaces,
            nm_acs,
            nm_devs,
        )
    })
    .unwrap_or_default();

    (ipv4_iface, ipv6_iface)
}

// Find interface with DHCP disabled and IP enabled in the order of:
//  * Use current DNS interface if it is still valid.
//  * Use desire interface if it is preferred for DNS interface.
//  * Use desire interface if it is valid for DNS interface.
//  * Use current interface if it is valid for DNS interface. Skip if NM marked
//    it as unmanaged or external managed.
fn find_dns_iface(
    is_ipv6: bool,
    merged_ifaces: &MergedInterfaces,
    cur_dns_ifaces: &[String],
    nm_acs: &[NmActiveConnection],
    nm_devs: &[NmDevice],
) -> Option<String> {
    // Try using current DNS interface if in desired list
    for iface_name in cur_dns_ifaces {
        if let Some(iface) = merged_ifaces.kernel_ifaces.get(iface_name) {
            if iface.is_changed() && iface.is_iface_valid_for_dns(is_ipv6) {
                return Some(iface_name.to_string());
            }
        }
    }

    // Do not use loopback interface for DNS
    // Use insert order to produce consistent DNS interface choice
    for iface_name in
        merged_ifaces
            .insert_order
            .as_slice()
            .iter()
            .filter_map(|(n, t)| {
                if !t.is_userspace() && t != &InterfaceType::Loopback {
                    Some(n)
                } else {
                    None
                }
            })
    {
        if let Some(iface) = merged_ifaces.kernel_ifaces.get(iface_name) {
            if !iface.is_changed() {
                continue;
            }
            if iface.is_iface_prefered_for_dns(is_ipv6) {
                return Some(iface_name.to_string());
            }
        }
    }

    // Do not use loopback interface for DNS
    // Use insert order to produce consistent DNS interface choice
    for iface_name in
        merged_ifaces
            .insert_order
            .as_slice()
            .iter()
            .filter_map(|(n, t)| {
                if !t.is_userspace() && t != &InterfaceType::Loopback {
                    Some(n)
                } else {
                    None
                }
            })
    {
        if let Some(iface) = merged_ifaces.kernel_ifaces.get(iface_name) {
            if !iface.is_changed() {
                continue;
            }
            if iface.is_iface_valid_for_dns(is_ipv6) {
                return Some(iface_name.to_string());
            }
        }
    }

    let mut cur_iface_names: Vec<&str> = merged_ifaces
        .kernel_ifaces
        .values()
        .filter_map(|i| {
            if !i.is_changed()
                && i.merged.iface_type() != InterfaceType::Loopback
            {
                Some(i.merged.name())
            } else {
                None
            }
        })
        .collect();
    // Sort the interface names to produce consistent choice.
    cur_iface_names.sort_unstable();

    // Try again among undesired current interface
    for iface_name in cur_iface_names {
        if let Some(iface) = merged_ifaces.kernel_ifaces.get(iface_name) {
            if iface.is_iface_valid_for_dns(is_ipv6)
                && (!is_external_managed(iface_name, nm_acs))
                && (!is_unmanaged(iface_name, nm_devs))
            {
                return Some(iface_name.to_string());
            }
        }
    }

    None
}

pub(crate) fn extract_ipv6_link_local_iface_from_dns_srv(
    srvs: &[String],
) -> Option<String> {
    for srv in srvs {
        let splits: Vec<&str> = srv.split('%').collect();
        if splits.len() == 2 && !splits[1].is_empty() {
            log::debug!(
                "Extracted IPv6 link local DNS interface name \
                {} from {srv}",
                splits[1]
            );
            return Some(splits[1].to_string());
        }
    }
    None
}

pub(crate) fn purge_dns_config(
    is_ipv6: bool,
    ifaces: &[String],
    merged_state: &mut MergedNetworkState,
) -> Result<(), NmstateError> {
    for iface_name in ifaces {
        if let Some(iface) =
            merged_state.interfaces.kernel_ifaces.get_mut(iface_name)
        {
            if iface.merged.is_absent() {
                continue;
            }
            if !iface.is_changed() {
                iface.mark_as_changed();
            }
            if let Some(apply_iface) = iface.for_apply.as_mut() {
                if apply_iface.base_iface().can_have_ip() {
                    if is_ipv6 {
                        if apply_iface.base_iface().ipv6.is_none() {
                            apply_iface.base_iface_mut().ipv6 =
                                iface.merged.base_iface_mut().ipv6.clone();
                        }
                    } else if apply_iface.base_iface().ipv4.is_none() {
                        apply_iface.base_iface_mut().ipv4 =
                            iface.merged.base_iface_mut().ipv4.clone();
                    }

                    set_iface_dns_conf(
                        is_ipv6,
                        apply_iface,
                        Vec::new(),
                        Vec::new(),
                        Vec::new(),
                        None,
                    )?;
                }
            }
        }
    }
    Ok(())
}

fn save_dns_to_iface(
    v4_iface_name: &str,
    v6_iface_name: &str,
    merged_state: &mut MergedNetworkState,
) -> Result<(), NmstateError> {
    let mut v4_servers = Vec::new();
    let mut v6_servers = Vec::new();
    let prefer_ipv6_srv = merged_state
        .dns
        .servers
        .first()
        .map(|s| is_ipv6_addr(s.as_str()))
        .unwrap_or_default();
    for srv in merged_state.dns.servers.as_slice() {
        if is_ipv6_addr(srv) {
            v6_servers.push(srv.to_string())
        } else {
            v4_servers.push(srv.to_string())
        }
    }
    if !v6_servers.is_empty() {
        _save_dns_to_iface(
            true,
            v6_iface_name,
            v6_servers,
            merged_state,
            prefer_ipv6_srv,
        )?;
    }
    if !v4_servers.is_empty() {
        _save_dns_to_iface(
            false,
            v4_iface_name,
            v4_servers,
            merged_state,
            !prefer_ipv6_srv,
        )?;
    }
    Ok(())
}

// Argument `preferred`: true will save the searches
// Assuming all IPv6 link local address is pointing to specified argument
// `iface_name` iface.
fn _save_dns_to_iface(
    is_ipv6: bool,
    iface_name: &str,
    mut servers: Vec<String>,
    merged_state: &mut MergedNetworkState,
    preferred: bool,
) -> Result<(), NmstateError> {
    for srv in servers.as_mut_slice() {
        if let Some((ip, _)) = parse_dns_ipv6_link_local_srv(srv)? {
            srv.replace_range(.., ip.to_string().as_str());
        }
    }

    if iface_name.is_empty() {
        return Err(NmstateError::new(
            ErrorKind::InvalidArgument,
            format!(
                "Failed to find suitable(IP enabled) \
                interface for DNS server {servers:?}"
            ),
        ));
    }

    if let Some(iface) =
        merged_state.interfaces.kernel_ifaces.get_mut(iface_name)
    {
        if !iface.is_changed() {
            iface.mark_as_changed();
        }
        if let Some(apply_iface) = iface.for_apply.as_mut() {
            if is_ipv6 {
                if apply_iface.base_iface().ipv6.is_none() {
                    apply_iface.base_iface_mut().ipv6 =
                        iface.merged.base_iface_mut().ipv6.clone();
                }
            } else if apply_iface.base_iface().ipv4.is_none() {
                apply_iface.base_iface_mut().ipv4 =
                    iface.merged.base_iface_mut().ipv4.clone();
            }
        }
        if let Some(apply_iface) = iface.for_apply.as_mut() {
            if preferred {
                set_iface_dns_conf(
                    is_ipv6,
                    apply_iface,
                    servers,
                    merged_state.dns.searches.clone(),
                    merged_state.dns.options.clone(),
                    Some(DEFAULT_DNS_PRIORITY),
                )?;
            } else {
                set_iface_dns_conf(
                    is_ipv6,
                    apply_iface,
                    servers,
                    Vec::new(),
                    Vec::new(),
                    Some(DEFAULT_DNS_PRIORITY + 10),
                )?;
            }
        }
    } else {
        return Err(NmstateError::new(
            ErrorKind::Bug,
            format!(
                "_save_dns_to_iface(): Failed to find interface \
                {iface_name} among {merged_state:?}"
            ),
        ));
    }

    Ok(())
}

fn set_iface_dns_conf(
    is_ipv6: bool,
    iface: &mut Interface,
    servers: Vec<String>,
    searches: Vec<String>,
    options: Vec<String>,
    priority: Option<i32>,
) -> Result<(), NmstateError> {
    let dns_conf = DnsClientState {
        server: Some(servers),
        search: Some(searches),
        options: Some(options),
        priority,
    };
    if is_ipv6 {
        if let Some(ip_conf) = iface.base_iface_mut().ipv6.as_mut() {
            ip_conf.dns = Some(dns_conf);
        } else if !dns_conf.is_purge() {
            return Err(NmstateError::new(
                ErrorKind::Bug,
                format!("BUG: The dns interface is hold None IP {iface:?}"),
            ));
        }
    } else if let Some(ip_conf) = iface.base_iface_mut().ipv4.as_mut() {
        ip_conf.dns = Some(dns_conf);
    } else if !dns_conf.is_purge() {
        return Err(NmstateError::new(
            ErrorKind::Bug,
            format!("BUG: The dns interface is hold None IP {iface:?}"),
        ));
    }
    Ok(())
}

fn get_cur_dns_ifaces(
    merged_ifaces: &MergedInterfaces,
) -> (Vec<String>, Vec<String>) {
    let mut v4_ifaces: Vec<String> = Vec::new();
    let mut v6_ifaces: Vec<String> = Vec::new();
    for iface in merged_ifaces
        .kernel_ifaces
        .values()
        .filter(|i| i.current.is_some())
    {
        let cur_iface = if let Some(c) = iface.current.as_ref() {
            c
        } else {
            continue;
        };

        if let Some(ipv4) = &cur_iface.base_iface().ipv4 {
            if ipv4.enabled {
                if let Some(dns_conf) = &ipv4.dns {
                    if !dns_conf.is_null()
                        && !v4_ifaces.contains(&cur_iface.name().to_string())
                    {
                        v4_ifaces.push(cur_iface.name().to_string())
                    }
                }
            }
        }
        if let Some(ipv6) = &cur_iface.base_iface().ipv6 {
            if ipv6.enabled {
                if let Some(dns_conf) = &ipv6.dns {
                    if !dns_conf.is_null()
                        && !v6_ifaces.contains(&cur_iface.name().to_string())
                    {
                        v6_ifaces.push(cur_iface.name().to_string())
                    }
                }
            }
        }
    }
    (v4_ifaces, v6_ifaces)
}

pub(crate) fn cur_dns_ifaces_still_valid_for_dns(
    merged_ifaces: &MergedInterfaces,
) -> bool {
    let (cur_v4_ifaces, cur_v6_ifaces) = get_cur_dns_ifaces(merged_ifaces);
    for iface_name in &cur_v4_ifaces {
        if let Some(iface) = merged_ifaces.kernel_ifaces.get(iface_name) {
            if iface.is_changed() && !iface.is_iface_valid_for_dns(false) {
                return false;
            }
        }
    }
    for iface_name in &cur_v6_ifaces {
        if let Some(iface) = merged_ifaces.kernel_ifaces.get(iface_name) {
            if iface.is_changed() && !iface.is_iface_valid_for_dns(true) {
                return false;
            }
        }
    }
    true
}

fn is_mixed_dns_servers(srvs: &[String]) -> bool {
    let mut pattern = String::new();
    for srv in srvs {
        let cur_char = if is_ipv6_addr(srv) { '6' } else { '4' };
        if !pattern.ends_with(cur_char) {
            pattern.push(cur_char);
        }
    }
    pattern.contains("464") || pattern.contains("646")
}

impl MergedInterface {
    // These are considered preferred DNS interface:
    //  * Desire state has specified IP stack with static IP or auto with
    //    `auto_dns: false`
    //  * The IPv6 address is not empty
    pub(crate) fn is_iface_prefered_for_dns(&self, is_ipv6: bool) -> bool {
        if let Some(apply_iface) = self.for_apply.as_ref() {
            if is_ipv6 {
                apply_iface.base_iface().ipv6.as_ref().map(|ip_conf| {
                    ip_conf.enabled
                        && ((ip_conf.is_static()
                            && ip_conf
                                .addresses
                                .as_ref()
                                .map(|a| !a.is_empty())
                                == Some(true))
                            || (ip_conf.is_auto()
                                && ip_conf.auto_dns == Some(false)))
                }) == Some(true)
            } else {
                apply_iface.base_iface().ipv4.as_ref().map(|ip_conf| {
                    ip_conf.enabled
                        && (ip_conf.is_static()
                            || (ip_conf.is_auto()
                                && ip_conf.auto_dns == Some(false)))
                }) == Some(true)
            }
        } else {
            false
        }
    }

    // IP stack is merged with current at this point.
    pub(crate) fn is_iface_valid_for_dns(&self, is_ipv6: bool) -> bool {
        if is_ipv6 {
            self.merged.base_iface().ipv6.as_ref().map(|ip_conf| {
                ip_conf.enabled && (ip_conf.is_static() || (ip_conf.is_auto()))
            }) == Some(true)
        } else {
            self.merged.base_iface().ipv4.as_ref().map(|ip_conf| {
                ip_conf.enabled && (ip_conf.is_static() || (ip_conf.is_auto()))
            }) == Some(true)
        }
    }
}

fn is_external_managed(
    iface_name: &str,
    nm_acs: &[NmActiveConnection],
) -> bool {
    for nm_ac in nm_acs {
        if nm_ac.iface_name.as_str() == iface_name
            && SUPPORTED_NM_KERNEL_IFACE_TYPES
                .contains(&nm_ac.iface_type.as_str())
            && nm_ac.state_flags & NM_ACTIVATION_STATE_FLAG_EXTERNAL > 0
        {
            return true;
        }
    }
    false
}

fn is_unmanaged(iface_name: &str, nm_devs: &[NmDevice]) -> bool {
    for nm_dev in nm_devs {
        if nm_dev.name.as_str() == iface_name
            && SUPPORTED_NM_KERNEL_IFACE_TYPES
                .contains(&nm_dev.iface_type.as_str())
            && nm_dev.state == NmDeviceState::Unmanaged
        {
            return true;
        }
    }
    false
}

// Try to select a interface to store the DNS search or option only information
// in the order of:
// * Use current DNS interface if still desired and still valid
// * Use auto interface from desired state
// * Use auto interface from current state
// * Use IP(prefer IPv6) enabled interface from desired state
// * Use IP(prefer IPv6) enabled interface from current state
// * Use current DNS interface if still valid
pub(crate) fn store_dns_search_or_option_to_iface(
    merged_state: &mut MergedNetworkState,
    nm_acs: &[NmActiveConnection],
    nm_devs: &[NmDevice],
) -> Result<(), NmstateError> {
    let (cur_v4_ifaces, cur_v6_ifaces) =
        get_cur_dns_ifaces(&merged_state.interfaces);

    // Use current DNS interface if they are desired
    for iface_name in cur_v6_ifaces {
        if let Some(iface) =
            merged_state.interfaces.kernel_ifaces.get_mut(&iface_name)
        {
            if iface.is_iface_valid_for_dns(true) {
                if let Some(apply_iface) = iface.for_apply.as_mut() {
                    set_iface_dns_conf(
                        true,
                        apply_iface,
                        Vec::new(),
                        merged_state.dns.searches.clone(),
                        merged_state.dns.options.clone(),
                        Some(DEFAULT_DNS_PRIORITY),
                    )?;
                    return Ok(());
                }
            }
        }
    }

    for iface_name in cur_v4_ifaces {
        if let Some(iface) =
            merged_state.interfaces.kernel_ifaces.get_mut(&iface_name)
        {
            if iface.is_iface_valid_for_dns(false) {
                if let Some(apply_iface) = iface.for_apply.as_mut() {
                    set_iface_dns_conf(
                        false,
                        apply_iface,
                        Vec::new(),
                        merged_state.dns.searches.clone(),
                        merged_state.dns.options.clone(),
                        Some(DEFAULT_DNS_PRIORITY),
                    )?;
                    return Ok(());
                }
            }
        }
    }

    // Use auto interface
    if store_dns_search_or_options_to_auto_iface(merged_state, nm_acs, nm_devs)
        .is_ok()
    {
        return Ok(());
    }

    store_dns_search_or_options_to_ip_enabled_iface(
        merged_state,
        nm_acs,
        nm_devs,
    )
}

fn set_iface_dns_search_or_option(
    iface: &mut MergedInterface,
    searches: Vec<String>,
    options: Vec<String>,
    is_ipv6: bool,
) -> Result<(), NmstateError> {
    if iface.for_apply.is_none() {
        iface.mark_as_changed();
    }
    if let Some(apply_iface) = iface.for_apply.as_mut() {
        if is_ipv6 {
            if apply_iface.base_iface().ipv6.is_none() {
                apply_iface.base_iface_mut().ipv6 =
                    iface.merged.base_iface_mut().ipv6.clone();
            }
        } else if apply_iface.base_iface().ipv4.is_none() {
            apply_iface.base_iface_mut().ipv4 =
                iface.merged.base_iface_mut().ipv4.clone();
        }
        set_iface_dns_conf(
            is_ipv6,
            apply_iface,
            Vec::new(),
            searches,
            options,
            Some(DEFAULT_DNS_PRIORITY),
        )?;
    }
    Ok(())
}

fn store_dns_search_or_options_to_auto_iface(
    merged_state: &mut MergedNetworkState,
    nm_acs: &[NmActiveConnection],
    nm_devs: &[NmDevice],
) -> Result<(), NmstateError> {
    // Use insert order to produce consistent DNS interface choice
    for iface_name in merged_state
        .interfaces
        .insert_order
        .as_slice()
        .iter()
        .filter_map(|(n, t)| {
            if !t.is_userspace() && t != &InterfaceType::Loopback {
                Some(n)
            } else {
                None
            }
        })
    {
        let iface =
            match merged_state.interfaces.kernel_ifaces.get_mut(iface_name) {
                Some(i) => i,
                None => continue,
            };
        if iface
            .merged
            .base_iface()
            .ipv6
            .as_ref()
            .map(|i| i.is_auto())
            .unwrap_or_default()
        {
            return set_iface_dns_search_or_option(
                iface,
                merged_state.dns.searches.clone(),
                merged_state.dns.options.clone(),
                true,
            );
        }
        if iface
            .merged
            .base_iface()
            .ipv4
            .as_ref()
            .map(|i| i.is_auto())
            .unwrap_or_default()
        {
            return set_iface_dns_search_or_option(
                iface,
                merged_state.dns.searches.clone(),
                merged_state.dns.options.clone(),
                false,
            );
        }
    }

    let mut cur_iface_names: Vec<String> = merged_state
        .interfaces
        .kernel_ifaces
        .values()
        .filter_map(|i| {
            if !i.is_changed()
                && i.merged.iface_type() != InterfaceType::Loopback
            {
                Some(i.merged.name().to_string())
            } else {
                None
            }
        })
        .collect();
    // Sort the interface names to produce consistent choice.
    cur_iface_names.sort_unstable();

    // * Use auto interface from current state
    for iface_name in &cur_iface_names {
        if is_external_managed(iface_name, nm_acs)
            || is_unmanaged(iface_name, nm_devs)
        {
            continue;
        }
        let iface =
            match merged_state.interfaces.kernel_ifaces.get_mut(iface_name) {
                Some(i) => i,
                None => continue,
            };
        if iface
            .merged
            .base_iface()
            .ipv6
            .as_ref()
            .map(|i| i.is_auto())
            .unwrap_or_default()
        {
            return set_iface_dns_search_or_option(
                iface,
                merged_state.dns.searches.clone(),
                merged_state.dns.options.clone(),
                true,
            );
        }
        if iface
            .merged
            .base_iface()
            .ipv4
            .as_ref()
            .map(|i| i.is_auto())
            .unwrap_or_default()
        {
            return set_iface_dns_search_or_option(
                iface,
                merged_state.dns.searches.clone(),
                merged_state.dns.options.clone(),
                false,
            );
        }
    }

    Err(NmstateError::new(
        ErrorKind::InvalidArgument,
        format!(
            "Failed to find suitable(Auto IP) interface for DNS \
            searches '{}' or options '{}'",
            merged_state.dns.searches.as_slice().join(" "),
            merged_state.dns.options.as_slice().join(" ")
        ),
    ))
}

fn store_dns_search_or_options_to_ip_enabled_iface(
    merged_state: &mut MergedNetworkState,
    nm_acs: &[NmActiveConnection],
    nm_devs: &[NmDevice],
) -> Result<(), NmstateError> {
    // Use insert order to produce consistent DNS interface choice
    for iface_name in merged_state
        .interfaces
        .insert_order
        .as_slice()
        .iter()
        .filter_map(|(n, t)| {
            if !t.is_userspace() && t != &InterfaceType::Loopback {
                Some(n)
            } else {
                None
            }
        })
    {
        let iface =
            match merged_state.interfaces.kernel_ifaces.get_mut(iface_name) {
                Some(i) => i,
                None => continue,
            };
        if iface
            .merged
            .base_iface()
            .ipv6
            .as_ref()
            .map(|i| i.enabled)
            .unwrap_or_default()
        {
            return set_iface_dns_search_or_option(
                iface,
                merged_state.dns.searches.clone(),
                merged_state.dns.options.clone(),
                true,
            );
        }
        if iface
            .merged
            .base_iface()
            .ipv4
            .as_ref()
            .map(|i| i.enabled)
            .unwrap_or_default()
        {
            return set_iface_dns_search_or_option(
                iface,
                merged_state.dns.searches.clone(),
                merged_state.dns.options.clone(),
                false,
            );
        }
    }

    let mut cur_iface_names: Vec<String> = merged_state
        .interfaces
        .kernel_ifaces
        .values()
        .filter_map(|i| {
            if !i.is_changed()
                && i.merged.iface_type() != InterfaceType::Loopback
            {
                Some(i.merged.name().to_string())
            } else {
                None
            }
        })
        .collect();
    // Sort the interface names to produce consistent choice.
    cur_iface_names.sort_unstable();

    // * Use auto interface from current state
    for iface_name in &cur_iface_names {
        if is_external_managed(iface_name, nm_acs)
            || is_unmanaged(iface_name, nm_devs)
        {
            continue;
        }
        let iface =
            match merged_state.interfaces.kernel_ifaces.get_mut(iface_name) {
                Some(i) => i,
                None => continue,
            };
        if iface
            .merged
            .base_iface()
            .ipv6
            .as_ref()
            .map(|i| i.enabled)
            .unwrap_or_default()
        {
            return set_iface_dns_search_or_option(
                iface,
                merged_state.dns.searches.clone(),
                merged_state.dns.options.clone(),
                true,
            );
        }
        if iface
            .merged
            .base_iface()
            .ipv4
            .as_ref()
            .map(|i| i.enabled)
            .unwrap_or_default()
        {
            return set_iface_dns_search_or_option(
                iface,
                merged_state.dns.searches.clone(),
                merged_state.dns.options.clone(),
                false,
            );
        }
    }

    Err(NmstateError::new(
        ErrorKind::InvalidArgument,
        format!(
            "Failed to find suitable(IP enabled) interface for DNS searches \
            '{}'",
            merged_state.dns.searches.as_slice().join(" ")
        ),
    ))
}
