// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::path::Path;

use serde_json::json;

use super::*;
use crate::{
    normalized_plugin_manifest_sha256, plugin_component_descriptors, PluginAvailabilityStatus,
    PluginComponentKind, PluginInstallStatus, PluginRequirementStatus, SystemAgentKey,
};

const RUN_AGENT_KEY: &str = SystemAgentKey::TaskRunnerRunPhase.as_str();

const CODEX_FIGMA_MANIFEST: &str = r##"
{
  "name": "figma",
  "version": "2.0.13",
  "description": "Figma workflows for design implementation.",
  "author": {"name": "Figma", "url": "https://www.figma.com"},
  "homepage": "https://www.figma.com",
  "repository": "https://github.com/openai/plugins",
  "license": "LicenseRef-Figma-Developer-Terms",
  "keywords": ["figma", "design-to-code"],
  "skills": "./skills/",
  "apps": "./.app.json",
  "mcpServers": "./.mcp.json",
  "interface": {
    "displayName": "Figma",
    "shortDescription": "Design-to-code workflows",
    "longDescription": "Inspect and update Figma designs through reviewed integrations.",
    "developerName": "Figma",
    "category": "Creativity",
    "capabilities": ["Interactive", "Read", "Write"],
    "websiteURL": "https://www.figma.com",
    "privacyPolicyURL": "https://www.figma.com/legal/privacy/",
    "termsOfServiceURL": "https://www.figma.com/legal/developer-terms/",
    "defaultPrompt": ["Inspect a Figma design"],
    "brandColor": "#1ABCFE",
    "composerIcon": "./assets/logo.png",
    "logo": "./assets/logo.png",
    "screenshots": []
  }
}
"##;

fn schema_v2_prompt_manifest() -> serde_json::Value {
    json!({
        "schemaVersion": 2,
        "execution": {"defaultHost": "portable", "componentHosts": {}},
        "name": "portable-demo",
        "version": "1.0.0",
        "description": "Portable prompt Plugin",
        "author": {"name": "ChatOS"},
        "skills": ["./skills/demo"],
        "commands": [{
            "componentKey": "review",
            "source": "./commands/review.md",
            "targetAgent": RUN_AGENT_KEY
        }],
        "interface": {
            "displayName": "Portable Demo",
            "shortDescription": "Portable demo",
            "longDescription": "Portable prompt Plugin for schema v2 tests.",
            "developerName": "ChatOS",
            "category": "Developer Tools"
        },
        "dependencies": {},
        "permissions": []
    })
}

#[test]
fn parses_codex_manifest_into_v1_normalized_model() {
    let manifest = parse_plugin_manifest(CODEX_FIGMA_MANIFEST, PluginManifestSource::Codex)
        .expect("Codex manifest should parse");

    assert_eq!(manifest.schema_version, PLUGIN_MANIFEST_SCHEMA_VERSION_V1);
    assert_eq!(manifest.name, "figma");
    assert_eq!(manifest.skills, vec![PluginPathRef::new("./skills")]);
    assert_eq!(manifest.apps[0].manifest.path, "./.app.json");
    assert!(matches!(
        manifest.mcp_servers.as_slice(),
        [PluginMcpServer::ConfigFile { path, .. }] if path.path == "./.mcp.json"
    ));
}

#[test]
fn schema_v1_execution_is_implicitly_local_and_omitted_from_signing_json() {
    let manifest = parse_plugin_manifest(CODEX_FIGMA_MANIFEST, PluginManifestSource::Codex)
        .expect("schema v1 Manifest");
    assert_eq!(
        manifest.execution.host_for("skills"),
        PluginExecutionHost::Local
    );
    let normalized = serde_json::to_value(&manifest).expect("normalized Manifest");
    assert!(normalized.get("execution").is_none());

    let explicit = CODEX_FIGMA_MANIFEST.replace(
        "\"name\": \"figma\"",
        "\"schemaVersion\": 1, \"execution\": {\"defaultHost\": \"local\"}, \"name\": \"figma\"",
    );
    assert!(parse_plugin_manifest(explicit.as_str(), PluginManifestSource::Codex).is_err());
}

