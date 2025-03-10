// SPDX-License-Identifier: Apache-2.0

use crate::{DnsState, ErrorKind, MergedDnsState};

#[test]
fn test_dns_verify_uncompressed_srvs() {
    let current: DnsState = serde_yaml::from_str(
        r"---
        config:
          server:
          - '3000::'
          - ::100
          - 0:0:f::100
          - 3001:db8:0:1:1:1:1:1
          - 3001:db8::2:1
          - 3001:db8::1
          - 3002:0:0:1::1
          - 3003:db8::1:0:0:1
          - 3004:db8::1:0:0:1
          - 3005:db8::1:0:0:1
          - ::ffff:192.0.2.1
          - ::ffff:192.0.2.2
          - 3::4
        ",
    )
    .unwrap();

    let desired: DnsState = serde_yaml::from_str(
        r"---
        config:
          server:
          - 3000:0000:0000:0000:0000:0000:0000:0000
          - 0000:0000:0000:0000:0000:0000:0000:0100
          - 0000:0000:000F:0000:0000:0000:0000:0100
          - 3001:db8::1:1:1:1:1
          - 3001:db8:0:0:0:0:2:1
          - 3001:db8::0:1
          - '3002:0:0:1:0:0:0:1'
          - 3003:dB8:0:0:1:0:0:1
          - 3004:db8::1:0:0:1
          - 3005:DB8:0:0:1::1
          - 0:0:0:0:0:FFFF:192.0.2.1
          - ::FFFF:192.0.2.2
          - 03:0000:000:00:0::4
        ",
    )
    .unwrap();

    let merged = MergedDnsState::new(desired, DnsState::new()).unwrap();

    merged.verify(&current).unwrap();
}

#[test]
fn test_dns_option_with_value() {
    let mut desired: DnsState = serde_yaml::from_str(
        r"---
        config:
          options:
          - rotate
          - ndots:9
        ",
    )
    .unwrap();
    desired.sanitize().unwrap();
}

#[test]
fn test_invalid_dns_option_with_value() {
    let mut desired: DnsState = serde_yaml::from_str(
        r"---
        config:
          options:
          - rotate
          - ndot:9
        ",
    )
    .unwrap();
    let result = desired.sanitize();
    assert!(result.is_err());

    if let Err(e) = result {
        assert_eq!(e.kind(), ErrorKind::InvalidArgument);
    }
}

#[test]
fn test_is_purge_dns_empty_dict() {
    let desired: DnsState = serde_yaml::from_str(
        r"---
        config: {}
        ",
    )
    .unwrap();
    assert!(desired.config.unwrap().is_purge());
}

#[test]
fn test_is_purge_dns_full_empty_dict() {
    let desired: DnsState = serde_yaml::from_str(
        r"---
        config:
          server: []
          search: []
          options: []
        ",
    )
    .unwrap();
    assert!(desired.config.unwrap().is_purge());
}

#[test]
fn test_not_purge() {
    let desired: DnsState = serde_yaml::from_str(
        r"---
        config:
          search: []
        ",
    )
    .unwrap();
    assert!(!desired.config.unwrap().is_purge());
}
