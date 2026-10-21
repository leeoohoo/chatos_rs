use chatos_local_agent_protocol::{
    GetRunCommand, GetRunSkillActivationsCommand, HostCommand, HostResult, LocalAgentToolOutcome,
    LocalSkillBundleSnapshot, LocalSkillDocumentSnapshot,
};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    pin::Pin,
    sync::Arc,
};
use tokio::sync::Mutex;

use crate::{
    agent_plugin_runtime::{AGENT_SKILL_ACTIVATE_TOOL_NAME, AGENT_SKILL_READ_RESOURCE_TOOL_NAME},
    host_lifecycle::HostManager,
};

const PRODUCT_SKILL_REFERENCE_PREFIX: &str = "product-skill:";
const PLUGIN_SKILL_REFERENCE_PREFIX: &str = "skill_";
const MAXIMUM_SKILL_PAGE_CHARACTERS: usize = 160_000;
const MAXIMUM_RUNTIME_SESSIONS: usize = 512;

pub(crate) type ProductSkillHostFuture<'a> =
    Pin<Box<dyn Future<Output = Result<HostResult, String>> + Send + 'a>>;

pub(crate) trait ProductSkillHost: Send + Sync {
    fn execute(&self, command: HostCommand) -> ProductSkillHostFuture<'_>;
}

impl ProductSkillHost for HostManager {
    fn execute(&self, command: HostCommand) -> ProductSkillHostFuture<'_> {
        Box::pin(HostManager::execute(self, command))
    }
}

#[derive(Clone, Default)]
pub(crate) struct ProductSkillRuntime {
    sessions: Arc<Mutex<HashMap<String, ProductSkillSession>>>,
}

#[derive(Clone, Debug)]
struct ProductSkillSession {
    bundle_fingerprint: String,
    activated_skill_refs: HashSet<String>,
}

#[derive(Clone, Debug)]
struct VerifiedProductSkillBundle {
    snapshot: LocalSkillBundleSnapshot,
    fingerprint: String,
}

impl ProductSkillRuntime {
    pub(crate) async fn reset(&self) {
        self.sessions.lock().await.clear();
    }

    /// Executes a frozen Product Skill call and returns `None` only for a
    /// well-formed opaque Plugin Skill reference. This keeps Product Skills
    /// available when a Todo has no Plugin plan and prevents forged Product
    /// refs from starting every selected Plugin process.
    pub(crate) async fn execute_if_product(
        &self,
        host: &dyn ProductSkillHost,
        owner_user_id: &str,
        run_id: &str,
        tool_name: &str,
        arguments: &Map<String, Value>,
    ) -> Result<Option<LocalAgentToolOutcome>, String> {
        let request = ProductSkillRequest::decode(tool_name, arguments)?;
        if is_opaque_plugin_skill_ref(request.skill_ref()) {
            return Ok(None);
        }
        let bundle = load_verified_bundle(host, owner_user_id, run_id).await?;
        let document = bundle
            .snapshot
            .document(request.skill_ref())
            .ok_or_else(product_skill_unavailable)?;
        if !request
            .skill_ref()
            .starts_with(PRODUCT_SKILL_REFERENCE_PREFIX)
        {
            return Err(product_skill_unavailable());
        }

        let session_key = format!("{owner_user_id}\0{run_id}");
        let durable = load_durable_activations(host, owner_user_id, run_id).await?;
        let mut sessions = self.sessions.lock().await;
        let bundle_changed = sessions
            .get(&session_key)
            .is_some_and(|session| session.bundle_fingerprint != bundle.fingerprint);
        if !sessions.contains_key(&session_key) {
            if sessions.len() >= MAXIMUM_RUNTIME_SESSIONS {
                return Err("Too many Product Skill sessions are active.".into());
            }
            sessions.insert(
                session_key.clone(),
                ProductSkillSession {
                    bundle_fingerprint: bundle.fingerprint.clone(),
                    activated_skill_refs: durable
                        .into_iter()
                        .filter(|skill_ref| bundle.snapshot.document(skill_ref).is_some())
                        .collect(),
                },
            );
        } else if bundle_changed {
            // A Run bundle is intended to be immutable. If it changes while
            // this process is alive, do not carry activations across content.
            sessions.insert(
                session_key.clone(),
                ProductSkillSession {
                    bundle_fingerprint: bundle.fingerprint.clone(),
                    activated_skill_refs: HashSet::new(),
                },
            );
        }
        let session = sessions
            .get_mut(&session_key)
            .ok_or_else(product_skill_runtime_error)?;
        let outcome = match request {
            ProductSkillRequest::Activate { skill_ref } => {
                session.activated_skill_refs.insert(skill_ref.to_owned());
                activation_outcome(document)
            }
            ProductSkillRequest::ReadResource {
                skill_ref,
                relative_path,
                offset,
                maximum_characters,
            } => {
                if !session.activated_skill_refs.contains(skill_ref) {
                    return Err("Activate this Product Skill before reading its resources.".into());
                }
                resource_outcome(document, relative_path, offset, maximum_characters)?
            }
        };
        Ok(Some(outcome))
    }
}

