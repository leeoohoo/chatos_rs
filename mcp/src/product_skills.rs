// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::SystemMcpProductSkillBinding;

const PRODUCT_SKILL_ROOT: &str = "product-skill:";
pub const PRODUCT_SKILL_RUNTIME_RESOURCE_ID: &str = "chatos_product_skill_runtime";
pub const PRODUCT_SKILL_RUNTIME_SERVER_NAME: &str = "product_skill";
pub const PRODUCT_SKILL_LIST_RESOURCES_TOOL: &str = "list_resources";
pub const PRODUCT_SKILL_READ_RESOURCE_TOOL: &str = "read_resource";
const PRODUCT_SKILL_RUNTIME_SKILLS: &[&str] = &["chatos-skill-runtime"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductSkillResource {
    pub relative_path: &'static str,
    pub content: &'static str,
}

impl ProductSkillResource {
    pub fn content_sha256(self) -> String {
        sha256(self.content)
    }

    pub fn size_bytes(self) -> usize {
        self.content.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProductSkillDocument {
    pub name: &'static str,
    pub instructions: &'static str,
    pub resources: &'static [ProductSkillResource],
}

impl ProductSkillDocument {
    pub fn skill_ref(self) -> String {
        format!("{PRODUCT_SKILL_ROOT}{}", self.name)
    }

    pub fn instructions_sha256(self) -> String {
        sha256(self.instructions)
    }

    pub fn resource(self, relative_path: &str) -> Option<ProductSkillResource> {
        self.resources
            .iter()
            .copied()
            .find(|resource| resource.relative_path == relative_path)
    }
}

macro_rules! resource {
    ($skill:literal, $path:literal) => {
        ProductSkillResource {
            relative_path: $path,
            content: include_str!(concat!(
                "../../clients/macos/Sources/ChatOSCore/AgentSkills/Skills/",
                $skill,
                "/",
                $path
            )),
        }
    };
}

macro_rules! skill {
    ($name:literal, [$($resource:expr),* $(,)?]) => {
        ProductSkillDocument {
            name: $name,
            instructions: include_str!(concat!(
                "../../clients/macos/Sources/ChatOSCore/AgentSkills/Skills/",
                $name,
                "/SKILL.md"
            )),
            resources: &[$($resource),*],
        }
    };
}

static PRODUCT_SKILLS: &[ProductSkillDocument] = &[
    skill!("chatos-project-files", []),
    skill!(
        "chatos-project-read",
        [resource!("chatos-project-read", "references/scenarios.md")]
    ),
    skill!(
        "chatos-project-write",
        [resource!(
            "chatos-project-write",
            "references/transactions-and-conflicts.md"
        )]
    ),
    skill!("chatos-terminal", []),
    skill!("chatos-skill-runtime", []),
    skill!(
        "chatos-terminal-command-execution",
        [resource!(
            "chatos-terminal-command-execution",
            "references/scenarios.md"
        )]
    ),
    skill!(
        "chatos-terminal-process-observation",
        [resource!(
            "chatos-terminal-process-observation",
            "references/scenarios.md"
        )]
    ),
    skill!(
        "chatos-terminal-process-control",
        [resource!(
            "chatos-terminal-process-control",
            "references/scenarios.md"
        )]
    ),
    skill!("requirement-survey", []),
    skill!(
        "requirement-survey-create",
        [resource!(
            "requirement-survey-create",
            "references/example.md"
        )]
    ),
    skill!(
        "requirement-survey-read-results",
        [resource!(
            "requirement-survey-read-results",
            "references/example.md"
        )]
    ),
    skill!(
        "requirement-survey-resolve",
        [resource!(
            "requirement-survey-resolve",
            "references/example.md"
        )]
    ),
    skill!(
        "requirement-survey-review-execution",
        [resource!(
            "requirement-survey-review-execution",
            "references/example.md"
        )]
    ),
    skill!(
        "chatos-agent-builder",
        [resource!(
            "chatos-agent-builder",
            "references/draft-quality.md"
        )]
    ),
    skill!(
        "chatos-remote-connection",
        [resource!(
            "chatos-remote-connection",
            "references/commands-and-transfers.md"
        )]
    ),
    skill!(
        "chatos-user-clarification",
        [resource!(
            "chatos-user-clarification",
            "references/forms-secrets-and-recovery.md"
        )]
    ),
    skill!(
        "chatos-notepad",
        [resource!(
            "chatos-notepad",
            "references/organization-and-mutations.md"
        )]
    ),
    skill!(
        "chatos-memory-context",
        [resource!(
            "chatos-memory-context",
            "references/expansion-boundaries.md"
        )]
    ),
    skill!(
        "chatos-command-approval",
        [resource!(
            "chatos-command-approval",
            "references/evidence-and-decisions.md"
        )]
    ),
    skill!(
        "chatos-task-progress",
        [resource!(
            "chatos-task-progress",
            "references/progress-examples.md"
        )]
    ),
    skill!(
        "chatos-async-task-orchestration",
        [resource!(
            "chatos-async-task-orchestration",
            "references/capabilities-lifecycle-and-recovery.md"
        )]
    ),
];

pub fn product_skill_document(name: &str) -> Option<ProductSkillDocument> {
    let name = name.trim();
    PRODUCT_SKILLS
        .iter()
        .copied()
        .find(|document| document.name == name)
}

pub fn product_skill_documents() -> &'static [ProductSkillDocument] {
    PRODUCT_SKILLS
}

pub fn product_skill_runtime_binding(tool_name: &str) -> Option<SystemMcpProductSkillBinding> {
    match tool_name.trim() {
        PRODUCT_SKILL_LIST_RESOURCES_TOOL | PRODUCT_SKILL_READ_RESOURCE_TOOL => {
            Some(SystemMcpProductSkillBinding {
                binding_id: "product-skill.control-plane",
                primary_skill: "chatos-skill-runtime",
                required_skills: PRODUCT_SKILL_RUNTIME_SKILLS,
                coverage_revision: 1,
            })
        }
        _ => None,
    }
}

pub fn product_skill_runtime_tool_definitions() -> Vec<Value> {
    vec![
        json!({
            "name": PRODUCT_SKILL_LIST_RESOURCES_TOOL,
            "description": "List the supporting resources for one product-skill Skill already bound to this Runtime Session. This never activates an unbound Skill or expands permissions.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "skill_ref": {
                        "type": "string",
                        "description": "Exact product-skill: reference disclosed in protected Skill context."
                    }
                },
                "required": ["skill_ref"],
                "additionalProperties": false
            },
            "annotations": {"readOnlyHint": true}
        }),
        json!({
            "name": PRODUCT_SKILL_READ_RESOURCE_TOOL,
            "description": "Read one paginated supporting resource from a product Skill already bound to this Runtime Session. First list resources and pass back the exact path and content hash.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "skill_ref": {
                        "type": "string",
                        "description": "Exact product-skill: reference disclosed in protected Skill context."
                    },
                    "relative_path": {
                        "type": "string",
                        "description": "Exact relative resource path returned by list_resources."
                    },
                    "content_sha256": {
                        "type": "string",
                        "pattern": "^[0-9a-f]{64}$",
                        "description": "Exact content hash returned by list_resources."
                    },
                    "offset": {
                        "type": "integer",
                        "minimum": 0,
                        "default": 0,
                        "description": "Unicode character offset returned by the previous page."
                    },
                    "max_chars": {
                        "type": "integer",
                        "minimum": 1,
                        "maximum": 20000,
                        "default": 12000
                    }
                },
                "required": ["skill_ref", "relative_path", "content_sha256"],
                "additionalProperties": false
            },
            "annotations": {"readOnlyHint": true}
        }),
    ]
}

