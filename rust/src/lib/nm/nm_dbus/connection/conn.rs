// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::convert::TryFrom;

use log::warn;

use serde::Deserialize;
use zvariant::{Signature, Type};

use super::super::{
    connection::bond::{NmSettingBond, NmSettingBondPort},
    connection::bridge::{NmSettingBridge, NmSettingBridgePort},
    connection::ethtool::NmSettingEthtool,
    connection::ieee8021x::NmSetting8021X,
    connection::infiniband::NmSettingInfiniBand,
    connection::ip::NmSettingIp,
    connection::loopback::NmSettingLoopback,
    connection::mac_vlan::NmSettingMacVlan,
    connection::macsec::NmSettingMacSec,
    connection::ovs::{
        NmSettingOvsBridge, NmSettingOvsDpdk, NmSettingOvsExtIds,
        NmSettingOvsIface, NmSettingOvsOtherConfig, NmSettingOvsPatch,
        NmSettingOvsPort,
    },
    connection::sriov::NmSettingSriov,
    connection::user::NmSettingUser,
    connection::veth::NmSettingVeth,
    connection::vlan::NmSettingVlan,
    connection::vpn::NmSettingVpn,
    connection::vrf::NmSettingVrf,
    connection::vxlan::NmSettingVxlan,
    connection::wired::NmSettingWired,
    convert::ToDbusValue,
    NmError,
};

const NM_AUTOCONENCT_PORT_DEFAULT: i32 = -1;
const NM_AUTOCONENCT_PORT_YES: i32 = 1;
const NM_AUTOCONENCT_PORT_NO: i32 = 0;

pub(crate) type NmConnectionDbusOwnedValue =
    HashMap<String, HashMap<String, zvariant::OwnedValue>>;

pub(crate) type DbusDictionary = HashMap<String, zvariant::OwnedValue>;

#[cfg(feature = "query_apply")]
pub(crate) type NmConnectionDbusValue<'a> =
    HashMap<&'a str, HashMap<&'a str, zvariant::Value<'a>>>;

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum NmSettingsConnectionFlag {
    Unsaved = 1,
    NmGenerated = 2,
    Volatile = 4,
    External = 8,
}

#[cfg(feature = "query_apply")]
fn from_u32_to_vec_nm_conn_flags(i: u32) -> Vec<NmSettingsConnectionFlag> {
    let mut ret = Vec::new();
    if i & NmSettingsConnectionFlag::Unsaved as u32 > 0 {
        ret.push(NmSettingsConnectionFlag::Unsaved);
    }
    if i & NmSettingsConnectionFlag::NmGenerated as u32 > 0 {
        ret.push(NmSettingsConnectionFlag::NmGenerated);
    }
    if i & NmSettingsConnectionFlag::Volatile as u32 > 0 {
        ret.push(NmSettingsConnectionFlag::Volatile);
    }
    if i & NmSettingsConnectionFlag::External as u32 > 0 {
        ret.push(NmSettingsConnectionFlag::External);
    }
    ret
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(try_from = "NmConnectionDbusOwnedValue")]
#[non_exhaustive]
pub struct NmConnection {
    pub connection: Option<NmSettingConnection>,
    pub bond: Option<NmSettingBond>,
    pub bond_port: Option<NmSettingBondPort>,
    pub bridge: Option<NmSettingBridge>,
    pub bridge_port: Option<NmSettingBridgePort>,
    pub ipv4: Option<NmSettingIp>,
    pub ipv6: Option<NmSettingIp>,
    pub ovs_bridge: Option<NmSettingOvsBridge>,
    pub ovs_port: Option<NmSettingOvsPort>,
    pub ovs_iface: Option<NmSettingOvsIface>,
    pub ovs_ext_ids: Option<NmSettingOvsExtIds>,
    pub ovs_other_config: Option<NmSettingOvsOtherConfig>,
    pub ovs_patch: Option<NmSettingOvsPatch>,
    pub ovs_dpdk: Option<NmSettingOvsDpdk>,
    pub wired: Option<NmSettingWired>,
    pub vlan: Option<NmSettingVlan>,
    pub vxlan: Option<NmSettingVxlan>,
    pub mac_vlan: Option<NmSettingMacVlan>,
    pub sriov: Option<NmSettingSriov>,
    pub vrf: Option<NmSettingVrf>,
    pub veth: Option<NmSettingVeth>,
    pub ieee8021x: Option<NmSetting8021X>,
    pub user: Option<NmSettingUser>,
    pub ethtool: Option<NmSettingEthtool>,
    pub infiniband: Option<NmSettingInfiniBand>,
    pub loopback: Option<NmSettingLoopback>,
    pub macsec: Option<NmSettingMacSec>,
    pub vpn: Option<NmSettingVpn>,
    #[serde(skip)]
    pub obj_path: String,
    #[serde(skip)]
    pub(crate) flags: Vec<NmSettingsConnectionFlag>,
    _other: HashMap<String, HashMap<String, zvariant::OwnedValue>>,
}

