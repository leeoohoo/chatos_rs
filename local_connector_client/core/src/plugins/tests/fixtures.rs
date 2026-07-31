// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD, Engine as _};
use chatos_plugin_management_sdk::*;
use ring::rand::SystemRandom;
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde_json::json;
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::super::*;

const MARKETPLACE_ID: &str = "trusted-marketplace";
const BUNDLED_MARKETPLACE_ID: &str = "chatos-bundled";
const PUBLISHER_ID: &str = "publisher-demo";
pub(in crate::plugins) const PLUGIN_ID: &str = "plugin-demo";

#[derive(Debug, Clone, Copy)]
pub(in crate::plugins) enum ArchiveMutation {
    None,
    Traversal,
    Symlink,
    Duplicate,
    MissingChecksum,
    WrongChecksum,
    InvalidSbom,
    SkillReferenceCycle,
    SkillTraversalReference,
}

pub(in crate::plugins) struct TestSigner {
    keypair: Ed25519KeyPair,
    key: SigningKeyRef,
    marketplace_id: &'static str,
}

impl TestSigner {
    pub(in crate::plugins) fn new() -> Self {
        Self::for_marketplace(MARKETPLACE_ID)
    }

    pub(in crate::plugins) fn new_bundled() -> Self {
        Self::for_marketplace(BUNDLED_MARKETPLACE_ID)
    }

