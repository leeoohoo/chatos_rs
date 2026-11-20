use super::*;

pub(super) async fn inspect_mcp_tools(
    package_root: &Path,
    executable: &Path,
    arguments: &[String],
    manifest_environment: &BTreeMap<String, String>,
) -> Result<Vec<Value>, String> {
    if !manifest_environment.is_empty() {
        return Err(install_error("npm MCP 预检不接受环境变量"));
    }
    let package_root = package_root
        .canonicalize()
        .map_err(|_| install_error("npm MCP 预检目录无效"))?;
    let executable = executable
        .canonicalize()
        .map_err(|_| install_error("npm MCP 启动器无效"))?;
    if !executable.is_file() || !executable.starts_with(&package_root) {
        return Err(install_error("npm MCP 启动器越界"));
    }
    let mut command = Command::new(&executable);
    command
        .args(arguments)
        .current_dir(&package_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(false)
        .env_clear()
        .env("CHATOS_PLUGIN_ROOT", &package_root);
    for key in ["SystemRoot", "WINDIR", "ComSpec", "PATHEXT", "TEMP", "TMP"] {
        if let Some(value) = std::env::var_os(key) {
            command.env(key, value);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.as_std_mut().creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|_| install_error("无法启动 npm MCP 隔离预检"))?;
    let termination = ProcessTermination::attach(&child).map_err(|_| {
        let _ = child.start_kill();
        install_error("无法隔离 npm MCP 预检进程")
    })?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| install_error("npm MCP 标准输入不可用"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| install_error("npm MCP 标准输出不可用"))?;
    let mut stdout = BufReader::new(stdout);
    let inspection = async {
        let initialization = mcp_request(
            &mut stdin,
            &mut stdout,
            1,
            "initialize",
            json!({
                "protocolVersion": MCP_PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "chatos-windows-installer", "version": env!("CARGO_PKG_VERSION")}
            }),
        )
        .await?;
        if initialization
            .get("protocolVersion")
            .and_then(Value::as_str)
            != Some(MCP_PROTOCOL_VERSION)
            || !initialization
                .get("serverInfo")
                .is_some_and(Value::is_object)
        {
            return Err(install_error("npm MCP 初始化响应无效"));
        }
        mcp_notify(&mut stdin, "notifications/initialized", json!({})).await?;
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;
        let mut observed_cursors = BTreeSet::new();
        for page in 0..MAXIMUM_MCP_TOOL_PAGES {
            let parameters = cursor
                .as_ref()
                .map(|cursor| json!({"cursor": cursor}))
                .unwrap_or_else(|| json!({}));
            let result = mcp_request(
                &mut stdin,
                &mut stdout,
                2 + page as u64,
                "tools/list",
                parameters,
            )
            .await?;
            let returned = result
                .get("tools")
                .and_then(Value::as_array)
                .ok_or_else(|| install_error("npm MCP tools/list 响应无效"))?;
            if tools.len().saturating_add(returned.len()) > MAXIMUM_MCP_TOOLS {
                return Err(install_error("npm MCP 工具数量超过限制"));
            }
            tools.extend(returned.iter().cloned());
            cursor = result
                .get("nextCursor")
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
            let Some(next) = cursor.as_ref() else {
                return validate_mcp_tools(tools);
            };
            if !observed_cursors.insert(next.clone()) {
                return Err(install_error("npm MCP 返回了重复工具游标"));
            }
        }
        Err(install_error("npm MCP 工具分页超过限制"))
    };
    let result = match timeout(MCP_INSPECTION_TIMEOUT, inspection).await {
        Ok(result) => result,
        Err(_) => Err(install_error("npm MCP 预检超过 30 秒")),
    };
    terminate_child(&mut child, &termination).await;
    result
}

pub(super) async fn mcp_request(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
    request_id: u64,
    method: &str,
    parameters: Value,
) -> Result<Value, String> {
    mcp_write(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": request_id,
            "method": method,
            "params": parameters
        }),
    )
    .await?;
    loop {
        let response = mcp_read(stdout).await?;
        if response.get("id").and_then(Value::as_u64) != Some(request_id) {
            continue;
        }
        if response.get("error").is_some() {
            return Err(install_error(&format!("npm MCP {method} 调用失败")));
        }
        return response
            .get("result")
            .cloned()
            .filter(Value::is_object)
            .ok_or_else(|| install_error("npm MCP 响应缺少 result"));
    }
}