// The signature is the same as the NmConnectionDbusOwnedValue because we are
// going through the try_from
impl Type for NmConnection {
    fn signature() -> Signature<'static> {
        NmConnectionDbusOwnedValue::signature()
    }
}

impl TryFrom<NmConnectionDbusOwnedValue> for NmConnection {
    type Error = NmError;
    fn try_from(
        mut v: NmConnectionDbusOwnedValue,
    ) -> Result<Self, Self::Error> {
        Ok(Self {
            connection: _from_map!(
                v,
                "connection",
                NmSettingConnection::try_from
            )?,
            ipv4: _from_map!(v, "ipv4", NmSettingIp::try_from)?,
            ipv6: _from_map!(v, "ipv6", NmSettingIp::try_from)?,
            bond: _from_map!(v, "bond", NmSettingBond::try_from)?,
            bond_port: _from_map!(v, "bond-port", NmSettingBondPort::try_from)?,
            bridge: _from_map!(v, "bridge", NmSettingBridge::try_from)?,
            bridge_port: _from_map!(
                v,
                "bridge-port",
                NmSettingBridgePort::try_from
            )?,
            ovs_bridge: _from_map!(
                v,
                "ovs-bridge",
                NmSettingOvsBridge::try_from
            )?,
            ovs_port: _from_map!(v, "ovs-port", NmSettingOvsPort::try_from)?,
            ovs_iface: _from_map!(
                v,
                "ovs-interface",
                NmSettingOvsIface::try_from
            )?,
            ovs_ext_ids: _from_map!(
                v,
                "ovs-external-ids",
                NmSettingOvsExtIds::try_from
            )?,
            ovs_other_config: _from_map!(
                v,
                "ovs-other-config",
                NmSettingOvsOtherConfig::try_from
            )?,
            ovs_patch: _from_map!(v, "ovs-patch", NmSettingOvsPatch::try_from)?,
            ovs_dpdk: _from_map!(v, "ovs-dpdk", NmSettingOvsDpdk::try_from)?,
            wired: _from_map!(v, "802-3-ethernet", NmSettingWired::try_from)?,
            vlan: _from_map!(v, "vlan", NmSettingVlan::try_from)?,
            vxlan: _from_map!(v, "vxlan", NmSettingVxlan::try_from)?,
            sriov: _from_map!(v, "sriov", NmSettingSriov::try_from)?,
            mac_vlan: _from_map!(v, "macvlan", NmSettingMacVlan::try_from)?,
            macsec: _from_map!(v, "macsec", NmSettingMacSec::try_from)?,
            vrf: _from_map!(v, "vrf", NmSettingVrf::try_from)?,
            veth: _from_map!(v, "veth", NmSettingVeth::try_from)?,
            ieee8021x: _from_map!(v, "802-1x", NmSetting8021X::try_from)?,
            user: _from_map!(v, "user", NmSettingUser::try_from)?,
            ethtool: _from_map!(v, "ethtool", NmSettingEthtool::try_from)?,
            infiniband: _from_map!(
                v,
                "infiniband",
                NmSettingInfiniBand::try_from
            )?,
            loopback: _from_map!(v, "loopback", NmSettingLoopback::try_from)?,
            vpn: _from_map!(v, "vpn", NmSettingVpn::try_from)?,
            _other: v,
            ..Default::default()
        })
    }
}

impl NmConnection {
    pub fn iface_name(&self) -> Option<&str> {
        _connection_inner_string_member!(self, iface_name)
    }