    fn for_marketplace(marketplace_id: &'static str) -> Self {
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).expect("test key");
        let keypair = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref()).expect("parse test key");
        let key = SigningKeyRef {
            key_id: "release-key-v1".to_string(),
            publisher_id: PUBLISHER_ID.to_string(),
            algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
            public_key_base64: STANDARD.encode(keypair.public_key().as_ref()),
            usages: vec![PLUGIN_SIGNING_KEY_USAGE_RELEASE.to_string()],
            valid_from: "2026-01-01T00:00:00Z".to_string(),
            valid_until: Some("2027-01-01T00:00:00Z".to_string()),
            revoked_at: None,
        };
        Self {
            keypair,
            key,
            marketplace_id,
        }
    }

    pub(in crate::plugins) fn package(
        &self,
        root: &Path,
        version: &str,
        mutation: ArchiveMutation,
    ) -> TestPackage {
        self.package_from_manifest(
            root,
            version,
            mutation,
            manifest_json(version),
            BTreeMap::new(),
        )
    }

    pub(in crate::plugins) fn package_with_prompt_execution(
        &self,
        root: &Path,
        version: &str,
        execution_host: PluginExecutionHost,
    ) -> TestPackage {
        let default_host = match execution_host {
            PluginExecutionHost::Cloud => "cloud",
            PluginExecutionHost::Local => "local",
            PluginExecutionHost::Portable => "portable",
        };
        self.package_from_manifest(
            root,
            version,
            ArchiveMutation::None,
            json!({
                "schemaVersion": 2,
                "execution": {"defaultHost": default_host, "componentHosts": {}},
                "name": "demo-plugin",
                "version": version,
                "description": "A signed schema v2 Prompt Plugin fixture",
                "author": {"name": "Demo Publisher"},
                "skills": ["./skills/demo"],
                "interface": {
                    "displayName": "Demo Plugin",
                    "shortDescription": "Signed Prompt Plugin",
                    "longDescription": "A signed schema v2 Prompt Plugin fixture",
                    "developerName": "Demo Publisher",
                    "category": "Developer Tools"
                },
                "dependencies": {},
                "permissions": []
            })
            .to_string(),
            BTreeMap::new(),
        )
    }

    pub(in crate::plugins) fn package_with_command(
        &self,
        root: &Path,
        version: &str,
        requires_confirmation: bool,
    ) -> TestPackage {
        self.package_from_manifest(
            root,
            version,
            ArchiveMutation::None,
            json!({
                "name": "demo-plugin",
                "version": version,
                "description": "A signed Plugin Command fixture",
                "author": {"name": "Demo Publisher"},
                "commands": [{
                    "componentKey": "review",
                    "source": "./commands/review.md",
                    "description": "Review the current change",
                    "argumentHint": "[path]",
                    "requiresConfirmation": requires_confirmation,
                    "targetAgent": "task_runner_run_phase",
                    "allowedTools": ["browser_tools_browser_snapshot"]
                }],
                "interface": {
                    "displayName": "Demo Plugin",
                    "shortDescription": "Signed test Plugin",
                    "longDescription": "A signed Plugin Command fixture",
                    "developerName": "Demo Publisher",
                    "category": "Developer Tools"
                },
                "permissions": [{
                    "permission": "workspace.read",
                    "required": true,
                    "components": ["review"]
                }]
            })
            .to_string(),
            BTreeMap::from([(
                "commands/review.md".to_string(),
                b"---\nname: review\n---\n\nReview the current change and report concrete findings.\n"
                    .to_vec(),
            )]),
        )
    }

    pub(in crate::plugins) fn package_with_agent(&self, root: &Path, version: &str) -> TestPackage {
        self.package_from_manifest(
            root,
            version,
            ArchiveMutation::None,
            json!({
                "name": "demo-plugin",
                "version": version,
                "description": "A signed Plugin Agent fixture",
                "author": {"name": "Demo Publisher"},
                "agents": [{
                    "componentKey": "reviewer",
                    "source": "./agents/reviewer.md",
                    "description": "Review the current change",
                    "baseAgent": "task_runner_run_phase",
                    "allowedTools": ["browser_tools_browser_snapshot"],
                    "maxIterations": 12
                }],
                "interface": {
                    "displayName": "Demo Plugin",
                    "shortDescription": "Signed test Plugin",
                    "longDescription": "A signed Plugin Agent fixture",
                    "developerName": "Demo Publisher",
                    "category": "Developer Tools"
                },
                "permissions": [{
                    "permission": "workspace.read",
                    "required": true,
                    "components": ["reviewer"]
                }]
            })
            .to_string(),
            BTreeMap::from([(
                "agents/reviewer.md".to_string(),
                b"---\nname: reviewer\n---\n\nReview the current change and report concrete findings.\n"
                    .to_vec(),
            )]),
        )
    }

    pub(in crate::plugins) fn package_with_hooks(&self, root: &Path, version: &str) -> TestPackage {
        let hook_set = json!({
            "schemaVersion": 1,
            "hooks": [
                {
                    "id": "audit-run",
                    "events": ["BeforePluginPrepare", "SessionStart", "RunCompleted", "RunFailed"],
                    "matcher": {"agentKeys": ["task_runner_run_phase"]},
                    "entrypoint": {
                        "type": "command",
                        "command": "./scripts/audit-hook.sh",
                        "args": ["--json"]
                    },
                    "timeoutMs": 2500,
                    "maxOutputBytes": 4096,
                    "failurePolicy": "continue"
                },
                {
                    "id": "audit-disabled",
                    "events": ["PluginDisabled"],
                    "entrypoint": {
                        "type": "command",
                        "command": "./scripts/audit-hook.sh",
                        "args": ["--json"]
                    },
                    "timeoutMs": 2500,
                    "maxOutputBytes": 4096,
                    "failurePolicy": "continue"
                }
            ]
        })
        .to_string();
        self.package_from_manifest(
            root,
            version,
            ArchiveMutation::None,
            json!({
                "name": "demo-plugin",
                "version": version,
                "description": "A signed Plugin Hook fixture",
                "author": {"name": "Demo Publisher"},
                "hooks": [{
                    "componentKey": "lifecycle-hooks",
                    "source": "./hooks.json"
                }],
                "interface": {
                    "displayName": "Demo Plugin",
                    "shortDescription": "Signed test Plugin",
                    "longDescription": "A signed Plugin Hook fixture",
                    "developerName": "Demo Publisher",
                    "category": "Developer Tools"
                },
                "permissions": [{
                    "permission": "process.spawn",
                    "required": true,
                    "components": ["lifecycle-hooks"]
                }]
            })
            .to_string(),
            BTreeMap::from([
                ("hooks.json".to_string(), hook_set.into_bytes()),
                (
                    "scripts/audit-hook.sh".to_string(),
                    b"#!/bin/sh\ncat >/dev/null\nprintf '{\"ok\":true}\\n'\n".to_vec(),
                ),
            ]),
        )
    }

    pub(in crate::plugins) fn package_with_packaged_hook_suite(
        &self,
        root: &Path,
        version: &str,
    ) -> TestPackage {
        let lifecycle_hook_set = json!({
            "schemaVersion": 1,
            "hooks": [{
                "id": "packaged-audit",
                "events": ["SessionStart"],
                "entrypoint": {
                    "type": "command",
                    "command": "./scripts/packaged-audit-hook.sh",
                    "args": ["--json"]
                },
                "timeoutMs": 8000,
                "maxOutputBytes": 4096,
                "failurePolicy": "continue"
            }]
        })
        .to_string();
        let workspace_hook_set = json!({
            "schemaVersion": 1,
            "hooks": [{
                "id": "packaged-workspace-write",
                "events": ["SessionStart"],
                "entrypoint": {
                    "type": "command",
                    "command": "./scripts/packaged-workspace-hook.sh",
                    "args": ["--json"]
                },
                "timeoutMs": 8000,
                "maxOutputBytes": 4096,
                "failurePolicy": "continue",
                "workspaceWrite": true
            }]
        })
        .to_string();
        self.package_from_manifest(
            root,
            version,
            ArchiveMutation::None,
            json!({
                "name": "demo-plugin",
                "version": version,
                "description": "A signed packaged Local Connector Hook fixture",
                "author": {"name": "Demo Publisher"},
                "hooks": [
                    {
                        "componentKey": "packaged-lifecycle-hooks",
                        "source": "./hooks-lifecycle.json"
                    },
                    {
                        "componentKey": "packaged-workspace-hooks",
                        "source": "./hooks-workspace.json"
                    }
                ],
                "interface": {
                    "displayName": "Demo Plugin",
                    "shortDescription": "Signed packaged Hook test Plugin",
                    "longDescription": "A signed packaged Local Connector Hook fixture",
                    "developerName": "Demo Publisher",
                    "category": "Developer Tools"
                },
                "permissions": [
                    {
                        "permission": "process.spawn",
                        "required": true,
                        "components": [
                            "packaged-lifecycle-hooks",
                            "packaged-workspace-hooks"
                        ]
                    },
                    {
                        "permission": "workspace.write",
                        "required": true,
                        "components": ["packaged-workspace-hooks"]
                    }
                ]
            })
            .to_string(),
            BTreeMap::from([
                (
                    "hooks-lifecycle.json".to_string(),
                    lifecycle_hook_set.into_bytes(),
                ),
                (
                    "hooks-workspace.json".to_string(),
                    workspace_hook_set.into_bytes(),
                ),
                (
                    "scripts/packaged-audit-hook.sh".to_string(),
                    b"#!/bin/sh\nprintf 'packaged-hook-stdout-secret\\n'\nprintf 'packaged-hook-stderr-secret\\n' >&2\n"
                        .to_vec(),
                ),
                (
                    "scripts/packaged-workspace-hook.sh".to_string(),
                    b"#!/bin/sh\ntest -n \"$CHATOS_WORKSPACE\"\nprintf 'created by packaged Hook\\n' > \"$CHATOS_WORKSPACE/hook-was-here\"\nif printf 'forbidden\\n' > \"$CHATOS_WORKSPACE/.git/plugin-hook-probe\" 2>/dev/null; then\n  exit 29\nfi\nprintf 'packaged-write-stdout-secret\\n'\nprintf 'packaged-write-stderr-secret\\n' >&2\n"
                        .to_vec(),
                ),
            ]),
        )
    }

    pub(in crate::plugins) fn package_with_ui(
        &self,
        root: &Path,
        version: &str,
        html: &[u8],
    ) -> TestPackage {
        self.package_from_manifest(
            root,
            version,
            ArchiveMutation::None,
            json!({
                "name": "demo-plugin",
                "version": version,
                "description": "A signed Plugin UI fixture",
                "author": {"name": "Demo Publisher"},
                "ui": [{
                    "componentKey": "security-workbench",
                    "source": "./ui/index.html",
                    "title": "Security Workbench",
                    "surface": "workbench",
                    "assets": ["./ui/app.js", "./ui/styles.css"],
                    "bridgeCapabilities": [
                        "artifact.download",
                        "artifact.list",
                        "artifact.read",
                        "host.context.read"
                    ],
                    "artifactMimeTypes": ["application/json", "application/pdf"]
                }],
                "interface": {
                    "displayName": "Demo Plugin",
                    "shortDescription": "Signed test Plugin",
                    "longDescription": "A signed Plugin UI fixture",
                    "developerName": "Demo Publisher",
                    "category": "Developer Tools"
                },
                "permissions": [{
                    "permission": "artifact.read",
                    "required": true,
                    "components": ["security-workbench"]
                }]
            })
            .to_string(),
            BTreeMap::from([
                ("ui/index.html".to_string(), html.to_vec()),
                (
                    "ui/app.js".to_string(),
                    b"window.parent.postMessage({type:'plugin-ui-ready'}, '*');\n".to_vec(),
                ),
                (
                    "ui/styles.css".to_string(),
                    b"body { color: CanvasText; background: Canvas; }\n".to_vec(),
                ),
            ]),
        )
    }

    pub(in crate::plugins) fn package_with_artifact_workbench(
        &self,
        root: &Path,
        version: &str,
        html: &[u8],
    ) -> TestPackage {
        assert_eq!(
            self.marketplace_id, BUNDLED_MARKETPLACE_ID,
            "native Adapter fixtures must use the bundled Marketplace identity"
        );
        let documents = crate::skills::internal_skill_catalog()
            .expect("internal Skill catalog")
            .skills
            .into_iter()
            .find(|item| item.skill_id == "internal_skill_documents")
            .expect("Documents Skill inventory");
        let skill_manifest = crate::skills::internal_skill_manifest(documents.skill_id.as_str())
            .expect("embedded Documents skill.json");
        let instructions = crate::skills::internal_skill_instructions(documents.skill_id.as_str())
            .expect("embedded Documents instructions");
        let description = serde_json::to_string(documents.description.as_str())
            .expect("encode Documents description");
        let skill_document = format!(
            "---\nname: {}\ndescription: {}\ndisable-model-invocation: false\n---\n\n{}\n",
            documents.name,
            description,
            instructions.trim_end()
        );
        self.package_from_manifest(
            root,
            version,
            ArchiveMutation::None,
            json!({
                "name": "demo-plugin",
                "version": version,
                "description": "A signed multi-component Artifact Workbench fixture",
                "author": {"name": "Demo Publisher"},
                "skills": ["./skills/demo", "./skills/documents"],
                "ui": [{
                    "componentKey": "artifact-workbench",
                    "source": "./ui/index.html",
                    "title": "Artifact Workbench",
                    "surface": "workbench",
                    "assets": ["./ui/app.js", "./ui/styles.css"],
                    "bridgeCapabilities": [
                        "artifact.download",
                        "artifact.list",
                        "artifact.read",
                        "artifact.create",
                        "artifact.update",
                        "host.context.read"
                    ],
                    "artifactMimeTypes": [
                        "application/json",
                        "application/pdf",
                        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                    ]
                }],
                "interface": {
                    "displayName": "Demo Plugin",
                    "shortDescription": "Signed Artifact Workbench fixture",
                    "longDescription": "A signed multi-component Artifact Workbench fixture",
                    "developerName": "Demo Publisher",
                    "category": "Developer Tools"
                },
                "permissions": [
                    {
                        "permission": "workspace.read",
                        "required": true,
                        "components": ["demo", "documents"]
                    },
                    {
                        "permission": "workspace.write",
                        "required": true,
                        "components": ["documents"]
                    },
                    {
                        "permission": "artifact.read",
                        "required": true,
                        "components": ["artifact-workbench"]
                    }
                ]
            })
            .to_string(),
            BTreeMap::from([
                (
                    "skills/documents/SKILL.md".to_string(),
                    skill_document.into_bytes(),
                ),
                (
                    "skills/documents/instructions.md".to_string(),
                    instructions.as_bytes().to_vec(),
                ),
                (
                    "skills/documents/skill.json".to_string(),
                    skill_manifest.as_bytes().to_vec(),
                ),
                ("ui/index.html".to_string(), html.to_vec()),
                (
                    "ui/app.js".to_string(),
                    b"window.parent.postMessage({type:'plugin-ui-ready'}, '*');\n".to_vec(),
                ),
                (
                    "ui/styles.css".to_string(),
                    b"body { color: CanvasText; background: Canvas; }\n".to_vec(),
                ),
            ]),
        )
    }

    pub(in crate::plugins) fn package_with_workspace_write_hook(
        &self,
        root: &Path,
        version: &str,
    ) -> TestPackage {
        let hook_set = json!({
            "schemaVersion": 1,
            "hooks": [{
                "id": "write-workspace",
                "events": ["SessionStart"],
                "entrypoint": {
                    "type": "command",
                    "command": "./scripts/write-hook.sh",
                    "args": ["--json"]
                },
                "timeoutMs": 2500,
                "maxOutputBytes": 4096,
                "failurePolicy": "continue",
                "workspaceWrite": true
            }]
        })
        .to_string();
        self.package_from_manifest(
            root,
            version,
            ArchiveMutation::None,
            json!({
                "name": "demo-plugin",
                "version": version,
                "description": "A signed writable Plugin Hook fixture",
                "author": {"name": "Demo Publisher"},
                "hooks": [{
                    "componentKey": "workspace-hooks",
                    "source": "./hooks.json"
                }],
                "interface": {
                    "displayName": "Demo Plugin",
                    "shortDescription": "Signed writable test Plugin",
                    "longDescription": "A signed writable Plugin Hook fixture",
                    "developerName": "Demo Publisher",
                    "category": "Developer Tools"
                },
                "permissions": [
                    {
                        "permission": "process.spawn",
                        "required": true,
                        "components": ["workspace-hooks"]
                    },
                    {
                        "permission": "workspace.write",
                        "required": true,
                        "components": ["workspace-hooks"]
                    }
                ]
            })
            .to_string(),
            BTreeMap::from([
                ("hooks.json".to_string(), hook_set.into_bytes()),
                (
                    "scripts/write-hook.sh".to_string(),
                    b"#!/bin/sh\ncat >/dev/null\nprintf '{\"ok\":true}\\n'\n".to_vec(),
                ),
            ]),
        )
    }

    pub(in crate::plugins) fn package_with_http_mcp(
        &self,
        root: &Path,
        version: &str,
        url: &str,
    ) -> TestPackage {
        self.package_from_manifest(
            root,
            version,
            ArchiveMutation::None,
            manifest_json_with_mcp(
                version,
                json!({
                    "demo-http": {
                        "type": "http",
                        "url": url,
                        "connectTimeoutMs": 5_000
                    }
                }),
                json!({
                    "permission": format!(
                        "network.domain:{}",
                        reqwest::Url::parse(url)
                            .expect("fixture MCP URL")
                            .host_str()
                            .expect("fixture MCP host")
                    ),
                    "required": true,
                    "components": ["demo-http"]
                }),
            ),
            BTreeMap::new(),
        )
    }

    pub(in crate::plugins) fn package_with_stdio_mcp(
        &self,
        root: &Path,
        version: &str,
    ) -> TestPackage {
        self.package_from_manifest(
            root,
            version,
            ArchiveMutation::None,
            manifest_json_with_mcp(
                version,
                json!({
                    "demo-stdio": {
                        "type": "stdio",
                        "command": "./mcp/server.sh",
                        "cwd": "./mcp"
                    }
                }),
                json!({
                    "permission": "process.spawn",
                    "required": true,
                    "components": ["demo-stdio"]
                }),
            ),
            BTreeMap::from([("mcp/server.sh".to_string(), b"#!/bin/sh\nexit 0\n".to_vec())]),
        )
    }

    pub(in crate::plugins) fn package_with_stdio_mcp_credential(
        &self,
        root: &Path,
        version: &str,
    ) -> TestPackage {
        self.package_from_manifest(
            root,
            version,
            ArchiveMutation::None,
            manifest_json_with_mcp(
                version,
                json!({
                    "demo-stdio": {
                        "type": "stdio",
                        "command": "./mcp/server.sh",
                        "cwd": "./mcp",
                        "env": {
                            "DEMO_TOKEN": "${credential:access_token}"
                        }
                    }
                }),
                json!([
                    {
                        "permission": "process.spawn",
                        "required": true,
                        "components": ["demo-stdio"]
                    },
                    {
                        "permission": "credential.use:demo",
                        "required": true,
                        "components": ["demo-stdio"]
                    }
                ]),
            ),
            BTreeMap::from([("mcp/server.sh".to_string(), b"#!/bin/sh\nexit 0\n".to_vec())]),
        )
    }

    #[cfg(unix)]
    pub(in crate::plugins) fn package_with_hanging_stdio_mcp_credential(
        &self,
        root: &Path,
        version: &str,
    ) -> TestPackage {
        let script = br##"#!/bin/sh
while IFS= read -r request; do
  id=$(printf '%s\n' "$request" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  case "$request" in
    *'"method":"tools/list"'*)
      if [ "$DEMO_TOKEN" != "stdio-top-secret" ]; then
        printf '{"jsonrpc":"2.0","id":"%s","error":{"code":-32000,"message":"missing credential"}}\n' "$id"
      else
        printf '{"jsonrpc":"2.0","id":"%s","result":{"tools":[{"name":"echo","description":"Echo input","inputSchema":{"type":"object"}}]}}\n' "$id"
      fi
      ;;
    *'"method":"tools/call"'*)
      sleep 300 &
      descendant_pid=$!
      printf '%s\n' "$descendant_pid" > descendant.pid
      wait "$descendant_pid"
      ;;
  esac