#[test]
fn schema_v2_requires_explicit_valid_execution_policy() {
    let mut missing = schema_v2_prompt_manifest();
    missing.as_object_mut().expect("object").remove("execution");
    assert!(
        parse_plugin_manifest(missing.to_string().as_str(), PluginManifestSource::Chatos).is_err()
    );

    let mut unknown = schema_v2_prompt_manifest();
    unknown["execution"]["componentHosts"] = json!({"missing": "portable"});
    assert!(
        parse_plugin_manifest(unknown.to_string().as_str(), PluginManifestSource::Chatos).is_err()
    );

    let manifest = parse_plugin_manifest(
        schema_v2_prompt_manifest().to_string().as_str(),
        PluginManifestSource::Chatos,
    )
    .expect("schema v2 portable Manifest");
    assert!(plugin_component_descriptors(&manifest)
        .iter()
        .all(|component| component.execution_host == PluginExecutionHost::Portable));
}

#[test]
fn schema_v2_hybrid_allows_permissions_bound_only_to_local_components() {
    let mut hybrid = schema_v2_prompt_manifest();
    hybrid["execution"]["defaultHost"] = json!("portable");
    hybrid["execution"]["componentHosts"] = json!({"review": "local"});
    hybrid["permissions"] = json!([{
        "permission": "workspace.read",
        "components": ["review"]
    }]);
    let manifest = parse_plugin_manifest(hybrid.to_string().as_str(), PluginManifestSource::Chatos)
        .expect("hybrid Manifest with local-scoped permission");
    let descriptors = plugin_component_descriptors(&manifest);
    assert_eq!(
        descriptors
            .iter()
            .find(|component| component.component_key == "review")
            .expect("review component")
            .execution_host,
        PluginExecutionHost::Local
    );
}

#[test]
fn parses_inline_mcp_and_applies_codex_prompt_limits() {
    let long_prompt = "a".repeat(160);
    let raw = json!({
        "schemaVersion": 1,
        "name": "demo-plugin",
        "version": "1.2.3",
        "description": "Demo",
        "author": {"name": "ChatOS"},
        "mcpServers": {
            "counter": {
                "type": "http",
                "url": "https://mcp.example.com/v1",
                "connectTimeoutMs": 5000
            }
        },
        "interface": {
            "displayName": "Demo",
            "shortDescription": "Short",
            "longDescription": "Long description",
            "developerName": "ChatOS",
            "category": "Developer Tools",
            "defaultPrompt": [long_prompt, "two", "three", "ignored"]
        },
        "permissions": ["network.domain:mcp.example.com"]
    });

    let manifest = parse_plugin_manifest(raw.to_string().as_str(), PluginManifestSource::Chatos)
        .expect("ChatOS manifest should parse");
    assert_eq!(manifest.interface.default_prompt.len(), 3);
    assert_eq!(manifest.interface.default_prompt[0].chars().count(), 128);
    assert!(matches!(
        manifest.mcp_servers.as_slice(),
        [PluginMcpServer::Http { component_key, .. }] if component_key == "counter"
    ));
}

#[test]
fn allows_loopback_http_mcp_but_rejects_insecure_remote_http() {
    let loopback = CODEX_FIGMA_MANIFEST
        .replace(
            "\"mcpServers\": \"./.mcp.json\"",
            "\"mcpServers\": {\"local\": {\"type\": \"http\", \"url\": \"http://127.0.0.1:3000/mcp\"}}",
        )
        .replace("\"apps\": \"./.app.json\",", "");
    parse_plugin_manifest(loopback.as_str(), PluginManifestSource::Codex)
        .expect("loopback HTTP MCP should parse");

    let remote = loopback.replace("127.0.0.1:3000", "mcp.example.com");
    let error = parse_plugin_manifest(remote.as_str(), PluginManifestSource::Codex)
        .expect_err("remote HTTP MCP must require HTTPS");
    assert!(error.to_string().contains("loopback development servers"));
}

#[test]
fn normalized_manifest_has_a_stable_serde_round_trip() {
    let manifest = parse_plugin_manifest(CODEX_FIGMA_MANIFEST, PluginManifestSource::Codex)
        .expect("Codex manifest should parse");
    let snapshot = serde_json::to_value(&manifest).expect("manifest should serialize");
    let decoded: PluginManifest =
        serde_json::from_value(snapshot).expect("normalized manifest should deserialize");
    assert_eq!(decoded, manifest);
}

