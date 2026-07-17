// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use super::*;

pub(super) fn merge_custom_profile(
    lower: &mut CustomPermissionProfile,
    higher: CustomPermissionProfile,
) {
    lower.description = higher.description.or(lower.description.take());
    lower.extends = higher.extends.or(lower.extends.take());
    lower.workspace_roots.extend(higher.workspace_roots);
    lower.file_system = merge_optional_file_system(lower.file_system.take(), higher.file_system);
    lower.network = merge_optional_network(lower.network.take(), higher.network);
}

pub(super) fn merge_optional_file_system(
    lower: Option<AdditionalFileSystemPermissions>,
    higher: Option<AdditionalFileSystemPermissions>,
) -> Option<AdditionalFileSystemPermissions> {
    match (lower, higher) {
        (Some(lower), Some(higher)) => {
            let mut entries = lower.normalized_entries();
            for entry in higher.normalized_entries() {
                entries.retain(|existing| existing.path != entry.path);
                entries.push(entry);
            }
            Some(AdditionalFileSystemPermissions {
                entries: Some(entries),
                glob_scan_max_depth: higher.glob_scan_max_depth.or(lower.glob_scan_max_depth),
                ..Default::default()
            })
        }
        (Some(lower), None) => Some(lower),
        (None, Some(higher)) => Some(higher),
        (None, None) => None,
    }
}

pub(super) fn merge_optional_network(
    lower: Option<NetworkRequirements>,
    higher: Option<NetworkRequirements>,
) -> Option<NetworkRequirements> {
    match (lower, higher) {
        (Some(lower), Some(higher)) => Some(NetworkRequirements {
            enabled: higher.enabled.or(lower.enabled),
            domains: merge_optional_map(lower.domains, higher.domains),
            unix_sockets: merge_optional_map(lower.unix_sockets, higher.unix_sockets),
            allow_local_binding: higher.allow_local_binding.or(lower.allow_local_binding),
            allow_upstream_proxy: higher.allow_upstream_proxy.or(lower.allow_upstream_proxy),
            mode: higher.mode.or(lower.mode),
            enable_socks5: higher.enable_socks5.or(lower.enable_socks5),
            enable_socks5_udp: higher.enable_socks5_udp.or(lower.enable_socks5_udp),
            dangerously_allow_all_unix_sockets: higher
                .dangerously_allow_all_unix_sockets
                .or(lower.dangerously_allow_all_unix_sockets),
            dangerously_allow_non_loopback_proxy: higher
                .dangerously_allow_non_loopback_proxy
                .or(lower.dangerously_allow_non_loopback_proxy),
            managed_allowed_domains_only: higher
                .managed_allowed_domains_only
                .or(lower.managed_allowed_domains_only),
            http_port: higher.http_port.or(lower.http_port),
            socks_port: higher.socks_port.or(lower.socks_port),
            allowed_domains: higher.allowed_domains.or(lower.allowed_domains),
            denied_domains: higher.denied_domains.or(lower.denied_domains),
            allow_unix_sockets: higher.allow_unix_sockets.or(lower.allow_unix_sockets),
        }),
        (Some(lower), None) => Some(lower),
        (None, Some(higher)) => Some(higher),
        (None, None) => None,
    }
}

pub(super) fn merge_optional_map<K: Ord, V>(
    lower: Option<BTreeMap<K, V>>,
    higher: Option<BTreeMap<K, V>>,
) -> Option<BTreeMap<K, V>> {
    match (lower, higher) {
        (Some(mut lower), Some(higher)) => {
            lower.extend(higher);
            Some(lower)
        }
        (Some(lower), None) => Some(lower),
        (None, Some(higher)) => Some(higher),
        (None, None) => None,
    }
}