    pub fn iface_type(&self) -> Option<&str> {
        _connection_inner_string_member!(self, iface_type)
    }

    pub fn id(&self) -> Option<&str> {
        _connection_inner_string_member!(self, id)
    }

    pub fn controller(&self) -> Option<&str> {
        _connection_inner_string_member!(self, controller)
    }

    pub fn controller_type(&self) -> Option<&str> {
        _connection_inner_string_member!(self, controller_type)
    }

    #[cfg(feature = "query_apply")]
    pub(crate) fn to_value(&self) -> Result<NmConnectionDbusValue, NmError> {
        let mut ret = HashMap::new();
        if let Some(con_set) = &self.connection {
            ret.insert("connection", con_set.to_value()?);
        }
        if let Some(bond_set) = &self.bond {
            ret.insert("bond", bond_set.to_value()?);
        }
        if let Some(br_set) = &self.bridge {
            ret.insert("bridge", br_set.to_value()?);
        }
        if let Some(br_port_set) = &self.bridge_port {
            ret.insert("bridge-port", br_port_set.to_value()?);
        }
        if let Some(ipv4_set) = &self.ipv4 {
            ret.insert("ipv4", ipv4_set.to_value()?);
        }
        if let Some(ipv6_set) = &self.ipv6 {
            ret.insert("ipv6", ipv6_set.to_value()?);
        }
        if let Some(ovs_bridge_set) = &self.ovs_bridge {
            ret.insert("ovs-bridge", ovs_bridge_set.to_value()?);
        }
        if let Some(ovs_port_set) = &self.ovs_port {
            ret.insert("ovs-port", ovs_port_set.to_value()?);
        }
        if let Some(ovs_iface_set) = &self.ovs_iface {
            ret.insert("ovs-interface", ovs_iface_set.to_value()?);
        }
        if let Some(ovs_ext_ids) = &self.ovs_ext_ids {
            ret.insert("ovs-external-ids", ovs_ext_ids.to_value()?);
        }
        if let Some(ovs_other_config) = &self.ovs_other_config {
            ret.insert("ovs-other-config", ovs_other_config.to_value()?);
        }
        if let Some(ovs_patch_set) = &self.ovs_patch {
            ret.insert("ovs-patch", ovs_patch_set.to_value()?);
        }
        if let Some(ovs_dpdk_set) = &self.ovs_dpdk {
            ret.insert("ovs-dpdk", ovs_dpdk_set.to_value()?);
        }
        if let Some(wired_set) = &self.wired {
            ret.insert("802-3-ethernet", wired_set.to_value()?);
        }
        if let Some(vlan) = &self.vlan {
            ret.insert("vlan", vlan.to_value()?);
        }
        if let Some(vxlan) = &self.vxlan {
            ret.insert("vxlan", vxlan.to_value()?);
        }
        if let Some(sriov) = &self.sriov {
            ret.insert("sriov", sriov.to_value()?);
        }
        if let Some(mac_vlan) = &self.mac_vlan {
            ret.insert("macvlan", mac_vlan.to_value()?);
        }
        if let Some(macsec) = &self.macsec {
            ret.insert("macsec", macsec.to_value()?);
        }
        if let Some(vrf) = &self.vrf {
            ret.insert("vrf", vrf.to_value()?);
        }
        if let Some(veth) = &self.veth {
            ret.insert("veth", veth.to_value()?);
        }
        if let Some(v) = &self.ieee8021x {
            ret.insert("802-1x", v.to_value()?);
        }
        if let Some(v) = &self.user {
            ret.insert("user", v.to_value()?);
        }
        if let Some(v) = &self.ethtool {
            ret.insert("ethtool", v.to_value()?);
        }
        if let Some(v) = &self.infiniband {
            ret.insert("infiniband", v.to_value()?);
        }
        if let Some(v) = &self.loopback {
            ret.insert("loopback", v.to_value()?);
        }
        if let Some(v) = &self.bond_port {
            ret.insert("bond-port", v.to_value()?);
        }
        if let Some(v) = &self.vpn {
            ret.insert("vpn", v.to_value()?);
        }
        for (key, setting_value) in &self._other {
            let mut other_setting_value: HashMap<&str, zvariant::Value> =
                HashMap::new();
            for (sub_key, sub_value) in setting_value {
                other_setting_value.insert(
                    sub_key.as_str(),
                    zvariant::Value::from(sub_value.clone()),
                );
            }
            ret.insert(key, other_setting_value);
        }
        Ok(ret)
    }