done
"##;
        self.package_from_manifest(
            root,
            version,
            ArchiveMutation::None,
            manifest_json_with_mcp(
                version,
                json!({
                    "demo-stdio": {
                        "type": "stdio",
                        "command": "./mcp/server.sh",
                        "cwd": "./mcp",
                        "env": {
                            "DEMO_TOKEN": "${credential:access_token}"
                        }
                    }
                }),
                json!([
                    {
                        "permission": "process.spawn",
                        "required": true,
                        "components": ["demo-stdio"]
                    },
                    {
                        "permission": "credential.use:demo",
                        "required": true,
                        "components": ["demo-stdio"]
                    }
                ]),
            ),
            BTreeMap::from([("mcp/server.sh".to_string(), script.to_vec())]),
        )
    }

    #[cfg(target_os = "macos")]
    pub(in crate::plugins) fn package_with_sandbox_probe_stdio_mcp(
        &self,
        root: &Path,
        version: &str,
        network_probe_port: u16,
    ) -> TestPackage {
        let script = format!(
            r##"#!/bin/sh
while IFS= read -r request; do
  id=$(printf '%s\n' "$request" | sed -n 's/.*"id":"\([^"]*\)".*/\1/p')
  case "$request" in
    *'"method":"tools/list"'*)
      if [ "$DEMO_TOKEN" != "sandbox-secret" ]; then
        printf '{{"jsonrpc":"2.0","id":"%s","error":{{"code":-32000,"message":"missing credential"}}}}\n' "$id"
      elif touch plugin-write-attempt 2>/dev/null; then
        printf '{{"jsonrpc":"2.0","id":"%s","error":{{"code":-32001,"message":"plugin root was writable"}}}}\n' "$id"
      elif ! touch "$HOME/state-ok" "$XDG_CACHE_HOME/cache-ok" "$TMPDIR/temp-ok"; then
        printf '{{"jsonrpc":"2.0","id":"%s","error":{{"code":-32002,"message":"runtime roots were not writable"}}}}\n' "$id"
      elif /usr/bin/nc -z -w 1 127.0.0.1 {network_probe_port} >/dev/null 2>&1; then
        printf '{{"jsonrpc":"2.0","id":"%s","error":{{"code":-32003,"message":"network isolation failed"}}}}\n' "$id"
      else
        printf '{{"jsonrpc":"2.0","id":"%s","result":{{"tools":[{{"name":"echo","description":"Echo input","inputSchema":{{"type":"object"}}}}]}}}}\n' "$id"
      fi
      ;;
    *'"method":"tools/call"'*)
      printf '{{"jsonrpc":"2.0","id":"%s","result":{{"content":{{"ok":true}}}}}}\n' "$id"
      ;;
  esac
