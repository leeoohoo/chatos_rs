async fn get_message_task_runner_run(
    auth: AuthUser,
    Path((message_id, run_id)): Path<(String, String)>,
    Query(query): Query<MessageTaskRunnerLookupQuery>,
) -> (StatusCode, Json<Value>) {
    let context = match resolve_message_task_runner_context(&auth, &message_id, &query).await {
        Ok(Some(context)) => context,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "当前消息没有关联的任务来源"})),
            );
        }
        Err(err) => return err,
    };
    let payload = match task_runner_api_client::get_message_run(
        context.base_url.as_str(),
        run_id.as_str(),
        context.source_session_id.as_str(),
        context.source_user_message_id.as_deref(),
        context.source_turn_id.as_deref(),
        query.event_limit(),
        query.event_offset(),
        query.include_events(),
    )
    .await
    {
        Ok(payload) => payload,
        Err(err) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(json!({"error": "读取运行详情失败", "detail": err})),
            );
        }
    };
    let matches = payload.get("task").is_some_and(|task| {
        task_matches_message_source(
            task,
            context.source_session_id.as_str(),
            context.source_user_message_id.as_deref(),
            context.source_turn_id.as_deref(),
        )
    });
    if !matches {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "运行记录不属于当前消息"})),
        );
    }
    (StatusCode::OK, Json(payload))
}

async fn retry_message_task_runner_run(
    auth: AuthUser,
    Path((message_id, run_id)): Path<(String, String)>,
    Query(query): Query<MessageTaskRunnerLookupQuery>,
    body: Bytes,
) -> (StatusCode, Json<Value>) {
    let payload = match parse_retry_message_task_runner_run_request(body.as_ref()) {
        Ok(payload) => payload,
        Err(err) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({
                    "error": "重试请求格式不正确",
                    "detail": err.to_string(),
                })),
            );
        }
    };
    let context = match resolve_message_task_runner_context(&auth, &message_id, &query).await {
        Ok(Some(context)) => context,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "当前消息没有关联的任务来源"})),
            );
        }
        Err(err) => return err,
    };
    let retry_instruction = normalize_text(payload.retry_instruction.as_deref());
    let execution_service_id = normalize_text(payload.execution_service_id.as_deref());
    if retry_instruction
        .as_deref()
        .is_some_and(|value| value.chars().count() > 4000)
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "阻塞处理意见不能超过 4000 个字符"})),
        );
    }
    if execution_service_id
        .as_deref()
        .is_some_and(|value| value.chars().count() > 255)
    {
        return (
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "执行服务 ID 不能超过 255 个字符"})),
        );
    }
    match task_runner_api_client::retry_message_run(
        context.base_url.as_str(),
        run_id.as_str(),
        context.source_session_id.as_str(),
        context.source_user_message_id.as_deref(),
        context.source_turn_id.as_deref(),
        retry_instruction.as_deref(),
        execution_service_id.as_deref(),
    )
    .await
    {
        Ok(payload) => (StatusCode::CREATED, Json(payload)),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "重试任务节点失败", "detail": err})),
        ),
    }
}

async fn get_message_task_runner_run_changes(
    auth: AuthUser,
    Path((message_id, run_id)): Path<(String, String)>,
    Query(query): Query<MessageTaskRunnerLookupQuery>,
) -> (StatusCode, Json<Value>) {
    let context = match resolve_message_task_runner_context(&auth, &message_id, &query).await {
        Ok(Some(context)) => context,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "当前消息没有关联的任务来源"})),
            );
        }
        Err(err) => return err,
    };
    match task_runner_api_client::get_message_run_changes(
        context.base_url.as_str(),
        run_id.as_str(),
        context.source_session_id.as_str(),
        context.source_user_message_id.as_deref(),
        context.source_turn_id.as_deref(),
    )
    .await
    {
        Ok(payload) => (StatusCode::OK, Json(payload)),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "读取任务代码变更失败", "detail": err})),
        ),
    }
}

async fn retry_message_task_runner_run_integration(
    auth: AuthUser,
    Path((message_id, run_id)): Path<(String, String)>,
    Query(query): Query<MessageTaskRunnerLookupQuery>,
) -> (StatusCode, Json<Value>) {
    let context = match resolve_message_task_runner_context(&auth, &message_id, &query).await {
        Ok(Some(context)) => context,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "当前消息没有关联的任务来源"})),
            );
        }
        Err(err) => return err,
    };
    match task_runner_api_client::retry_message_run_integration(
        context.base_url.as_str(),
        run_id.as_str(),
        context.source_session_id.as_str(),
        context.source_user_message_id.as_deref(),
        context.source_turn_id.as_deref(),
    )
    .await
    {
        Ok(payload) => (StatusCode::OK, Json(payload)),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "重新集成任务代码失败", "detail": err})),
        ),
    }
}

async fn waive_message_task_runner_run_integration(
    auth: AuthUser,
    Path((message_id, run_id)): Path<(String, String)>,
    Query(query): Query<MessageTaskRunnerLookupQuery>,
    Json(request): Json<WaiveMessageTaskRunnerRunIntegrationRequest>,
) -> (StatusCode, Json<Value>) {
    let context = match resolve_message_task_runner_context(&auth, &message_id, &query).await {
        Ok(Some(context)) => context,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"error": "当前消息没有关联的任务来源"})),
            );
        }
        Err(err) => return err,
    };
    match task_runner_api_client::waive_message_run_integration(
        context.base_url.as_str(),
        run_id.as_str(),
        context.source_session_id.as_str(),
        context.source_user_message_id.as_deref(),
        context.source_turn_id.as_deref(),
        request.reason.as_str(),
    )
    .await
    {
        Ok(payload) => (StatusCode::OK, Json(payload)),
        Err(err) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error": "放弃任务代码变更失败", "detail": err})),
        ),
    }
}

#[cfg(test)]
include!("message_task_runner_inline_tests.rs");