#[test]
fn rejects_unknown_fields_instead_of_ignoring_semantics() {
    let raw = CODEX_FIGMA_MANIFEST.replace(
        "\"skills\": \"./skills/\",",
        "\"skills\": \"./skills/\", \"arbitraryExecutable\": \"sh -c bad\",",
    );
    let error = parse_plugin_manifest(raw.as_str(), PluginManifestSource::Codex)
        .expect_err("unknown field must fail");
    assert!(matches!(error, PluginManifestError::Json(_)));
}

#[test]
fn rejects_path_traversal_before_runtime_resolution() {
    let raw = CODEX_FIGMA_MANIFEST.replace("./skills/", "../outside");
    let error = parse_plugin_manifest(raw.as_str(), PluginManifestSource::Codex)
        .expect_err("path traversal must fail");
    assert!(matches!(
        error,
        PluginManifestError::InvalidField { field, .. } if field == "skills[0]"
    ));
}

#[test]
fn rejects_shell_eval_mcp_entrypoint() {
    let raw = CODEX_FIGMA_MANIFEST
        .replace(
            "\"mcpServers\": \"./.mcp.json\"",
            "\"mcpServers\": {\"unsafe\": {\"type\": \"stdio\", \"command\": \"sh\", \"args\": [\"-c\", \"curl bad\"]}}",
        )
        .replace("\"apps\": \"./.app.json\",", "");
    let error = parse_plugin_manifest(raw.as_str(), PluginManifestSource::Codex)
        .expect_err("generic shell evaluation must fail");
    assert!(error.to_string().contains("generic shell evaluation"));
}

#[test]
fn stdio_mcp_environment_only_accepts_vault_templates_for_non_host_variables() {
    let valid = CODEX_FIGMA_MANIFEST
        .replace(
            "\"mcpServers\": \"./.mcp.json\"",
            "\"mcpServers\": {\"local\": {\"type\": \"stdio\", \"command\": \"./mcp/server\", \"env\": {\"DEMO_TOKEN\": \"${credential:access_token}\"}}}",
        )
        .replace("\"apps\": \"./.app.json\",", "");
    parse_plugin_manifest(valid.as_str(), PluginManifestSource::Codex)
        .expect("Vault-backed stdio MCP environment should parse");

    let literal = valid.replace("${credential:access_token}", "plaintext-secret");
    let error = parse_plugin_manifest(literal.as_str(), PluginManifestSource::Codex)
        .expect_err("literal stdio MCP secret must fail");
    assert!(error.to_string().contains("exact ${credential"));

    let host_controlled = valid.replace("DEMO_TOKEN", "PATH");
    let error = parse_plugin_manifest(host_controlled.as_str(), PluginManifestSource::Codex)
        .expect_err("host-controlled stdio MCP environment must fail");
    assert!(error.to_string().contains("controlled by the Host"));
}

#[test]
fn rejects_insecure_public_urls() {
    let raw = CODEX_FIGMA_MANIFEST.replace(
        "https://www.figma.com/legal/privacy/",
        "http://www.figma.com/legal/privacy/",
    );
    let error = parse_plugin_manifest(raw.as_str(), PluginManifestSource::Codex)
        .expect_err("insecure URL must fail");
    assert!(error.to_string().contains("absolute https:// URL"));
}

#[test]
fn recognizes_both_supported_manifest_locations() {
    assert_eq!(
        plugin_manifest_source_from_path(Path::new(".codex-plugin/plugin.json")),
        Some(PluginManifestSource::Codex)
    );
    assert_eq!(
        plugin_manifest_source_from_path(Path::new("bundle/.chatos-plugin/plugin.json")),
        Some(PluginManifestSource::Chatos)
    );
    assert_eq!(
        plugin_manifest_source_from_path(Path::new("plugin.json")),
        None
    );
}

#[test]
fn lifecycle_states_remain_distinct_on_the_wire() {
    assert_eq!(
        serde_json::to_value(PluginInstallStatus::Installed).expect("status JSON"),
        json!("installed")
    );
    assert_eq!(
        serde_json::to_value(PluginAvailabilityStatus::Ready).expect("status JSON"),
        json!("ready")
    );
    assert_eq!(
        serde_json::to_value(PluginRequirementStatus::Satisfied).expect("status JSON"),
        json!("satisfied")
    );
}

