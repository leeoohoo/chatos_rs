use super::*;

pub(super) struct ParsedCodexHookBundle {
    hooks: Vec<LocalPluginHookDefinition>,
    events: Vec<String>,
    content_sha256: String,
    normalized_hook_sha256: String,
}

pub(super) fn validate_local_package(package_root: &Path) -> Result<PluginManifest, String> {
    let raw = read_bounded_text(
        &package_root.join("chatos.plugin.json"),
        MAXIMUM_MANIFEST_BYTES,
    )?;
    let manifest = parse_plugin_manifest(&raw).map_err(|_error| {
        #[cfg(test)]
        eprintln!("local plugin manifest parse diagnostic: {_error}");
        install_error("本地 Plugin 清单无效")
    })?;
    validate_plugin_manifest(&manifest).map_err(|_error| {
        #[cfg(test)]
        eprintln!("local plugin manifest validation diagnostic: {_error}");
        install_error("本地 Plugin 清单无效")
    })?;
    if !windows_compatible(&manifest.dependencies.supported_platforms) {
        return Err(install_error("本地 Plugin 不支持 Windows"));
    }
    load_agent_plugin_skill_snapshot(
        package_root,
        &manifest.name,
        manifest.skills.iter().map(|skill| skill.path.as_str()),
    )
    .map_err(|_| install_error("本地 Plugin Skill 无效"))?;
    let hook_summaries = inspect_plugin_hooks(package_root, &manifest)?;
    let mut executable_count = 0_usize;
    for server in &manifest.mcp_servers {
        if let PluginMcpServer::Stdio { bin, .. } = server {
            resolve_executable(package_root, bin)?;
            executable_count += 1;
        }
    }
    if executable_count == 0
        && manifest.skills.is_empty()
        && (hook_summaries.is_empty() || !manifest.mcp_servers.is_empty())
    {
        return Err(install_error(
            "本地 Plugin 尚无可运行的 stdio MCP、有效 Skill 或受支持的 Hook",
        ));
    }
    Ok(manifest)
}

pub(super) fn inspect_plugin_hooks(
    package_root: &Path,
    manifest: &PluginManifest,
) -> Result<Vec<LocalPluginHookSummary>, String> {
    if manifest.hooks.len() > MAXIMUM_HOOK_BUNDLES {
        return Err(install_error("Codex Hook Bundle 数量无效"));
    }
    manifest
        .hooks
        .iter()
        .map(|hook| {
            let relative = normalized_hook_relative_path(&hook.source.path)?;
            let source = secure_hook_source(package_root, &relative)?;
            let raw = read_bounded_text(&source, MAXIMUM_HOOK_BUNDLE_BYTES)?;
            let parsed = parse_codex_hook_bundle(&raw)?;
            Ok(LocalPluginHookSummary {
                component_key: hook.component_key.clone(),
                hook_count: parsed.hooks.len(),
                events: parsed.events,
                content_sha256: parsed.content_sha256,
                normalized_hook_sha256: parsed.normalized_hook_sha256,
                hooks: parsed.hooks,
            })
        })
        .collect()
}

pub(super) fn normalized_hook_relative_path(raw_path: &str) -> Result<PathBuf, String> {
    let mut path = raw_path.trim();
    while let Some(stripped) = path.strip_prefix("./") {
        path = stripped;
    }
    if path.is_empty() || path.contains('\0') {
        return Err(install_error("Hook path 必须位于 Plugin 目录内"));
    }
    safe_relative_path(Path::new(path))
        .map_err(|_| install_error("Hook path 必须位于 Plugin 目录内"))
}

pub(super) fn secure_hook_source(package_root: &Path, relative: &Path) -> Result<PathBuf, String> {
    let root = package_root
        .canonicalize()
        .map_err(|_| install_error("Plugin 安装目录无效"))?;
    let source = package_root.join(relative);
    let metadata = fs::symlink_metadata(&source)
        .map_err(|_| install_error("Hook 文件不存在或发生链接逃逸"))?;
    if !metadata.is_file() || is_symlink_or_reparse(&metadata) {
        return Err(install_error("Hook 文件不存在或发生链接逃逸"));
    }
    let resolved = source
        .canonicalize()
        .map_err(|_| install_error("Hook 文件不存在或发生链接逃逸"))?;
    if !resolved.starts_with(&root) || resolved == root {
        return Err(install_error("Hook 文件不存在或发生链接逃逸"));
    }
    Ok(resolved)
}

