// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use chatos_plugin_management_sdk::parse_plugin_manifest;

#[test]
fn documented_third_party_plugin_manifests_parse() {
    for (name, raw) in [
        (
            "circleci",
            include_str!("../../../docs/plugins/examples/circleci-plugin.manifest.json"),
        ),
        (
            "sentry",
            include_str!("../../../docs/plugins/examples/sentry-plugin.manifest.json"),
        ),
        (
            "build-web",
            include_str!("../../../docs/plugins/examples/build-web-plugin.manifest.json"),
        ),
    ] {
        let manifest = parse_plugin_manifest(raw)
            .unwrap_or_else(|error| panic!("{name} example manifest should parse: {error}"));
        assert_eq!(manifest.name, name);
        assert!(
            !manifest.mcp_servers.is_empty(),
            "{name} should demonstrate an adapter MCP component",
        );
        assert!(
            !manifest.permissions.is_empty(),
            "{name} should document component-scoped permissions",
        );
    }
}