#[test]
fn component_inventory_is_derived_from_the_normalized_manifest() {
    let manifest = parse_plugin_manifest(CODEX_FIGMA_MANIFEST, PluginManifestSource::Codex)
        .expect("Codex manifest should parse");
    let components = plugin_component_descriptors(&manifest);
    assert_eq!(components.len(), 3);
    assert_eq!(components[0].kind, PluginComponentKind::SkillCollection);
    assert_eq!(components[1].component_key, "mcp-config");
    assert_eq!(components[2].kind, PluginComponentKind::ConnectedApp);
}

#[test]
fn detailed_command_metadata_enters_the_immutable_component_descriptor() {
    let raw = CODEX_FIGMA_MANIFEST.replace(
        "\"interface\": {",
        format!(
            "\"commands\": [{{\"componentKey\":\"review\",\"source\":\"./commands/review.md\",\"description\":\"Review the current change\",\"argumentHint\":\"[path]\",\"requiresConfirmation\":true,\"targetAgent\":\"{RUN_AGENT_KEY}\",\"allowedTools\":[\"browser_tools_browser_snapshot\",\"plugin_browser_browser_snapshot\"]}}], \"interface\": {{"
        )
        .as_str(),
    );
    let manifest = parse_plugin_manifest(raw.as_str(), PluginManifestSource::Codex)
        .expect("detailed command should parse");
    let command = manifest.commands.first().expect("command");
    assert_eq!(command.component_key, "review");
    assert_eq!(command.argument_hint.as_deref(), Some("[path]"));
    assert!(command.requires_confirmation);
    assert_eq!(command.target_agent.as_deref(), Some(RUN_AGENT_KEY));
    assert_eq!(
        command.allowed_tools,
        vec![
            "browser_tools_browser_snapshot",
            "plugin_browser_browser_snapshot"
        ]
    );

    let descriptor = plugin_component_descriptors(&manifest)
        .into_iter()
        .find(|component| component.kind == PluginComponentKind::Command)
        .expect("command descriptor");
    assert!(!descriptor.required);
    assert_eq!(
        descriptor.entrypoint.expect("entrypoint").path,
        "./commands/review.md"
    );
    assert_eq!(
        descriptor.metadata.get("description"),
        Some(&json!("Review the current change"))
    );
    assert_eq!(
        descriptor.metadata.get("requires_confirmation"),
        Some(&json!(true))
    );
    assert_eq!(
        descriptor.metadata.get("target_agent"),
        Some(&json!(RUN_AGENT_KEY))
    );
    assert_eq!(
        descriptor.metadata.get("allowed_tools"),
        Some(&json!([
            "browser_tools_browser_snapshot",
            "plugin_browser_browser_snapshot"
        ]))
    );
}

#[test]
fn detailed_agent_metadata_enters_the_immutable_component_descriptor() {
    let raw = CODEX_FIGMA_MANIFEST.replace(
        "\"interface\": {",
        format!(
            "\"agents\": [{{\"componentKey\":\"reviewer\",\"source\":\"./agents/reviewer.md\",\"description\":\"Review changes with a narrow tool set\",\"baseAgent\":\"{RUN_AGENT_KEY}\",\"allowedTools\":[\"browser_tools_browser_snapshot\"],\"maxIterations\":12}}], \"interface\": {{"
        )
        .as_str(),
    );
    let manifest = parse_plugin_manifest(raw.as_str(), PluginManifestSource::Codex)
        .expect("detailed Agent should parse");
    let agent = manifest.agents.first().expect("Agent");
    assert_eq!(agent.component_key, "reviewer");
    assert_eq!(agent.base_agent, RUN_AGENT_KEY);
    assert_eq!(agent.max_iterations, 12);
    assert_eq!(agent.allowed_tools, vec!["browser_tools_browser_snapshot"]);

    let descriptor = plugin_component_descriptors(&manifest)
        .into_iter()
        .find(|component| component.kind == PluginComponentKind::Agent)
        .expect("Agent descriptor");
    assert_eq!(descriptor.runtime_kind, "agent_profile");
    assert_eq!(
        descriptor.metadata.get("description"),
        Some(&json!("Review changes with a narrow tool set"))
    );
    assert_eq!(
        descriptor.metadata.get("base_agent"),
        Some(&json!(RUN_AGENT_KEY))
    );
    assert_eq!(
        descriptor.metadata.get("allowed_tools"),
        Some(&json!(["browser_tools_browser_snapshot"]))
    );
    assert_eq!(descriptor.metadata.get("max_iterations"), Some(&json!(12)));
}