pub(super) fn parse_codex_hook_bundle(raw: &str) -> Result<ParsedCodexHookBundle, String> {
    let root = serde_json::from_str::<Value>(raw)
        .map_err(|_| install_error("Codex Hook 文件不是有效 JSON"))?;
    let event_map = root
        .as_object()
        .and_then(|root| root.get("hooks"))
        .and_then(Value::as_object)
        .filter(|hooks| !hooks.is_empty())
        .ok_or_else(|| install_error("Codex Hook 文件必须包含非空 hooks 对象"))?;
    let mut events = event_map.keys().cloned().collect::<Vec<_>>();
    events.sort();
    let mut hooks = Vec::new();
    let mut normalized = Vec::new();
    for event in &events {
        if !SUPPORTED_CODEX_HOOK_EVENTS.contains(&event.as_str()) {
            return Err(if event == "PreCompact" {
                install_error("当前 runtime 无法准确触发 PreCompact；请使用 PostCompact")
            } else {
                install_error(&format!("不支持的 Codex Hook 事件：{event}"))
            });
        }
        let groups = event_map[event]
            .as_array()
            .filter(|groups| !groups.is_empty())
            .ok_or_else(|| install_error(&format!("Codex Hook {event} 必须是非空数组")))?;
        for (group_index, group) in groups.iter().enumerate() {
            let group = group.as_object().ok_or_else(|| {
                install_error(&format!("Codex Hook {event}[{group_index}] 必须是对象"))
            })?;
            if let Some(matcher) = group.get("matcher") {
                let matcher = matcher.as_str().ok_or_else(|| {
                    install_error(&format!("Codex Hook {event} matcher 必须是字符串"))
                })?;
                let matcher = matcher.trim();
                if matcher.is_empty()
                    || matcher.len() > MAXIMUM_HOOK_MATCHER_BYTES
                    || matcher.contains('\0')
                    || Regex::new(matcher).is_err()
                {
                    return Err(install_error(&format!(
                        "Codex Hook {event} matcher 不是有效正则表达式"
                    )));
                }
            }
            let handlers = group
                .get("hooks")
                .and_then(Value::as_array)
                .filter(|handlers| !handlers.is_empty())
                .ok_or_else(|| {
                    install_error(&format!("Codex Hook {event}[{group_index}] 缺少 handlers"))
                })?;
            for (handler_index, handler) in handlers.iter().enumerate() {
                let handler = handler.as_object().ok_or_else(|| {
                    install_error(&format!("Codex Hook {event} handler 必须是对象"))
                })?;
                if hooks.len() >= MAXIMUM_HOOK_COUNT {
                    return Err(install_error("Codex Hook 数量超过 64 个"));
                }
                if handler.get("type").and_then(Value::as_str) != Some("command") {
                    return Err(install_error(
                        "当前只接受同步 command Hook；mcp_tool 尚未接入",
                    ));
                }
                if handler.get("async").and_then(Value::as_bool) == Some(true) {
                    return Err(install_error(
                        "当前版本尚未支持 async Codex Hook，不能静默改为同步执行",
                    ));
                }
                let command = handler
                    .get("command")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|command| {
                        !command.is_empty()
                            && command.len() <= MAXIMUM_HOOK_COMMAND_BYTES
                            && !command.contains('\0')
                    })
                    .ok_or_else(|| install_error("Codex Hook command 无效或超过限制"))?
                    .to_owned();
                let timeout_seconds = match handler.get("timeout") {
                    Some(timeout) => timeout
                        .as_f64()
                        .filter(|timeout| {
                            timeout.is_finite()
                                && *timeout >= MINIMUM_HOOK_TIMEOUT_SECONDS
                                && *timeout <= MAXIMUM_HOOK_TIMEOUT_SECONDS
                        })
                        .ok_or_else(|| install_error("Codex Hook timeout 必须在 0.1–30 秒之间"))?,
                    None => 5.0,
                };
                let additional_context_limit = match handler.get("additionalContextLimit") {
                    Some(limit) => limit
                        .as_f64()
                        .filter(|limit| {
                            limit.is_finite()
                                && *limit >= 0.0
                                && limit.fract() == 0.0
                                && *limit <= MAXIMUM_HOOK_ADDITIONAL_CONTEXT_BYTES as f64
                        })
                        .ok_or_else(|| install_error("additionalContextLimit 超过安全限制"))?
                        as u64,
                    None => 64 * 1_024,
                };
                let matcher = group
                    .get("matcher")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .map(str::to_owned);
                let hook_id = format!("{event}-{group_index}-{handler_index}");
                let timeout_ms = (timeout_seconds * 1_000.0).round() as u64;
                hooks.push(LocalPluginHookDefinition {
                    hook_id: hook_id.clone(),
                    event: event.clone(),
                    matcher: matcher.clone(),
                    command: command.clone(),
                    is_async: false,
                    timeout_ms,
                    additional_context_limit,
                });
                normalized.push(json!({
                    "id": hook_id,
                    "event": event,
                    "matcher": matcher.unwrap_or_default(),
                    "command": command,
                    "timeoutSeconds": timeout_seconds,
                    "additionalContextLimit": additional_context_limit,
                }));
            }
        }
    }
    let normalized = serde_json::to_vec(&normalized)
        .map_err(|_| install_error("Codex Hook 无法生成规范化摘要"))?;
    Ok(ParsedCodexHookBundle {
        hooks,
        events,
        content_sha256: sha256_text(raw),
        normalized_hook_sha256: format!("{:x}", Sha256::digest(normalized)),
    })
}