enum ProductSkillRequest<'a> {
    Activate {
        skill_ref: &'a str,
    },
    ReadResource {
        skill_ref: &'a str,
        relative_path: &'a str,
        offset: usize,
        maximum_characters: usize,
    },
}

impl<'a> ProductSkillRequest<'a> {
    fn decode(tool_name: &str, arguments: &'a Map<String, Value>) -> Result<Self, String> {
        match tool_name {
            AGENT_SKILL_ACTIVATE_TOOL_NAME => {
                require_allowed_keys(arguments, &["skill_ref"])?;
                Ok(Self::Activate {
                    skill_ref: required_string(arguments, "skill_ref", 512)?,
                })
            }
            AGENT_SKILL_READ_RESOURCE_TOOL_NAME => {
                require_allowed_keys(
                    arguments,
                    &["skill_ref", "relative_path", "offset", "max_chars"],
                )?;
                Ok(Self::ReadResource {
                    skill_ref: required_string(arguments, "skill_ref", 512)?,
                    relative_path: required_string(arguments, "relative_path", 1_024)?,
                    offset: optional_u64(arguments, "offset", 0, usize::MAX as u64)?.unwrap_or(0)
                        as usize,
                    maximum_characters: optional_u64(
                        arguments,
                        "max_chars",
                        1,
                        MAXIMUM_SKILL_PAGE_CHARACTERS as u64,
                    )?
                    .unwrap_or(MAXIMUM_SKILL_PAGE_CHARACTERS as u64)
                        as usize,
                })
            }
            _ => Err(product_skill_arguments_error()),
        }
    }

    fn skill_ref(&self) -> &str {
        match self {
            Self::Activate { skill_ref } | Self::ReadResource { skill_ref, .. } => skill_ref,
        }
    }
}

async fn load_verified_bundle(
    host: &dyn ProductSkillHost,
    owner_user_id: &str,
    run_id: &str,
) -> Result<VerifiedProductSkillBundle, String> {
    let result = host
        .execute(HostCommand::GetRunSkillBundle(GetRunCommand {
            owner_user_id: owner_user_id.to_owned(),
            run_id: run_id.to_owned(),
        }))
        .await?;
    let HostResult::RunSkillBundle {
        run_id: returned_run_id,
        bundle: Some(bundle),
    } = result
    else {
        return Err(product_skill_unavailable());
    };
    if returned_run_id != run_id {
        return Err(product_skill_runtime_error());
    }
    VerifiedProductSkillBundle::new(bundle)
}

async fn load_durable_activations(
    host: &dyn ProductSkillHost,
    owner_user_id: &str,
    run_id: &str,
) -> Result<Vec<String>, String> {
    let result = host
        .execute(HostCommand::GetRunSkillActivations(
            GetRunSkillActivationsCommand {
                owner_user_id: owner_user_id.to_owned(),
                run_id: run_id.to_owned(),
            },
        ))
        .await?;
    let HostResult::RunSkillActivations {
        run_id: returned_run_id,
        skill_refs,
    } = result
    else {
        return Err(product_skill_runtime_error());
    };
    if returned_run_id != run_id {
        return Err(product_skill_runtime_error());
    }
    Ok(skill_refs)
}

impl VerifiedProductSkillBundle {
    fn new(snapshot: LocalSkillBundleSnapshot) -> Result<Self, String> {
        snapshot
            .validate()
            .map_err(|_| product_skill_runtime_error())?;
        for document in &snapshot.documents {
            require_digest(&document.instructions, &document.instructions_sha256)?;
            for resource in &document.resources {
                require_digest(&resource.content, &resource.sha256)?;
            }
        }
        let expected_revision = bundle_revision(&snapshot)?;
        if !digest_matches(&expected_revision, &snapshot.bundle_revision) {
            return Err(product_skill_runtime_error());
        }
        let encoded = canonical_json(&snapshot)?;
        let fingerprint = format!("sha256:{:x}", Sha256::digest(encoded));
        Ok(Self {
            snapshot,
            fingerprint,
        })
    }
}