done
"##
        );
        self.package_from_manifest(
            root,
            version,
            ArchiveMutation::None,
            manifest_json_with_mcp(
                version,
                json!({
                    "demo-stdio": {
                        "type": "stdio",
                        "command": "./mcp/server.sh",
                        "cwd": "./mcp",
                        "env": {
                            "DEMO_TOKEN": "${credential:access_token}"
                        }
                    }
                }),
                json!([
                    {
                        "permission": "process.spawn",
                        "required": true,
                        "components": ["demo-stdio"]
                    },
                    {
                        "permission": "credential.use:demo",
                        "required": true,
                        "components": ["demo-stdio"]
                    }
                ]),
            ),
            BTreeMap::from([("mcp/server.sh".to_string(), script.into_bytes())]),
        )
    }

    pub(in crate::plugins) fn package_with_mcp_config(
        &self,
        root: &Path,
        version: &str,
        url: &str,
    ) -> TestPackage {
        self.package_from_manifest(
            root,
            version,
            ArchiveMutation::None,
            manifest_json_with_mcp(
                version,
                json!("./.mcp.json"),
                json!({
                    "permission": format!(
                        "network.domain:{}",
                        reqwest::Url::parse(url)
                            .expect("fixture MCP URL")
                            .host_str()
                            .expect("fixture MCP host")
                    ),
                    "required": true,
                    "components": ["mcp-config"]
                }),
            ),
            BTreeMap::from([(
                ".mcp.json".to_string(),
                json!({
                    "mcpServers": {
                        "config-http": {
                            "type": "http",
                            "url": url,
                            "connectTimeoutMs": 5_000
                        },
                        "config-http-secondary": {
                            "type": "http",
                            "url": url,
                            "connectTimeoutMs": 5_000
                        }
                    }
                })
                .to_string()
                .into_bytes(),
            )]),
        )
    }

    pub(in crate::plugins) fn package_with_http_mcp_credential(
        &self,
        root: &Path,
        version: &str,
        url: &str,
    ) -> TestPackage {
        self.package_from_manifest(
            root,
            version,
            ArchiveMutation::None,
            manifest_json_with_mcp(
                version,
                json!({
                    "demo-http": {
                        "type": "http",
                        "url": url,
                        "headers": {
                            "Authorization": "Bearer ${credential:access_token}",
                            "X-Plugin-Client": "chatos"
                        },
                        "connectTimeoutMs": 5_000
                    }
                }),
                json!([
                    {
                        "permission": format!(
                            "network.domain:{}",
                            reqwest::Url::parse(url)
                                .expect("fixture MCP URL")
                                .host_str()
                                .expect("fixture MCP host")
                        ),
                        "required": true,
                        "components": ["demo-http"]
                    },
                    {
                        "permission": "credential.use:demo",
                        "required": true,
                        "components": ["demo-http"]
                    }
                ]),
            ),
            BTreeMap::new(),
        )
    }

    pub(in crate::plugins) fn package_with_oauth_mcp(
        &self,
        root: &Path,
        version: &str,
        mcp_url: &str,
        authorization_url: &str,
        token_url: &str,
    ) -> TestPackage {
        let mcp_host = reqwest::Url::parse(mcp_url)
            .expect("fixture OAuth MCP URL")
            .host_str()
            .expect("fixture OAuth MCP host")
            .to_string();
        self.package_from_manifest(
            root,
            version,
            ArchiveMutation::None,
            json!({
                "name": "demo-plugin",
                "version": version,
                "description": "A signed OAuth MCP test Plugin",
                "author": {"name": "Demo Publisher"},
                "skills": "./skills",
                "apps": "./apps/demo.app.json",
                "mcpServers": {
                    "demo-http": {
                        "type": "http",
                        "url": mcp_url,
                        "oauthResource": "resource-demo",
                        "connectTimeoutMs": 5_000
                    }
                },
                "interface": {
                    "displayName": "Demo Plugin",
                    "shortDescription": "Signed OAuth test Plugin",
                    "longDescription": "A signed OAuth MCP test Plugin",
                    "developerName": "Demo Publisher",
                    "category": "Developer Tools"
                },
                "permissions": [
                    {
                        "permission": "workspace.read",
                        "required": true,
                        "components": ["skills"]
                    },
                    {
                        "permission": format!("network.domain:{mcp_host}"),
                        "required": true,
                        "components": ["demo-http"]
                    },
                    {
                        "permission": "oauth.scope:demo:read",
                        "required": true,
                        "components": ["demo-http"]
                    }
                ]
            })
            .to_string(),
            BTreeMap::from([(
                "apps/demo.app.json".to_string(),
                json!({
                    "schemaVersion": 1,
                    "provider": "demo",
                    "clientId": "demo-client",
                    "authorizationUrl": authorization_url,
                    "tokenUrl": token_url,
                    "resource": "resource-demo",
                    "scopes": ["read"],
                    "callbackType": "loopback"
                })
                .to_string()
                .into_bytes(),
            )]),
        )
    }

    fn package_from_manifest(
        &self,
        root: &Path,
        version: &str,
        mutation: ArchiveMutation,
        manifest_raw: String,
        extra_files: BTreeMap<String, Vec<u8>>,
    ) -> TestPackage {
        let manifest = parse_plugin_manifest(manifest_raw.as_str(), PluginManifestSource::Codex)
            .expect("normalized manifest");
        let archive_path = root.join(format!("demo-{version}-{mutation:?}.zip"));
        write_test_archive(
            archive_path.as_path(),
            manifest_raw.as_bytes(),
            version,
            mutation,
            extra_files,
        );
        let artifact_sha256 = sha256_bytes(
            fs::read(&archive_path)
                .expect("read test archive")
                .as_slice(),
        );
        let marketplace = PluginMarketplaceRecord {
            id: self.marketplace_id.to_string(),
            name: self.marketplace_id.to_string(),
            owner_user_id: None,
            visibility: "public".to_string(),
            source_kind: "admin_registry".to_string(),
            catalog_url: Some("https://plugins.example.com/catalog.json".to_string()),
            enabled: true,
            trust_level: "trusted".to_string(),
            trusted_signing_keys: vec![self.key.clone()],
            last_catalog_revision: Some("revision-1".to_string()),
            last_synced_at: Some("2026-07-22T00:00:00Z".to_string()),
        };
        let catalog = PluginCatalogRecord {
            id: PLUGIN_ID.to_string(),
            plugin_key: format!("demo-plugin@{}", self.marketplace_id),
            marketplace_id: self.marketplace_id.to_string(),
            owner_user_id: None,
            name: manifest.name.clone(),
            display_name: manifest.interface.display_name.clone(),
            description: manifest.description.clone(),
            publisher: PluginPublisher {
                id: PUBLISHER_ID.to_string(),
                name: "Demo Publisher".to_string(),
                website: Some("https://plugins.example.com".to_string()),
                verified: true,
            },
            interface: manifest.interface.clone(),
            keywords: manifest.keywords.clone(),
            visibility: "public".to_string(),
            featured: false,
            enabled: true,
            latest_release_id: format!("release-{version}"),
            license: PluginLicenseMetadata {
                license_id: "MIT".to_string(),
                license_url: None,
                redistributable: true,
                reviewed_at: Some("2026-07-22T00:00:00Z".to_string()),
            },
            created_at: "2026-07-22T00:00:00Z".to_string(),
            updated_at: "2026-07-22T00:00:00Z".to_string(),
        };
        let mut signature = PluginReleaseSignature {
            key_id: self.key.key_id.clone(),
            publisher_id: PUBLISHER_ID.to_string(),
            marketplace_id: self.marketplace_id.to_string(),
            algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
            signature_base64: String::new(),
            signed_at: "2026-07-22T00:00:00Z".to_string(),
            manifest_sha256: normalized_plugin_manifest_sha256(&manifest).expect("manifest hash"),
        };
        let context = PluginReleaseVerificationContext {
            plugin_id: PLUGIN_ID,
            version,
            marketplace_id: self.marketplace_id,
            publisher_id: PUBLISHER_ID,
            artifact_sha256: artifact_sha256.as_str(),
        };
        let payload = plugin_release_signing_payload(context, &signature).expect("signing payload");
        signature.signature_base64 = STANDARD.encode(self.keypair.sign(&payload).as_ref());
        let release = PluginReleaseRecord {
            id: format!("release-{version}"),
            plugin_id: PLUGIN_ID.to_string(),
            version: version.to_string(),
            manifest_schema_version: manifest.schema_version,
            normalized_manifest: manifest.clone(),
            artifact_ref: format!("https://plugins.example.com/demo-{version}.zip"),
            artifact_sha256,
            signature,
            sbom_ref: Some("./sbom.json".to_string()),
            supported_platforms: manifest.dependencies.supported_platforms.clone(),
            components: plugin_component_descriptors(&manifest),
            dependencies: manifest.dependencies.clone(),
            permissions: manifest.permissions.clone(),
            release_channel: "stable".to_string(),
            published_at: "2026-07-22T00:00:00Z".to_string(),
            revoked_at: None,
        };
        TestPackage {
            archive_path,
            marketplace,
            catalog,
            release,
        }
    }
}

