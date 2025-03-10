// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use crate::{
    nispor::{
        base_iface::np_iface_to_base_iface,
        bond::{append_bond_port_config, np_bond_to_nmstate},
        error::np_error_to_nmstate,
        ethernet::np_ethernet_to_nmstate,
        hostname::get_hostname_state,
        infiniband::np_ib_to_nmstate,
        linux_bridge::{append_bridge_port_config, np_bridge_to_nmstate},
        mac_vlan::{np_mac_vlan_to_nmstate, np_mac_vtap_to_nmstate},
        macsec::np_macsec_to_nmstate,
        route::get_routes,
        route_rule::get_route_rules,
        veth::np_veth_to_nmstate,
        vlan::np_vlan_to_nmstate,
        vrf::np_vrf_to_nmstate,
        vxlan::np_vxlan_to_nmstate,
    },
    DummyInterface, Interface, InterfaceType, Interfaces, LoopbackInterface,
    NetworkState, NmstateError, OvsInterface, UnknownInterface,
};

pub(crate) fn nispor_retrieve(
    running_config_only: bool,
) -> Result<NetworkState, NmstateError> {
    let mut net_state = NetworkState {
        hostname: get_hostname_state(),
        prop_list: vec!["interfaces", "routes", "rules", "hostname"],
        ..Default::default()
    };
    let mut filter = nispor::NetStateFilter::default();
    // Do not query routes in order to prevent BGP routes consuming too much CPU
    // time, we let `get_routes()` do the query by itself.
    filter.route = None;
    let np_state = nispor::NetState::retrieve_with_filter(&filter)
        .map_err(np_error_to_nmstate)?;

    for (_, np_iface) in np_state.ifaces.iter() {
        // The `ovs-system` is reserved for OVS kernel datapath
        if np_iface.name == "ovs-system" {
            continue;
        }
        // The `ovs-netdev` is reserved for OVS netdev datapath
        if np_iface.name == "ovs-netdev" {
            continue;
        }
        // The vti interface is reserved for Ipsec
        if np_iface.iface_type == nispor::IfaceType::Other("Vti".into()) {
            continue;
        }

        let base_iface = np_iface_to_base_iface(np_iface, running_config_only);
        let iface = match &base_iface.iface_type {
            InterfaceType::LinuxBridge => {
                let mut br_iface = np_bridge_to_nmstate(np_iface, base_iface)?;
                let mut port_np_ifaces = Vec::new();
                for port_name in br_iface.ports().unwrap_or_default() {
                    if let Some(p) = np_state.ifaces.get(port_name) {
                        port_np_ifaces.push(p)
                    }
                }
                append_bridge_port_config(
                    &mut br_iface,
                    np_iface,
                    port_np_ifaces,
                );
                Interface::LinuxBridge(br_iface)
            }
            InterfaceType::Bond => {
                let mut bond_iface = np_bond_to_nmstate(np_iface, base_iface);
                let mut port_np_ifaces = Vec::new();
                for port_name in bond_iface.ports().unwrap_or_default() {
                    if let Some(p) = np_state.ifaces.get(port_name) {
                        port_np_ifaces.push(p)
                    }
                }
                append_bond_port_config(&mut bond_iface, port_np_ifaces);
                Interface::Bond(bond_iface)
            }
            InterfaceType::Ethernet => Interface::Ethernet(
                np_ethernet_to_nmstate(np_iface, base_iface),
            ),
            InterfaceType::Veth => {
                Interface::Ethernet(np_veth_to_nmstate(np_iface, base_iface))
            }
            InterfaceType::Vlan => {
                Interface::Vlan(np_vlan_to_nmstate(np_iface, base_iface))
            }
            InterfaceType::Vxlan => {
                Interface::Vxlan(np_vxlan_to_nmstate(np_iface, base_iface))
            }
            InterfaceType::Dummy => Interface::Dummy({
                let mut iface = DummyInterface::new();
                iface.base = base_iface;
                iface
            }),
            InterfaceType::OvsInterface => Interface::OvsInterface({
                let mut iface = OvsInterface::new();
                iface.base = base_iface;
                iface
            }),
            InterfaceType::MacVlan => {
                Interface::MacVlan(np_mac_vlan_to_nmstate(np_iface, base_iface))
            }
            InterfaceType::MacVtap => {
                Interface::MacVtap(np_mac_vtap_to_nmstate(np_iface, base_iface))
            }
            InterfaceType::Vrf => {
                Interface::Vrf(np_vrf_to_nmstate(np_iface, base_iface))
            }
            InterfaceType::InfiniBand => {
                // We don't support HFI interface which contains PKEY but no
                // parent.
                if base_iface.name.starts_with("hfi1") {
                    log::info!(
                        "Ignoring unsupported HFI interface {}",
                        base_iface.name
                    );
                    continue;
                }
                Interface::InfiniBand(np_ib_to_nmstate(np_iface, base_iface))
            }
            InterfaceType::Loopback => {
                Interface::Loopback(LoopbackInterface { base: base_iface })
            }
            InterfaceType::MacSec => {
                Interface::MacSec(np_macsec_to_nmstate(np_iface, base_iface))
            }
            _ => {
                log::info!(
                    "Got unsupported interface {} type {:?}",
                    np_iface.name,
                    np_iface.iface_type
                );
                Interface::Unknown({
                    let mut iface = UnknownInterface::new();
                    iface.base = base_iface;
                    iface
                })
            }
        };
        net_state.append_interface_data(iface);
    }
    set_controller_type(&mut net_state.interfaces);
    net_state.routes = get_routes(running_config_only);
    net_state.rules = get_route_rules(&np_state.rules, running_config_only);

    Ok(net_state)
}

fn set_controller_type(ifaces: &mut Interfaces) {
    let mut ctrl_to_type: HashMap<String, InterfaceType> = HashMap::new();
    for iface in ifaces.to_vec() {
        if iface.is_controller() {
            ctrl_to_type
                .insert(iface.name().to_string(), iface.iface_type().clone());
        }
    }
    for iface in ifaces.kernel_ifaces.values_mut() {
        if let Some(ctrl) = iface.base_iface().controller.as_ref() {
            if let Some(ctrl_type) = ctrl_to_type.get(ctrl) {
                iface.base_iface_mut().controller_type =
                    Some(ctrl_type.clone());
            }
        }
    }
}
