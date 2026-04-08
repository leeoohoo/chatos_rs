use serde_json::{json, Value};

use super::parsing::parse_task_drafts;
use super::{TaskPlannerOptions, TaskPlannerService};

fn contains_schema_key(node: &Value, key: &str) -> bool {
    match node {
        Value::Object(map) => map
            .iter()
            .any(|(name, value)| name == key || contains_schema_key(value, key)),
        Value::Array(items) => items.iter().any(|item| contains_schema_key(item, key)),
        _ => false,
    }
}

#[test]
fn parse_task_drafts_supports_single_task_shape() {
    let args = json!({ "title": "Ship task planner", "priority": "high" });
    let drafts = parse_task_drafts(&args).expect("single task payload should parse");
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].title, "Ship task planner");
    assert_eq!(drafts[0].priority, "high");
    assert_eq!(drafts[0].status, "pending_confirm");
}

#[test]
fn parse_task_drafts_falls_back_to_single_task_when_tasks_array_is_empty() {
    let args = json!({
        "title": "Ship task planner",
        "details": "single task fallback",
        "priority": "high",
        "tasks": [],
    });
    let drafts = parse_task_drafts(&args).expect("empty tasks array should fall back");
    assert_eq!(drafts.len(), 1);
    assert_eq!(drafts[0].title, "Ship task planner");
    assert_eq!(drafts[0].details, "single task fallback");
    assert_eq!(drafts[0].priority, "high");
}

#[test]
fn parse_task_drafts_supports_simplified_task_requirements() {
    let args = json!({
        "title": "排查任务创建失败",
        "details": "优先复用当前联系人能力",
        "required_builtin_capabilities": ["read", "terminal"],
        "required_context_assets": [
            { "asset_type": "skill", "asset_ref": "SK1" },
            { "asset_type": "common", "asset_ref": "CMD_DEPLOY" }
        ]
    });

    let drafts = parse_task_drafts(&args).expect("simplified task payload should parse");
    assert_eq!(drafts.len(), 1);
    assert_eq!(
        drafts[0].required_builtin_capabilities,
        vec!["read".to_string(), "terminal".to_string()]
    );
    assert_eq!(drafts[0].required_context_assets.len(), 2);
    assert_eq!(drafts[0].required_context_assets[0].asset_type, "skill");
    assert_eq!(drafts[0].required_context_assets[0].asset_ref, "SK1");
}

#[test]
fn parse_task_drafts_supports_task_graph_fields() {
    let args = json!({
        "tasks": [
            {
                "task_ref": "impl_api",
                "task_kind": "implementation",
                "title": "实现接口",
                "depends_on_refs": [],
                "acceptance_criteria": ["接口可用"]
            },
            {
                "task_ref": "verify_api",
                "task_kind": "verification",
                "title": "验证接口",
                "depends_on_refs": ["impl_api"],
                "verification_of_refs": ["impl_api"],
                "acceptance_criteria": ["输出验证结果"]
            }
        ]
    });

    let drafts = parse_task_drafts(&args).expect("task graph payload should parse");
    assert_eq!(drafts.len(), 2);
    assert_eq!(drafts[0].task_ref.as_deref(), Some("impl_api"));
    assert_eq!(drafts[0].task_kind.as_deref(), Some("implementation"));
    assert_eq!(drafts[1].depends_on_refs, vec!["impl_api".to_string()]);
    assert_eq!(drafts[1].verification_of_refs, vec!["impl_api".to_string()]);
    assert_eq!(drafts[1].acceptance_criteria, vec!["输出验证结果".to_string()]);
}

#[test]
fn create_tasks_schema_is_strict_and_compatible() {
    let service = TaskPlannerService::new(TaskPlannerOptions {
        server_name: "task_planner".to_string(),
        review_timeout_ms: 120_000,
    })
    .expect("task planner service should initialize");

    let create_tasks_tool = service
        .list_tools()
        .into_iter()
        .find(|tool| tool.get("name").and_then(Value::as_str) == Some("create_tasks"))
        .expect("create_tasks tool should exist");

    let schema = create_tasks_tool
        .get("inputSchema")
        .expect("create_tasks should expose inputSchema");

    assert_eq!(
        schema.get("additionalProperties"),
        Some(&Value::Bool(false))
    );

    let task_item_schema = schema
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|props| props.get("tasks"))
        .and_then(|tasks| tasks.get("items"))
        .expect("tasks.items schema should exist");

    assert_eq!(
        task_item_schema.get("additionalProperties"),
        Some(&Value::Bool(false))
    );

    assert!(
        !contains_schema_key(schema, "oneOf"),
        "create_tasks schema should not contain oneOf"
    );
    assert!(
        !contains_schema_key(schema, "planned_builtin_mcp_ids"),
        "create_tasks schema should not expose internal planned_builtin_mcp_ids"
    );
    assert!(
        !contains_schema_key(schema, "planned_context_assets"),
        "create_tasks schema should not expose internal planned_context_assets"
    );
    assert!(
        contains_schema_key(schema, "task_ref"),
        "create_tasks schema should expose task_ref for graph planning"
    );
    assert!(
        contains_schema_key(schema, "depends_on_refs"),
        "create_tasks schema should expose depends_on_refs for graph planning"
    );
    assert!(
        contains_schema_key(schema, "verification_of_refs"),
        "create_tasks schema should expose verification_of_refs for graph planning"
    );

    let execution_result_contract_schema = schema
        .get("properties")
        .and_then(Value::as_object)
        .and_then(|props| props.get("execution_result_contract"))
        .expect("execution_result_contract schema should exist");
    assert_eq!(
        execution_result_contract_schema.get("additionalProperties"),
        Some(&Value::Bool(false))
    );
}

#[test]
fn task_planner_tools_expose_only_planning_actions() {
    let service = TaskPlannerService::new(TaskPlannerOptions {
        server_name: "task_planner".to_string(),
        review_timeout_ms: 120_000,
    })
    .expect("task planner service should initialize");

    let tools = service.list_tools();
    let tool_names: Vec<&str> = tools
        .iter()
        .filter_map(|tool| tool.get("name").and_then(Value::as_str))
        .collect();

    assert!(tool_names.contains(&"list_tasks"));
    assert!(tool_names.contains(&"create_tasks"));
    assert!(tool_names.contains(&"confirm_task"));
    assert!(tool_names.contains(&"get_contact_builtin_mcp_grants"));
    assert!(tool_names.contains(&"list_contact_runtime_assets"));
    assert!(!tool_names.contains(&"update_task"));
    assert!(!tool_names.contains(&"complete_task"));
    assert!(!tool_names.contains(&"delete_task"));
}
