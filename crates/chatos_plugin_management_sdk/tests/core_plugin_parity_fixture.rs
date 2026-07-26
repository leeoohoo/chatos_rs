// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::HashSet;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct FixtureFile {
    schema_version: u32,
    plugins: Vec<PluginFixture>,
}

#[derive(Debug, Deserialize)]
struct PluginFixture {
    plugin: String,
    wave: String,
    license_class: String,
    redistribution_review: String,
    required_components: Vec<String>,
    acceptance: Vec<String>,
}

#[test]
fn core_plugin_parity_fixture_covers_the_frozen_thirteen() {
    let fixture: FixtureFile =
        serde_json::from_str(include_str!("fixtures/core_plugin_parity_v1.json"))
            .expect("fixture JSON should be valid");
    assert_eq!(fixture.schema_version, 1);
    assert_eq!(fixture.plugins.len(), 13);

    let expected = HashSet::from([
        "documents",
        "pdf",
        "spreadsheets",
        "presentations",
        "template-creator",
        "remotion",
        "visualize",
        "browser",
        "chrome",
        "computer-use",
        "figma",
        "game-studio",
        "chatos-security",
    ]);
    let actual = fixture
        .plugins
        .iter()
        .map(|item| item.plugin.as_str())
        .collect::<HashSet<_>>();
    assert_eq!(actual, expected);

    for plugin in fixture.plugins {
        assert!(matches!(
            plugin.wave.as_str(),
            "A" | "B" | "C" | "D" | "E" | "F"
        ));
        assert!(matches!(
            plugin.license_class.as_str(),
            "open_source" | "proprietary_reimplementation" | "restricted_api_integration"
        ));
        assert_eq!(plugin.redistribution_review, "pending");
        assert!(!plugin.required_components.is_empty());
        assert!(!plugin.acceptance.is_empty());
    }
}
