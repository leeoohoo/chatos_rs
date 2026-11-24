use super::*;

#[test]
fn retry_guidance_and_attachments_remain_durable() {
    let mut retry = claim(Value::Null, None);
    retry.run.model_attempt = 2;
    assert_eq!(
        durable_step_input(&retry, "message").expect("retry").1,
        "model_retry"
    );

    let guidance = Some(json!({
        "type": "guidance",
        "guidance": [{"message_id": "message-2", "message": "inspect tests", "attachments": []}]
    }));
    let (items, reason) =
        durable_step_input(&claim(Value::Null, guidance), "message").expect("guidance");
    assert_eq!(reason, "initial_request_with_guidance");
    assert_eq!(items.len(), 2);

    let mut attachment = claim(Value::Null, None);
    attachment.run.input = json!({
        "message": "",
        "attachments": [{
            "attachment_id": "attachment-1",
            "display_name": "brief.pdf",
            "media_type": "application/pdf",
            "byte_size": 42,
            "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "authorized_local_ref": "local-attachment:authority-1",
            "metadata": {"must_not_be_forwarded": true}
        }]
    });
    let (items, _) = durable_step_input(&attachment, "message").expect("attachment");
    let manifest = items[0]["content"][0]["text"].as_str().expect("manifest");
    assert!(manifest.contains("local-attachment:authority-1"));
    assert!(!manifest.contains("must_not_be_forwarded"));
}

#[test]
fn ask_user_resume_becomes_function_call_output_instead_of_a_new_user_message() {
    let checkpoint = json!({
        "response": {
            "request_input_items": [{"role": "user", "content": "build it"}],
            "response_output_items": [{
                "type": "function_call",
                "name": "ask_user_prompt_choices",
                "call_id": "ask-call-1",
                "arguments": "{}"
            }]
        }
    });
    let continuation = json!({
        "type": "resume",
        "reason": "ask_user_submitted",
        "input": {
            "source": "ask_user",
            "tool_call_id": "ask-call-1",
            "values": {},
            "selection": "local"
        }
    });
    let (items, reason) = durable_step_input(&claim(checkpoint, Some(continuation)), "message")
        .expect("Ask User continuation");
    assert_eq!(reason, "user_resume");
    let output = items.last().expect("function output");
    assert_eq!(output["type"], "function_call_output");
    assert_eq!(output["call_id"], "ask-call-1");
    assert!(output["output"]
        .as_str()
        .is_some_and(|value| value.contains("\"selection\":\"local\"")));
}

#[test]
fn survey_answers_resume_the_matching_tool_call_for_main_chat_and_task() {
    for profile in [MAIN_CHAT_PROFILE_KEY, TASK_EXECUTION_PROFILE_KEY] {
        let checkpoint = json!({"response": {
            "request_input_items": [{"role": "user", "content": "help define the project"}],
            "response_output_items": [{"type": "function_call", "name": "requirement_survey_create",
                "call_id": "survey-call-1", "arguments": "{}"}]
        }});
        let continuation = json!({"type": "requirement_survey_resolved", "survey_id": "survey-1",
            "answers": {"scope": "prototype", "editable": true}});
        let mut run_claim = claim(checkpoint, Some(continuation));
        run_claim.run.profile_key = profile.into();
        let (items, reason) = durable_step_input(&run_claim, "message").unwrap();
        assert_eq!(reason, "requirement_survey_resolved");
        let output = items.last().unwrap();
        assert_eq!(output["type"], "function_call_output");
        assert_eq!(output["call_id"], "survey-call-1");
        let value: Value = serde_json::from_str(output["output"].as_str().unwrap()).unwrap();
        assert_eq!(value["answers"]["scope"], "prototype");
        assert_eq!(value["answers"]["editable"], true);
        assert_eq!(items.len(), 3);
    }
    assert!(requirement_survey_output_item(&[], &json!({"answers": {}})).is_err());
    assert!(requirement_survey_output_item(
        &[json!({"type": "function_call",
        "name": "requirement_survey_create", "call_id": "call-1"})],
        &json!({})
    )
    .is_err());
}
