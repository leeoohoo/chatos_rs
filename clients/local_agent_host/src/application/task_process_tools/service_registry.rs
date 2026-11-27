use super::*;

mod execution;

#[derive(Clone)]
pub struct LocalTaskProcessToolExecutor {
    runtime: Arc<LocalAgentRuntime>,
    owner_user_id: String,
    services: Arc<Mutex<RunServicePortRegistry>>,
}

impl LocalTaskProcessToolExecutor {
    pub fn new(
        runtime: Arc<LocalAgentRuntime>,
        owner_user_id: impl Into<String>,
    ) -> Result<Self, String> {
        let owner_user_id = owner_user_id.into().trim().to_string();
        if owner_user_id.is_empty() || owner_user_id.len() > 256 {
            return Err("Task process tool owner must be 1..=256 characters".to_string());
        }
        Ok(Self {
            runtime,
            owner_user_id,
            services: Arc::new(Mutex::new(RunServicePortRegistry::default())),
        })
    }

    async fn validate_run(&self, run_id: &str) -> Result<LocalAgentRunRecord, String> {
        let run = self
            .runtime
            .get_run_for_host_worker(run_id)
            .await
            .map_err(|error| error.to_string())?
            .ok_or_else(|| format!("Task Run not found: {run_id}"))?;
        if run.owner_user_id != self.owner_user_id
            || run.profile_key != "task_execution"
            || run.owner_entity_type != "task"
        {
            return Err(
                "Task process tools are available only to the active Task execution".to_string(),
            );
        }
        Ok(run)
    }

    async fn recovery_method_already_tried(
        &self,
        run: &LocalAgentRunRecord,
        blocker: &BlockerReport,
    ) -> Result<bool, String> {
        let fingerprint = blocker
            .failure_fingerprint
            .as_deref()
            .map(str::trim)
            .unwrap_or_default();
        let next_method = blocker
            .next_method
            .as_deref()
            .map(str::trim)
            .unwrap_or_default();
        if fingerprint.is_empty() || next_method.is_empty() {
            return Ok(false);
        }
        Ok(self
            .runtime
            .successful_tool_invocations_for_run_for_host_worker(
                &run.owner_user_id,
                &run.run_id,
                TASK_OUTCOME_REPORT_TOOL,
            )
            .await
            .map_err(|error| error.to_string())?
            .iter()
            .filter_map(|invocation| invocation.result.as_ref())
            .any(|result| {
                result.get("reported").and_then(Value::as_bool) == Some(false)
                    && result.get("classification").and_then(Value::as_str)
                        == Some("retry_with_changed_method")
                    && result
                        .get("failure_fingerprint")
                        .and_then(Value::as_str)
                        .is_some_and(|value| value.trim() == fingerprint)
                    && result
                        .get("next_method")
                        .and_then(Value::as_str)
                        .is_some_and(|value| value.trim() == next_method)
            }))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ProcessOperation {
    Append,
    Replace,
    Clear,
}

fn default_process_operation() -> ProcessOperation {
    ProcessOperation::Append
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordProcessArgs {
    #[serde(default = "default_process_operation")]
    operation: ProcessOperation,
    #[serde(default)]
    heading: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    acceptance_checks: Vec<ProcessAcceptanceCheck>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProcessAcceptanceCheck {
    criterion_index: u64,
    status: String,
    #[serde(default)]
    evidence: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ServiceOperation {
    Allocate,
    Release,
}

fn default_service_operation() -> ServiceOperation {
    ServiceOperation::Allocate
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AllocateServiceArgs {
    #[serde(default = "default_service_operation")]
    operation: ServiceOperation,
    service_name: String,
    #[serde(default)]
    preferred_port: Option<u16>,
    #[serde(default = "default_service_scheme")]
    scheme: String,
}

fn default_service_scheme() -> String {
    "http".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ServiceAllocation {
    pub(super) port: u16,
    scheme: String,
}

#[derive(Default)]
pub(super) struct RunServicePortRegistry {
    allocations: BTreeMap<(String, String), ServiceAllocation>,
}

impl RunServicePortRegistry {
    pub(super) fn allocate(
        &mut self,
        run_id: &str,
        service_name: &str,
        preferred_port: Option<u16>,
        scheme: &str,
    ) -> Result<ServiceAllocation, String> {
        let key = (run_id.to_string(), service_name.to_string());
        if let Some(existing) = self.allocations.get(&key) {
            return Ok(existing.clone());
        }
        let reserved = self
            .allocations
            .values()
            .map(|allocation| allocation.port)
            .collect::<BTreeSet<_>>();
        let port = match preferred_port.filter(|port| !reserved.contains(port)) {
            Some(port) if loopback_port_is_available(port) => port,
            _ => allocate_unreserved_loopback_port(&reserved)?,
        };
        let allocation = ServiceAllocation {
            port,
            scheme: scheme.to_string(),
        };
        self.allocations.insert(key, allocation.clone());
        Ok(allocation)
    }

    pub(super) fn release(
        &mut self,
        run_id: &str,
        service_name: &str,
    ) -> Option<ServiceAllocation> {
        self.allocations
            .remove(&(run_id.to_string(), service_name.to_string()))
    }
}

fn loopback_port_is_available(port: u16) -> bool {
    TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)).is_ok()
}

fn allocate_unreserved_loopback_port(reserved: &BTreeSet<u16>) -> Result<u16, String> {
    for _ in 0..32 {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .map_err(|error| format!("failed to allocate loopback service port: {error}"))?;
        let port = listener
            .local_addr()
            .map_err(|error| format!("failed to inspect loopback service port: {error}"))?
            .port();
        if !reserved.contains(&port) {
            return Ok(port);
        }
    }
    Err("failed to allocate an unreserved loopback service port".to_string())
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ReportedResultStatus {
    Succeeded,
    Failed,
    Blocked,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReportOutcomeArgs {
    #[serde(alias = "status")]
    result_status: ReportedResultStatus,
    reason: String,
    #[serde(default)]
    final_response: Option<String>,
    #[serde(default)]
    acceptance_checks: Option<Vec<AcceptanceCheck>>,
    #[serde(default)]
    verification_report: Option<Value>,
    #[serde(default)]
    blocker: Option<BlockerReport>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum BlockerClassification {
    HandledLocally,
    RetryWithChangedMethod,
    ReplanRequired,
    UserActionRequired,
    UnknownSideEffectsNeedsReview,
}

impl BlockerClassification {
    const fn as_str(self) -> &'static str {
        match self {
            Self::HandledLocally => "handled_locally",
            Self::RetryWithChangedMethod => "retry_with_changed_method",
            Self::ReplanRequired => "replan_required",
            Self::UserActionRequired => "user_action_required",
            Self::UnknownSideEffectsNeedsReview => "unknown_side_effects_needs_review",
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BlockerReport {
    classification: BlockerClassification,
    evidence: String,
    #[serde(default)]
    failure_fingerprint: Option<String>,
    #[serde(default)]
    previous_method: Option<String>,
    #[serde(default)]
    next_method: Option<String>,
    #[serde(default)]
    affected_task_ids: Vec<String>,
    #[serde(default)]
    user_question: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptanceCheck {
    criterion_index: u64,
    status: String,
    evidence: String,
}