pub(in crate::plugins) struct TestPackage {
    pub(super) archive_path: PathBuf,
    marketplace: PluginMarketplaceRecord,
    catalog: PluginCatalogRecord,
    release: PluginReleaseRecord,
}

impl TestPackage {
    pub(in crate::plugins) fn archive_path(&self) -> &Path {
        self.archive_path.as_path()
    }

    pub(in crate::plugins) fn install_source(&self) -> PluginInstallSource {
        PluginInstallSource {
            marketplace: self.marketplace.clone(),
            catalog: self.catalog.clone(),
            release: self.release.clone(),
            preference: None,
        }
    }

    pub(in crate::plugins) fn install_request(&self) -> PluginInstallRequest<'_> {
        PluginInstallRequest {
            marketplace: &self.marketplace,
            catalog: &self.catalog,
            release: &self.release,
            archive_path: self.archive_path.as_path(),
        }
    }

    pub(in crate::plugins) fn corrupt_release_signature(&mut self) {
        self.release.signature.signature_base64 = STANDARD.encode([0_u8; 64]);
    }

    pub(super) fn verification_request<'a>(
        &'a self,
        extraction_root: &'a Path,
    ) -> PluginArtifactVerificationRequest<'a> {
        PluginArtifactVerificationRequest {
            marketplace: &self.marketplace,
            catalog: &self.catalog,
            release: &self.release,
            archive_path: self.archive_path.as_path(),
            extraction_root,
        }
    }
}