pub(super) async fn mcp_notify(
    stdin: &mut ChildStdin,
    method: &str,
    parameters: Value,
) -> Result<(), String> {
    mcp_write(
        stdin,
        &json!({"jsonrpc": "2.0", "method": method, "params": parameters}),
    )
    .await
}

pub(super) async fn mcp_write(stdin: &mut ChildStdin, value: &Value) -> Result<(), String> {
    let mut payload =
        serde_json::to_vec(value).map_err(|_| install_error("无法编码 npm MCP 请求"))?;
    payload.push(b'\n');
    stdin
        .write_all(&payload)
        .await
        .map_err(|_| install_error("无法写入 npm MCP 请求"))?;
    stdin
        .flush()
        .await
        .map_err(|_| install_error("无法刷新 npm MCP 请求"))
}

pub(super) async fn mcp_read(stdout: &mut BufReader<ChildStdout>) -> Result<Value, String> {
    let mut payload = Vec::new();
    loop {
        let available = stdout
            .fill_buf()
            .await
            .map_err(|_| install_error("无法读取 npm MCP 响应"))?;
        if available.is_empty() {
            return Err(install_error("npm MCP 进程在响应前退出"));
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        let content_length = newline.unwrap_or(available.len());
        if payload.len().saturating_add(content_length) > MAXIMUM_MCP_RESPONSE_BYTES {
            return Err(install_error("npm MCP 响应超过限制"));
        }
        payload.extend_from_slice(&available[..content_length]);
        stdout.consume(consumed);
        if newline.is_some() {
            break;
        }
    }
    while matches!(payload.last(), Some(b'\n' | b'\r')) {
        payload.pop();
    }
    serde_json::from_slice(&payload).map_err(|_| install_error("npm MCP 响应不是有效 JSON"))
}

pub(super) fn validate_mcp_tools(tools: Vec<Value>) -> Result<Vec<Value>, String> {
    let mut names = BTreeSet::new();
    let mut total_schema_bytes = 0_usize;
    for tool in &tools {
        let name = tool
            .get("name")
            .and_then(Value::as_str)
            .filter(|value| {
                !value.is_empty()
                    && value.len() <= 1_024
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.:|".contains(&byte))
            })
            .ok_or_else(|| install_error("npm MCP 工具名称无效"))?;
        if !names.insert(name.to_owned()) {
            return Err(install_error("npm MCP 工具名称重复"));
        }
        let description = tool
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if description.chars().count() > 16_000 || description.contains('\0') {
            return Err(install_error("npm MCP 工具说明无效"));
        }
        let schema = tool
            .get("inputSchema")
            .cloned()
            .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
        let schema_bytes =
            serde_json::to_vec(&schema).map_err(|_| install_error("npm MCP 工具 schema 无效"))?;
        total_schema_bytes = total_schema_bytes
            .checked_add(schema_bytes.len())
            .filter(|value| *value <= MAXIMUM_MCP_TOOL_CATALOG_BYTES)
            .ok_or_else(|| install_error("npm MCP 工具目录超过限制"))?;
        if !schema.is_object() || schema_bytes.len() > MAXIMUM_MCP_TOOL_SCHEMA_BYTES {
            return Err(install_error("npm MCP 工具 schema 无效或超过限制"));
        }
    }
    let mut tools = tools;
    tools.sort_by(|left, right| {
        left.get("name")
            .and_then(Value::as_str)
            .cmp(&right.get("name").and_then(Value::as_str))
    });
    Ok(tools)
}

pub(super) async fn terminate_child(child: &mut Child, termination: &ProcessTermination) {
    let _ = termination.terminate(false);
    if timeout(PROCESS_TERMINATION_GRACE, child.wait())
        .await
        .is_err()
    {
        let _ = termination.terminate(true);
        let _ = timeout(PROCESS_TERMINATION_GRACE, child.wait()).await;
    }
}