    pub fn set_parent(&mut self, parent: &str) {
        if let Some(setting) = self.vlan.as_mut() {
            setting.parent = Some(parent.to_string());
        }
        if let Some(setting) = self.vxlan.as_mut() {
            setting.parent = Some(parent.to_string());
        }
        if let Some(setting) = self.infiniband.as_mut() {
            setting.parent = Some(parent.to_string());
        }
        if let Some(setting) = self.mac_vlan.as_mut() {
            setting.parent = Some(parent.to_string());
        }
        if let Some(setting) = self.macsec.as_mut() {
            setting.parent = Some(parent.to_string());
        }
    }

    pub fn uuid(&self) -> Option<&str> {
        if let Some(nm_conn_set) = &self.connection {
            if let Some(ref uuid) = nm_conn_set.uuid {
                return Some(uuid);
            }
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(try_from = "DbusDictionary")]
#[non_exhaustive]
pub struct NmSettingConnection {
    pub id: Option<String>,
    pub uuid: Option<String>,
    pub iface_type: Option<String>,
    pub iface_name: Option<String>,
    pub controller: Option<String>,
    pub controller_type: Option<String>,
    pub autoconnect: Option<bool>,
    pub autoconnect_ports: Option<bool>,
    pub lldp: Option<bool>,
    pub mptcp_flags: Option<u32>,
    _other: HashMap<String, zvariant::OwnedValue>,
}

impl TryFrom<DbusDictionary> for NmSettingConnection {
    type Error = NmError;
    fn try_from(mut v: DbusDictionary) -> Result<Self, Self::Error> {
        Ok(Self {
            id: _from_map!(v, "id", String::try_from)?,
            uuid: _from_map!(v, "uuid", String::try_from)?,
            iface_type: _from_map!(v, "type", String::try_from)?,
            iface_name: _from_map!(v, "interface-name", String::try_from)?,
            controller: _from_map!(v, "master", String::try_from)?,
            controller_type: _from_map!(v, "slave-type", String::try_from)?,
            autoconnect: _from_map!(v, "autoconnect", bool::try_from)?
                .or(Some(true)),
            autoconnect_ports: NmSettingConnection::i32_to_autoconnect_ports(
                _from_map!(v, "autoconnect-slaves", i32::try_from)?,
            ),
            lldp: _from_map!(v, "lldp", i32::try_from)?.map(|i| i == 1),
            mptcp_flags: _from_map!(v, "mptcp-flags", u32::try_from)?,
            _other: v,
        })
    }
}

impl ToDbusValue for NmSettingConnection {
    fn to_value(&self) -> Result<HashMap<&str, zvariant::Value>, NmError> {
        let mut ret = HashMap::new();
        if let Some(v) = &self.id {
            ret.insert("id", zvariant::Value::new(v.as_str()));
        }
        if let Some(v) = &self.uuid {
            ret.insert("uuid", zvariant::Value::new(v.as_str()));
        }
        if let Some(v) = &self.iface_type {
            ret.insert("type", zvariant::Value::new(v.as_str()));
        }
        if let Some(v) = &self.iface_name {
            ret.insert("interface-name", zvariant::Value::new(v.as_str()));
        }
        if let Some(v) = &self.controller {
            ret.insert("master", zvariant::Value::new(v.as_str()));
        }
        if let Some(v) = &self.controller_type {
            ret.insert("slave-type", zvariant::Value::new(v.as_str()));
        }
        if let Some(v) = &self.lldp {
            ret.insert("lldp", zvariant::Value::new(v));
        }
        if let Some(v) = &self.mptcp_flags {
            ret.insert("mptcp-flags", zvariant::Value::new(v));
        }

        ret.insert(
            "autoconnect",
            if let Some(false) = &self.autoconnect {
                zvariant::Value::new(false)
            } else {
                zvariant::Value::new(true)
            },
        );
        ret.insert(
            "autoconnect-slaves",
            match &self.autoconnect_ports {
                Some(true) => zvariant::Value::new(NM_AUTOCONENCT_PORT_YES),
                Some(false) => zvariant::Value::new(NM_AUTOCONENCT_PORT_NO),
                None => zvariant::Value::new(NM_AUTOCONENCT_PORT_DEFAULT),
            },
        );
        ret.extend(self._other.iter().map(|(key, value)| {
            (key.as_str(), zvariant::Value::from(value.clone()))
        }));
        Ok(ret)
    }
}

impl NmSettingConnection {
    fn i32_to_autoconnect_ports(val: Option<i32>) -> Option<bool> {
        match val {
            Some(NM_AUTOCONENCT_PORT_YES) => Some(true),
            Some(NM_AUTOCONENCT_PORT_NO) => Some(false),
            Some(v) => {
                warn!("Unknown autoconnect-ports value {}", v);
                None
            }
            // For autoconnect, None means true
            None => Some(true),
        }
    }
}

#[cfg(feature = "query_apply")]
pub(crate) fn nm_con_get_from_obj_path(
    dbus_con: &zbus::Connection,
    con_obj_path: &str,
) -> Result<NmConnection, NmError> {
    let proxy = zbus::Proxy::new(
        dbus_con,
        super::super::dbus::NM_DBUS_INTERFACE_ROOT,
        con_obj_path,
        super::super::dbus::NM_DBUS_INTERFACE_SETTING,
    )?;
    let mut nm_conn = proxy.call::<(), NmConnection>("GetSettings", &())?;
    nm_conn.obj_path = con_obj_path.to_string();
    if let Some(ieee_8021x_conf) = nm_conn.ieee8021x.as_mut() {
        if let Ok(nm_secrets) = proxy
            .call::<&str, NmConnectionDbusOwnedValue>("GetSecrets", &"802-1x")
        {
            if let Some(nm_secret) = nm_secrets.get("802-1x") {
                ieee_8021x_conf.fill_secrets(nm_secret);
            }
        }
    }
    if let Some(macsec_conf) = nm_conn.macsec.as_mut() {
        if let Ok(nm_secrets) = proxy
            .call::<&str, NmConnectionDbusOwnedValue>("GetSecrets", &"macsec")
        {
            if let Some(nm_secret) = nm_secrets.get("macsec") {
                macsec_conf.fill_secrets(nm_secret);
            }
        }
    }
    if let Some(vpn_conf) = nm_conn.vpn.as_mut() {
        if let Ok(nm_secrets) =
            proxy.call::<&str, NmConnectionDbusOwnedValue>("GetSecrets", &"vpn")
        {
            if let Some(nm_secret) = nm_secrets.get("vpn") {
                vpn_conf.fill_secrets(nm_secret);
            }
        }
    }
    if let Ok(flags) = proxy.get_property::<u32>("Flags") {
        nm_conn.flags = from_u32_to_vec_nm_conn_flags(flags);
    }
    Ok(nm_conn)
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
#[serde(try_from = "DbusDictionary")]
#[non_exhaustive]
pub struct NmRange {
    pub start: u64,
    pub end: u64,
    _other: DbusDictionary,
}

impl TryFrom<DbusDictionary> for NmRange {
    type Error = NmError;
    fn try_from(mut v: DbusDictionary) -> Result<Self, Self::Error> {
        Ok(Self {
            start: _from_map!(v, "start", u64::try_from)?.unwrap_or_default(),
            end: _from_map!(v, "end", u64::try_from)?.unwrap_or_default(),
            _other: v,
        })
    }
}

impl NmRange {
    pub fn to_value(&self) -> Result<zvariant::Value, NmError> {
        let mut ret = zvariant::Dict::new(
            zvariant::Signature::from_str_unchecked("s"),
            zvariant::Signature::from_str_unchecked("v"),
        );
        ret.append(
            zvariant::Value::new("start"),
            zvariant::Value::new(zvariant::Value::U64(self.start)),
        )?;
        ret.append(
            zvariant::Value::new("end"),
            zvariant::Value::new(zvariant::Value::U64(self.end)),
        )?;
        Ok(zvariant::Value::Dict(ret))
    }
}
