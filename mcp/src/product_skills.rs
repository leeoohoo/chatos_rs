// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

use sha2::{Digest, Sha256};

const PRODUCT_SKILL_ROOT: &str = "product-skill:";

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
        [
            resource!(
                "chatos-async-task-orchestration",
                "references/acceptance-examples-and-antipatterns.md"
            ),
            resource!(
                "chatos-async-task-orchestration",
                "references/capabilities-and-plugins.md"
            ),
            resource!(
                "chatos-async-task-orchestration",
                "references/complexity-and-decomposition.md"
            ),
            resource!(
                "chatos-async-task-orchestration",
                "references/design-and-creative.md"
            ),
            resource!(
                "chatos-async-task-orchestration",
                "references/documents-presentations-and-media.md"
            ),
            resource!(
                "chatos-async-task-orchestration",
                "references/followups-cancellation-and-recovery.md"
            ),
            resource!(
                "chatos-async-task-orchestration",
                "references/graph-patterns-and-dependencies.md"
            ),
            resource!(
                "chatos-async-task-orchestration",
                "references/research-data-and-operations.md"
            ),
            resource!(
                "chatos-async-task-orchestration",
                "references/software-engineering.md"
            ),
            resource!(
                "chatos-async-task-orchestration",
                "references/writing-content-and-marketing.md"
            ),
        ]
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

    #[test]
    fn every_bound_system_tool_skill_resolves_from_the_central_bundle() {
        let mut required_skills = BTreeSet::new();
        for descriptor in system_mcp_catalog() {
            let SystemMcpToolCatalog::Static(tools) =
                system_mcp_tool_catalog(descriptor.key).expect("tool catalog");
            let tool_names = tools
                .iter()
                .map(|tool| {
                    tool.get("name")
                        .and_then(serde_json::Value::as_str)
                        .expect("static tool name")
                        .to_string()
                })
                .collect::<Vec<_>>();
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
    fn task_orchestration_bundle_tracks_the_current_split_resources() {
        let document = product_skill_document("chatos-async-task-orchestration")
            .expect("task orchestration Skill");
        assert_eq!(document.resources.len(), 10);
        assert!(document
            .resource("references/capabilities-and-plugins.md")
            .is_some());
        assert!(document
            .resource("references/capabilities-lifecycle-and-recovery.md")
            .is_none());
    }

    #[test]
    fn product_skill_lookup_is_exact_after_outer_whitespace() {
        let document = product_skill_document(" chatos-terminal ").expect("terminal Skill");
        assert_eq!(document.name, "chatos-terminal");
        assert_eq!(document.skill_ref(), "product-skill:chatos-terminal");
        assert!(product_skill_document("CHATOS-terminal").is_none());
        assert!(document.resource("references/missing.md").is_none());
    }
}