pub(super) fn recovery_record(
    transaction_id: &str,
    plugin_id: &str,
    target_version: Option<&str>,
    staging_path: Option<&str>,
    final_path: Option<&str>,
) -> PluginTransactionRecord {
    PluginTransactionRecord {
        transaction_id: transaction_id.to_string(),
        operation: PluginTransactionOperation::Install,
        status: PluginInstallStatus::Installing,
        plugin_id: plugin_id.to_string(),
        release_id: None,
        from_version: None,
        target_version: target_version.map(ToOwned::to_owned),
        relative_staging_path: staging_path.map(ToOwned::to_owned),
        relative_final_path: final_path.map(ToOwned::to_owned),
        relative_storage_path: None,
        relative_trash_path: None,
        downloaded_bytes: 0,
        total_bytes: None,
        started_at: "2026-07-22T00:00:00Z".to_string(),
        updated_at: "2026-07-22T00:00:00Z".to_string(),
        completed_at: None,
        recovered_after_restart: false,
        last_error: None,
    }
}

fn manifest_json(version: &str) -> String {
    json!({
        "name": "demo-plugin",
        "version": version,
        "description": "A signed multi-component test Plugin",
        "author": {"name": "Demo Publisher"},
        "skills": "./skills",
        "interface": {
            "displayName": "Demo Plugin",
            "shortDescription": "Signed test Plugin",
            "longDescription": "A signed multi-component test Plugin",
            "developerName": "Demo Publisher",
            "category": "Developer Tools"
        },
        "permissions": [{
            "permission": "workspace.read",
            "required": true,
            "components": ["skills"]
        }]
    })
    .to_string()
}

