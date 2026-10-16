use super::*;

#[path = "planner_attachment_transport_tests.rs"]
mod transport_tests;

fn image_continuation() -> Value {
    json!({"type":"tool_results", "invocations":[{
        "call_id":"attachment-call", "tool_name":"local_attachment_read", "status":"succeeded",
        "result":{
            "text":"{\"sha256_verified\":true,\"model_image_supplied\":true}",
            "_structured_result":{"sha256_verified":true,"model_image_supplied":true},
            "_model_input":[{"type":"input_image","image_url":"data:image/png;base64,iVBORw0KGgo=","detail":"high"}]
        }
    }]})
}

fn image_claim(task: bool) -> LocalAgentRunClaim {
    let mut current = claim(
        json!({"response":{
            "request_input_items":[{"role":"user","content":"请帮我回复截图里的邮件"}],
            "response_output_items":[{"type":"function_call","call_id":"attachment-call","name":"local_attachment_read","arguments":"{}"}]
        }}),
        Some(image_continuation()),
    );
    if task {
        current.run.profile_key = TASK_EXECUTION_PROFILE_KEY.into();
        current.run.input["tool_options"] = json!({
            "requires_execution": false,
            "enabled_builtin_kinds": [],
            "plugin_hints": []
        });
    }
    current
}

#[tokio::test]
async fn main_chat_and_task_attachment_outputs_become_real_image_input() {
    for task in [false, true] {
        let planner = if task {
            ControlPlaneLocalAiStepPlanner::task_execution(
                OwnerCheckingModelResolver,
                OwnerCheckingCapabilityResolver,
            )
        } else {
            ControlPlaneLocalAiStepPlanner::main_chat(
                OwnerCheckingModelResolver,
                OwnerCheckingCapabilityResolver,
            )
        };
        let prepared = planner.prepare_ai_step(&image_claim(task)).await.unwrap();
        let items = &prepared.request.current_input_items;
        assert!(items.iter().any(|item| item.pointer("/content/0/image_url")
            == Some(&json!("data:image/png;base64,iVBORw0KGgo="))));
        let tool_output = items
            .iter()
            .find(|item| item["type"] == "function_call_output")
            .unwrap();
        assert!(tool_output["output"]
            .as_str()
            .unwrap()
            .contains("sha256_verified"));
        assert!(!tool_output["output"].as_str().unwrap().contains("base64"));
        assert!(!serde_json::to_string(&prepared.external_tool_results)
            .unwrap()
            .contains("base64"));
    }
}

#[test]
fn attachment_image_is_retained_across_subsequent_steps_and_retry() {
    let (items, _) = durable_step_input(&image_claim(false), "message").unwrap();
    let mut next = claim(
        json!({"response":{
            "request_input_items":items,
            "response_output_items":[{"role":"assistant","content":"已读取，准备拟稿"}]
        }}),
        None,
    );
    next.run.model_attempt = 2;
    let (retried, _) = durable_step_input(&next, "message").unwrap();
    assert!(retried
        .iter()
        .any(|item| item.pointer("/content/0/image_url")
            == Some(&json!("data:image/png;base64,iVBORw0KGgo="))));
}

struct TextOnlyModel;
#[async_trait]
impl LocalModelRuntimeResolver for TextOnlyModel {
    async fn resolve_model_runtime(
        &self,
        owner: &str,
        reference: &str,
        revision: &str,
    ) -> Result<TransientLocalModelRuntime, String> {
        let mut resolved = OwnerCheckingModelResolver
            .resolve_model_runtime(owner, reference, revision)
            .await?;
        resolved.model_config.supports_images = Some(false);
        Ok(resolved)
    }
}

#[tokio::test]
async fn text_only_model_gets_specific_diagnosis_not_invalid_image_request() {
    let prepared =
        ControlPlaneLocalAiStepPlanner::main_chat(TextOnlyModel, OwnerCheckingCapabilityResolver)
            .prepare_ai_step(&image_claim(false))
            .await
            .unwrap();
    assert!(!prepared
        .request
        .current_input_items
        .iter()
        .any(|item| item.pointer("/content/0/image_url").is_some()));
    assert!(prepared
        .request
        .prefixed_input_items
        .iter()
        .any(|item| item["content"]
            .as_str()
            .is_some_and(|text| text.contains("explicitly does not support images"))));
}

#[test]
fn malformed_image_result_does_not_become_visual_evidence() {
    let mut current = image_claim(false);
    current.run.continuation_input.as_mut().unwrap()["invocations"][0]["result"]["_model_input"]
        [0]["image_url"] = json!("file:///private/image.png");
    let (items, _) = durable_step_input(&current, "message").unwrap();
    assert!(!items
        .iter()
        .any(|item| item.pointer("/content/0/image_url").is_some()));
}