fn sha256(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::{
        system_mcp_catalog, system_mcp_product_skill_binding, system_mcp_tool_catalog,
        SystemMcpToolCatalog,
    };

    const TASK_RUNNER_TOOLS: &[&str] = &[
        "list_tasks",
        "get_task",
        "create_task",
        "create_tasks_with_prerequisites",
        "cancel_task",
        "wait_for_task_completion",
        "get_task_dependency_graph",
    ];

    #[test]
    fn every_bound_system_tool_skill_resolves_from_the_central_bundle() {
        let mut required_skills = BTreeSet::new();
        for descriptor in system_mcp_catalog() {
            let tool_names = match system_mcp_tool_catalog(descriptor.key).expect("tool catalog") {
                SystemMcpToolCatalog::Static(tools) => tools
                    .iter()
                    .map(|tool| {
                        tool.get("name")
                            .and_then(serde_json::Value::as_str)
                            .expect("static tool name")
                            .to_string()
                    })
                    .collect::<Vec<_>>(),
                SystemMcpToolCatalog::Dynamic => TASK_RUNNER_TOOLS
                    .iter()
                    .map(|name| (*name).to_string())
                    .collect(),
            };
            for tool_name in tool_names {
                let binding = system_mcp_product_skill_binding(descriptor.key, tool_name.as_str())
                    .unwrap_or_else(|| {
                        panic!(
                            "{}:{} is missing its product Skill binding",
                            descriptor.key.as_str(),
                            tool_name
                        )
                    });
                required_skills.extend(binding.required_skills.iter().copied());
            }
        }

        for skill_name in required_skills {
            let document = product_skill_document(skill_name)
                .unwrap_or_else(|| panic!("{skill_name} is missing from the product Skill bundle"));
            assert!(!document.instructions.trim().is_empty(), "{skill_name}");
            assert_eq!(document.instructions_sha256().len(), 64, "{skill_name}");
            for resource in document.resources {
                assert!(
                    resource.relative_path.starts_with("references/"),
                    "{}:{}",
                    skill_name,
                    resource.relative_path
                );
                assert!(!resource.content.trim().is_empty(), "{skill_name}");
                assert_eq!(resource.content_sha256().len(), 64, "{skill_name}");
            }
        }
    }

    #[test]
    fn product_skill_resources_are_relative_and_unique() {
        let mut skill_names = BTreeSet::new();
        for document in product_skill_documents() {
            assert!(skill_names.insert(document.name), "{}", document.name);
            let mut resource_paths = BTreeSet::new();
            for resource in document.resources {
                assert!(resource_paths.insert(resource.relative_path));
                assert!(!resource.relative_path.starts_with('/'));
                assert!(!resource.relative_path.split('/').any(|part| part == ".."));
            }
        }
    }

    #[test]
    fn product_skill_lookup_is_exact_after_outer_whitespace() {
        let document = product_skill_document(" chatos-terminal ").expect("terminal Skill");
        assert_eq!(document.name, "chatos-terminal");
        assert_eq!(document.skill_ref(), "product-skill:chatos-terminal");
        assert!(product_skill_document("CHATOS-terminal").is_none());
        assert!(document.resource("references/missing.md").is_none());
    }

    #[test]
    fn product_skill_control_tools_are_read_only_and_bound() {
        let tools = product_skill_runtime_tool_definitions();
        assert_eq!(tools.len(), 2);
        for tool in tools {
            let name = tool["name"].as_str().expect("tool name");
            assert_eq!(
                tool.pointer("/annotations/readOnlyHint"),
                Some(&json!(true))
            );
            assert_eq!(
                product_skill_runtime_binding(name)
                    .expect("control binding")
                    .primary_skill,
                "chatos-skill-runtime"
            );
        }
    }
}