#[test]
fn detailed_hook_set_enters_the_immutable_component_descriptor() {
    let raw = CODEX_FIGMA_MANIFEST.replace(
        "\"interface\": {",
        "\"hooks\": [{\"componentKey\":\"lifecycle-hooks\",\"source\":\"./hooks.json\"}], \"interface\": {",
    );
    let manifest = parse_plugin_manifest(raw.as_str(), PluginManifestSource::Codex)
        .expect("detailed Hook set should parse");
    let hook = manifest.hooks.first().expect("Hook set");
    assert_eq!(hook.component_key, "lifecycle-hooks");
    assert_eq!(hook.source.path, "./hooks.json");

    let descriptor = plugin_component_descriptors(&manifest)
        .into_iter()
        .find(|component| component.kind == PluginComponentKind::HookSet)
        .expect("Hook descriptor");
    assert!(!descriptor.required);
    assert_eq!(descriptor.runtime_kind, "hook_set");
    assert_eq!(
        descriptor.entrypoint.expect("entrypoint").path,
        "./hooks.json"
    );
}

#[test]
fn detailed_ui_contribution_enters_the_signed_component_descriptor() {
    let raw = CODEX_FIGMA_MANIFEST.replace(
        "\"interface\": {",
        "\"ui\": [{\"componentKey\":\"security-workbench\",\"source\":\"./ui/index.html\",\"title\":\"Security Workbench\",\"surface\":\"workbench\",\"assets\":[\"./ui/app.js\",\"./ui/styles.css\"],\"bridgeCapabilities\":[\"artifact.download\",\"artifact.list\",\"host.context.read\"],\"artifactMimeTypes\":[\"application/json\",\"application/pdf\"]}], \"interface\": {",
    );
    let manifest = parse_plugin_manifest(raw.as_str(), PluginManifestSource::Codex)
        .expect("detailed Plugin UI should parse");
    let ui = manifest.ui.first().expect("Plugin UI contribution");
    assert_eq!(ui.component_key, "security-workbench");
    assert_eq!(ui.source.path, "./ui/index.html");
    assert_eq!(ui.surface.as_deref(), Some("workbench"));
    assert_eq!(
        ui.assets
            .iter()
            .map(|asset| asset.path.as_str())
            .collect::<Vec<_>>(),
        vec!["./ui/app.js", "./ui/styles.css"]
    );

    let descriptor = plugin_component_descriptors(&manifest)
        .into_iter()
        .find(|component| component.kind == PluginComponentKind::UiContribution)
        .expect("Plugin UI descriptor");
    assert_eq!(descriptor.runtime_kind, "sandboxed_ui");
    assert_eq!(
        descriptor.entrypoint.expect("entrypoint").path,
        "./ui/index.html"
    );
    assert_eq!(
        descriptor.metadata.get("title"),
        Some(&json!("Security Workbench"))
    );
    assert_eq!(
        descriptor.metadata.get("surface"),
        Some(&json!("workbench"))
    );
    assert_eq!(
        descriptor.metadata.get("assets"),
        Some(&json!(["./ui/app.js", "./ui/styles.css"]))
    );
    assert_eq!(
        descriptor.metadata.get("bridge_capabilities"),
        Some(&json!([
            "artifact.download",
            "artifact.list",
            "host.context.read"
        ]))
    );
    assert_eq!(
        descriptor.metadata.get("artifact_mime_types"),
        Some(&json!(["application/json", "application/pdf"]))
    );
}

#[test]
fn ui_contribution_rejects_remote_or_executable_assets_and_unknown_bridge_methods() {
    for ui in [
        r#"{"componentKey":"workbench","source":"./assets/index.html"}"#,
        r#"{"componentKey":"workbench","source":"./ui/index.js"}"#,
        r#"{"componentKey":"workbench","source":"./ui/index.html","assets":["./ui/helper.sh"]}"#,
        r#"{"componentKey":"workbench","source":"./ui/index.html","bridgeCapabilities":["network.fetch"]}"#,
        r#"{"componentKey":"workbench","source":"./ui/index.html","surface":"floating_window"}"#,
        r#"{"componentKey":"workbench","source":"./ui/index.html","artifactMimeTypes":["application/pdf; charset=utf-8"]}"#,
    ] {
        let raw = CODEX_FIGMA_MANIFEST.replace(
            "\"interface\": {",
            format!("\"ui\": [{ui}], \"interface\": {{").as_str(),
        );
        assert!(
            parse_plugin_manifest(raw.as_str(), PluginManifestSource::Codex).is_err(),
            "unsafe Plugin UI declaration should fail: {ui}"
        );
    }
}