fn manifest_json_with_mcp(
    version: &str,
    mcp_servers: serde_json::Value,
    permission: serde_json::Value,
) -> String {
    let mut permissions = vec![json!({
        "permission": "workspace.read",
        "required": true,
        "components": ["skills"]
    })];
    match permission {
        serde_json::Value::Array(items) => permissions.extend(items),
        permission => permissions.push(permission),
    }
    json!({
        "name": "demo-plugin",
        "version": version,
        "description": "A signed multi-component test Plugin",
        "author": {"name": "Demo Publisher"},
        "skills": "./skills",
        "mcpServers": mcp_servers,
        "interface": {
            "displayName": "Demo Plugin",
            "shortDescription": "Signed test Plugin",
            "longDescription": "A signed multi-component test Plugin",
            "developerName": "Demo Publisher",
            "category": "Developer Tools"
        },
        "permissions": permissions
    })
    .to_string()
}

fn write_test_archive(
    path: &Path,
    manifest: &[u8],
    version: &str,
    mutation: ArchiveMutation,
    extra_files: BTreeMap<String, Vec<u8>>,
) {
    let skill = match mutation {
        ArchiveMutation::SkillReferenceCycle => format!(
            "---\nname: demo\ndescription: Cycle fixture\n---\n\n# Demo Skill\n\nVersion {version}\n\nSee [A](references/a.md).\n"
        ),
        ArchiveMutation::SkillTraversalReference => format!(
            "---\nname: demo\ndescription: Traversal fixture\n---\n\n# Demo Skill\n\nVersion {version}\n\nSee [outside](../../../../outside.md).\n"
        ),
        _ => format!(
            "---\nname: demo\ndescription: Signed demo Skill\n---\n\n# Demo Skill\n\nVersion {version}\n\nSee [Guide](references/guide.md) and use `scripts/run.sh`.\n"
        ),
    }
    .into_bytes();
    let sbom = if matches!(mutation, ArchiveMutation::InvalidSbom) {
        json!({"format":"unknown"}).to_string().into_bytes()
    } else {
        json!({"bomFormat":"CycloneDX","specVersion":"1.5"})
            .to_string()
            .into_bytes()
    };
    let mut files = BTreeMap::from([
        (".codex-plugin/plugin.json".to_string(), manifest.to_vec()),
        ("skills/demo/SKILL.md".to_string(), skill),
        ("sbom.json".to_string(), sbom),
    ]);
    files.extend(extra_files);
    match mutation {
        ArchiveMutation::SkillReferenceCycle => {
            files.insert(
                "skills/demo/references/a.md".to_string(),
                b"Continue with [B](b.md).\n".to_vec(),
            );
            files.insert(
                "skills/demo/references/b.md".to_string(),
                b"Return to [A](a.md).\n".to_vec(),
            );
        }
        ArchiveMutation::SkillTraversalReference => {}
        _ => {
            files.insert(
                "skills/demo/references/guide.md".to_string(),
                b"# Guide\n\nSee [common](../../../references/common.md).\n".to_vec(),
            );
            files.insert(
                "references/common.md".to_string(),
                b"# Common reference\n".to_vec(),
            );
            files.insert(
                "skills/demo/scripts/run.sh".to_string(),
                b"#!/bin/sh\nprintf '%s\\n' demo\n".to_vec(),
            );
        }
    }
    let mut checksums = files
        .iter()
        .map(|(path, content)| (path.clone(), sha256_bytes(content)))
        .collect::<BTreeMap<_, _>>();
    if matches!(mutation, ArchiveMutation::MissingChecksum) {
        checksums.remove("skills/demo/SKILL.md");
    }
    if matches!(mutation, ArchiveMutation::WrongChecksum) {
        checksums.insert("skills/demo/SKILL.md".to_string(), "0".repeat(64));
    }
    files.insert(
        ".codex-plugin/checksums.json".to_string(),
        json!({"schemaVersion":1,"files":checksums})
            .to_string()
            .into_bytes(),
    );

    let file = File::create(path).expect("create test archive");
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .unix_permissions(0o644);
    for (name, content) in &files {
        let file_options = if name.starts_with("mcp/")
            || name.starts_with("scripts/")
            || name.starts_with("binaries/")
        {
            options.unix_permissions(0o755)
        } else {
            options
        };
        writer
            .start_file(name, file_options)
            .expect("start ZIP file");
        writer.write_all(content).expect("write ZIP file");
    }
    match mutation {
        ArchiveMutation::Traversal => {
            writer
                .start_file("../outside", options)
                .expect("start traversal entry");
            writer.write_all(b"outside").expect("write traversal entry");
        }
        ArchiveMutation::Symlink => writer
            .add_symlink("skills/demo/link", "../../outside", options)
            .expect("add symlink entry"),
        ArchiveMutation::Duplicate => {
            writer
                .start_file("skills/demo/skill.md", options)
                .expect("start case-colliding entry");
            writer
                .write_all(b"case collision")
                .expect("write case-colliding entry");
        }
        ArchiveMutation::None
        | ArchiveMutation::MissingChecksum
        | ArchiveMutation::WrongChecksum
        | ArchiveMutation::InvalidSbom
        | ArchiveMutation::SkillReferenceCycle
        | ArchiveMutation::SkillTraversalReference => {}
    }
    writer.finish().expect("finish test archive");
}

fn sha256_bytes(value: &[u8]) -> String {
    hex::encode(Sha256::digest(value))
}