fn activation_outcome(document: &LocalSkillDocumentSnapshot) -> LocalAgentToolOutcome {
    LocalAgentToolOutcome::Succeeded {
        output: json!({
            "activated": true,
            "skill_ref": document.skill_ref,
            "kind": "operational",
            "name": document.name,
            "role": document.role,
            "description": document.description,
            "instructions": document.instructions,
            "instructions_sha256": document.instructions_sha256,
            "resources": document.resources.iter()
                .map(|resource| resource.relative_path.as_str())
                .collect::<Vec<_>>(),
        }),
    }
}

fn resource_outcome(
    document: &LocalSkillDocumentSnapshot,
    relative_path: &str,
    offset: usize,
    maximum_characters: usize,
) -> Result<LocalAgentToolOutcome, String> {
    let resource = document
        .resources
        .iter()
        .find(|resource| resource.relative_path == relative_path)
        .ok_or_else(product_skill_unavailable)?;
    let scalars = resource.content.chars().collect::<Vec<_>>();
    if offset > scalars.len() {
        return Err(product_skill_arguments_error());
    }
    let end = offset.saturating_add(maximum_characters).min(scalars.len());
    let content = scalars[offset..end].iter().collect::<String>();
    Ok(LocalAgentToolOutcome::Succeeded {
        output: json!({
            "skill_ref": document.skill_ref,
            "relative_path": resource.relative_path,
            "sha256": resource.sha256,
            "content": content,
            "offset": offset,
            "next_offset": (end < scalars.len()).then_some(end),
            "truncated": end < scalars.len(),
        }),
    })
}

fn bundle_revision(bundle: &LocalSkillBundleSnapshot) -> Result<String, String> {
    let mut unversioned = bundle.clone();
    unversioned.bundle_revision = "unversioned".into();
    let encoded = canonical_json(&unversioned)?;
    Ok(format!("sha256:{:x}", Sha256::digest(encoded)))
}

fn canonical_json<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let value = serde_json::to_value(value).map_err(|_| product_skill_runtime_error())?;
    serde_json::to_vec(&value).map_err(|_| product_skill_runtime_error())
}

fn require_digest(content: &str, expected: &str) -> Result<(), String> {
    let actual = format!("sha256:{:x}", Sha256::digest(content.as_bytes()));
    digest_matches(&actual, expected)
        .then_some(())
        .ok_or_else(product_skill_runtime_error)
}

fn digest_matches(actual: &str, expected: &str) -> bool {
    actual
        .strip_prefix("sha256:")
        .zip(expected.strip_prefix("sha256:").or(Some(expected)))
        .is_some_and(|(actual, expected)| actual.eq_ignore_ascii_case(expected))
}

fn is_opaque_plugin_skill_ref(skill_ref: &str) -> bool {
    skill_ref
        .strip_prefix(PLUGIN_SKILL_REFERENCE_PREFIX)
        .is_some_and(|digest| {
            digest.len() == 64
                && digest
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        })
}

fn require_allowed_keys(arguments: &Map<String, Value>, allowed: &[&str]) -> Result<(), String> {
    (!arguments.keys().any(|key| !allowed.contains(&key.as_str())))
        .then_some(())
        .ok_or_else(product_skill_arguments_error)
}

fn required_string<'a>(
    arguments: &'a Map<String, Value>,
    field: &str,
    maximum: usize,
) -> Result<&'a str, String> {
    arguments
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.trim() == *value
                && value.chars().count() <= maximum
                && !value.chars().any(char::is_control)
        })
        .ok_or_else(product_skill_arguments_error)
}

fn optional_u64(
    arguments: &Map<String, Value>,
    field: &str,
    minimum: u64,
    maximum: u64,
) -> Result<Option<u64>, String> {
    match arguments.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .filter(|value| (minimum..=maximum).contains(value))
            .map(Some)
            .ok_or_else(product_skill_arguments_error),
    }
}

fn product_skill_arguments_error() -> String {
    "The Product Skill arguments are invalid.".into()
}

fn product_skill_unavailable() -> String {
    "The Product Skill is unavailable in the current Run snapshot.".into()
}