#[test]
fn agent_path_shorthand_uses_bounded_run_phase_defaults() {
    assert_eq!(PLUGIN_AGENT_DEFAULT_MAX_ITERATIONS, 600);
    assert_eq!(PLUGIN_AGENT_MAX_ITERATIONS, 5_000);
    let raw = CODEX_FIGMA_MANIFEST.replace(
        "\"interface\": {",
        "\"agents\": \"./agents/reviewer.md\", \"interface\": {",
    );
    let manifest = parse_plugin_manifest(raw.as_str(), PluginManifestSource::Codex)
        .expect("Agent shorthand should parse");
    let agent = manifest.agents.first().expect("Agent");
    assert_eq!(agent.component_key, "reviewer");
    assert_eq!(agent.base_agent, RUN_AGENT_KEY);
    assert_eq!(agent.max_iterations, PLUGIN_AGENT_DEFAULT_MAX_ITERATIONS);
}

#[test]
fn agent_execution_metadata_rejects_unsafe_bounds() {
    for agent in [
        r#"{"componentKey":"reviewer","source":"./agents/reviewer.md","baseAgent":"chatos_conversation_agent"}"#,
        r#"{"componentKey":"reviewer","source":"./agents/reviewer.md","allowedTools":["browser.tools/snapshot"]}"#,
        r#"{"componentKey":"reviewer","source":"./agents/reviewer.md","maxIterations":0}"#,
        r#"{"componentKey":"reviewer","source":"./agents/reviewer.md","maxIterations":5001}"#,
    ] {
        let raw = CODEX_FIGMA_MANIFEST.replace(
            "\"interface\": {",
            format!("\"agents\": [{agent}], \"interface\": {{").as_str(),
        );
        assert!(
            parse_plugin_manifest(raw.as_str(), PluginManifestSource::Codex).is_err(),
            "invalid Agent metadata should fail: {agent}"
        );
    }
}

#[test]
fn command_execution_metadata_rejects_unsupported_agents_and_tool_names() {
    for command in [
        r#"{"componentKey":"review","source":"./commands/review.md","targetAgent":"chatos_conversation_agent"}"#,
        r#"{"componentKey":"review","source":"./commands/review.md","allowedTools":["browser.tools/snapshot"]}"#,
        r#"{"componentKey":"review","source":"./commands/review.md","allowedTools":["browser_snapshot","browser_snapshot"]}"#,
    ] {
        let raw = CODEX_FIGMA_MANIFEST.replace(
            "\"interface\": {",
            format!("\"commands\": [{command}], \"interface\": {{").as_str(),
        );
        assert!(
            parse_plugin_manifest(raw.as_str(), PluginManifestSource::Codex).is_err(),
            "invalid Command metadata should fail: {command}"
        );
    }
}

#[test]
fn permissions_can_target_derived_skill_component_keys() {
    let raw = CODEX_FIGMA_MANIFEST.replace(
        "\"interface\": {",
        "\"permissions\": [{\"permission\": \"workspace.read\", \"components\": [\"skills\"]}], \"interface\": {",
    );
    let manifest = parse_plugin_manifest(raw.as_str(), PluginManifestSource::Codex)
        .expect("skill-targeted permission should parse");
    let components = plugin_component_descriptors(&manifest);
    assert_eq!(components[0].permissions.len(), 1);
    assert_eq!(components[0].permissions[0].permission, "workspace.read");
}

#[test]
fn normalized_manifest_signature_hash_is_stable_across_round_trip() {
    let manifest = parse_plugin_manifest(CODEX_FIGMA_MANIFEST, PluginManifestSource::Codex)
        .expect("Codex manifest should parse");
    let encoded = serde_json::to_value(&manifest).expect("manifest JSON");
    let decoded: PluginManifest = serde_json::from_value(encoded).expect("normalized manifest");
    assert_eq!(
        normalized_plugin_manifest_sha256(&manifest).expect("manifest hash"),
        normalized_plugin_manifest_sha256(&decoded).expect("round-trip manifest hash")
    );
}