fn product_skill_runtime_error() -> String {
    "The current Run Product Skill snapshot failed integrity validation.".into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chatos_local_agent_protocol::{
        LocalSkillResourceSnapshot, LocalToolSkillActivationPolicy, LocalToolSkillBindingSnapshot,
    };
    use std::sync::Mutex as StandardMutex;

    struct FakeProductSkillHost {
        bundle: StandardMutex<LocalSkillBundleSnapshot>,
        activations: StandardMutex<Vec<String>>,
        commands: StandardMutex<Vec<&'static str>>,
    }

    impl FakeProductSkillHost {
        fn new(bundle: LocalSkillBundleSnapshot, activations: Vec<String>) -> Self {
            Self {
                bundle: StandardMutex::new(bundle),
                activations: StandardMutex::new(activations),
                commands: StandardMutex::new(Vec::new()),
            }
        }

        fn replace_bundle(&self, bundle: LocalSkillBundleSnapshot) {
            *self.bundle.lock().unwrap() = bundle;
        }
    }

    impl ProductSkillHost for FakeProductSkillHost {
        fn execute(&self, command: HostCommand) -> ProductSkillHostFuture<'_> {
            Box::pin(async move {
                match command {
                    HostCommand::GetRunSkillBundle(command) => {
                        self.commands.lock().unwrap().push("bundle");
                        Ok(HostResult::RunSkillBundle {
                            run_id: command.run_id,
                            bundle: Some(self.bundle.lock().unwrap().clone()),
                        })
                    }
                    HostCommand::GetRunSkillActivations(command) => {
                        self.commands.lock().unwrap().push("activations");
                        Ok(HostResult::RunSkillActivations {
                            run_id: command.run_id,
                            skill_refs: self.activations.lock().unwrap().clone(),
                        })
                    }
                    _ => panic!("unexpected Product Skill command"),
                }
            })
        }
    }

    fn digest(content: &str) -> String {
        format!("sha256:{:x}", Sha256::digest(content.as_bytes()))
    }

    fn snapshot() -> LocalSkillBundleSnapshot {
        let mut bundle = LocalSkillBundleSnapshot {
            schema_version: 2,
            bundle_revision: "unversioned".into(),
            documents: vec![LocalSkillDocumentSnapshot {
                skill_ref: "product-skill:test".into(),
                name: "test".into(),
                role: "specialist".into(),
                description: "Test the frozen runtime".into(),
                instructions: "Follow the frozen instructions.".into(),
                instructions_sha256: digest("Follow the frozen instructions."),
                resources: vec![LocalSkillResourceSnapshot {
                    relative_path: "references/guide.md".into(),
                    content: "A😀BC".into(),
                    sha256: digest("A😀BC"),
                }],
            }],
            tool_skill_bindings: vec![LocalToolSkillBindingSnapshot {
                provider_id: "test.provider".into(),
                binding_id: "test.binding".into(),
                tool_names: vec!["test_tool".into()],
                router_skill_ref: "product-skill:test".into(),
                specialist_skill_ref: "product-skill:test".into(),
                activation_policy: LocalToolSkillActivationPolicy::OnDemand,
            }],
        };
        bundle.bundle_revision = bundle_revision(&bundle).unwrap();
        bundle
    }

    #[test]
    fn verifies_frozen_content_and_rejects_digest_changes() {
        let bundle = snapshot();
        assert!(VerifiedProductSkillBundle::new(bundle.clone()).is_ok());
        let mut tampered = bundle;
        tampered.documents[0].instructions.push_str(" changed");
        assert!(VerifiedProductSkillBundle::new(tampered).is_err());
    }

    #[test]
    fn rejects_a_bundle_revision_that_does_not_bind_the_snapshot() {
        let mut bundle = snapshot();
        bundle.bundle_revision = format!("sha256:{}", "0".repeat(64));
        assert!(VerifiedProductSkillBundle::new(bundle).is_err());
    }

    #[test]
    fn pages_resources_by_unicode_scalar_and_rejects_unknown_paths() {
        let bundle = snapshot();
        let document = &bundle.documents[0];
        let LocalAgentToolOutcome::Succeeded { output } =
            resource_outcome(document, "references/guide.md", 1, 2).unwrap()
        else {
            panic!("resource outcome")
        };
        assert_eq!(output["content"], json!("😀B"));
        assert_eq!(output["next_offset"], json!(3));
        assert_eq!(output["truncated"], json!(true));
        assert!(resource_outcome(document, "../guide.md", 0, 2).is_err());
        assert!(resource_outcome(document, "references/guide.md", 10, 2).is_err());
    }

    #[test]
    fn routes_only_well_formed_opaque_plugin_refs_to_the_plugin_runtime() {
        assert!(is_opaque_plugin_skill_ref(&format!(
            "skill_{}",
            "a".repeat(64)
        )));
        assert!(!is_opaque_plugin_skill_ref("product-skill:test"));
        assert!(!is_opaque_plugin_skill_ref("skill_forged"));
        assert!(!is_opaque_plugin_skill_ref(&format!(
            "skill_{}",
            "A".repeat(64)
        )));
    }

    #[test]
    fn bundle_fingerprint_changes_with_content_and_activation_shape_matches_macos() {
        let first = VerifiedProductSkillBundle::new(snapshot()).unwrap();
        let LocalAgentToolOutcome::Succeeded { output } =
            activation_outcome(&first.snapshot.documents[0])
        else {
            panic!("activation outcome")
        };
        assert_eq!(output["activated"], json!(true));
        assert_eq!(output["skill_ref"], json!("product-skill:test"));
        assert_eq!(output["resources"][0], json!("references/guide.md"));

        let mut changed = snapshot();
        changed.documents[0].description = "Changed description".into();
        changed.bundle_revision = bundle_revision(&changed).unwrap();
        let changed = VerifiedProductSkillBundle::new(changed).unwrap();
        assert_ne!(first.fingerprint, changed.fingerprint);
    }

    #[tokio::test]
    async fn product_skill_works_without_a_plugin_plan_and_requires_activation() {
        let host = FakeProductSkillHost::new(snapshot(), vec![]);
        let runtime = ProductSkillRuntime::default();
        let read = Map::from_iter([
            ("skill_ref".into(), json!("product-skill:test")),
            ("relative_path".into(), json!("references/guide.md")),
        ]);
        assert!(runtime
            .execute_if_product(
                &host,
                "owner-1",
                "run-1",
                AGENT_SKILL_READ_RESOURCE_TOOL_NAME,
                &read,
            )
            .await
            .is_err());

        let activate = Map::from_iter([("skill_ref".into(), json!("product-skill:test"))]);
        assert!(runtime
            .execute_if_product(
                &host,
                "owner-1",
                "run-1",
                AGENT_SKILL_ACTIVATE_TOOL_NAME,
                &activate,
            )
            .await
            .unwrap()
            .is_some());
        let LocalAgentToolOutcome::Succeeded { output } = runtime
            .execute_if_product(
                &host,
                "owner-1",
                "run-1",
                AGENT_SKILL_READ_RESOURCE_TOOL_NAME,
                &read,
            )
            .await
            .unwrap()
            .unwrap()
        else {
            panic!("resource outcome")
        };
        assert_eq!(output["content"], json!("A😀BC"));
    }

    #[tokio::test]
    async fn restores_only_current_bundle_activations_and_clears_them_on_change() {
        let host = FakeProductSkillHost::new(snapshot(), vec!["product-skill:test".into()]);
        let runtime = ProductSkillRuntime::default();
        let read = Map::from_iter([
            ("skill_ref".into(), json!("product-skill:test")),
            ("relative_path".into(), json!("references/guide.md")),
        ]);
        assert!(runtime
            .execute_if_product(
                &host,
                "owner-1",
                "run-1",
                AGENT_SKILL_READ_RESOURCE_TOOL_NAME,
                &read,
            )
            .await
            .unwrap()
            .is_some());

        let mut changed = snapshot();
        changed.documents[0].description = "Replacement snapshot".into();
        changed.bundle_revision = bundle_revision(&changed).unwrap();
        host.replace_bundle(changed);
        assert!(runtime
            .execute_if_product(
                &host,
                "owner-1",
                "run-1",
                AGENT_SKILL_READ_RESOURCE_TOOL_NAME,
                &read,
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn opaque_plugin_refs_bypass_product_bundle_loading_but_forged_refs_do_not() {
        let host = FakeProductSkillHost::new(snapshot(), vec![]);
        let runtime = ProductSkillRuntime::default();
        let plugin = Map::from_iter([(
            "skill_ref".into(),
            json!(format!("skill_{}", "a".repeat(64))),
        )]);
        assert!(runtime
            .execute_if_product(
                &host,
                "owner-1",
                "run-1",
                AGENT_SKILL_ACTIVATE_TOOL_NAME,
                &plugin,
            )
            .await
            .unwrap()
            .is_none());
        assert!(host.commands.lock().unwrap().is_empty());

        let forged = Map::from_iter([("skill_ref".into(), json!("skill_forged"))]);
        assert!(runtime
            .execute_if_product(
                &host,
                "owner-1",
                "run-1",
                AGENT_SKILL_ACTIVATE_TOOL_NAME,
                &forged,
            )
            .await
            .is_err());
        assert_eq!(host.commands.lock().unwrap().as_slice(), ["bundle"]);
    }
}
