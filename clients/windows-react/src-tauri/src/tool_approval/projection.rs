use chatos_local_agent_protocol::{
    LocalAgentToolApprovalStatus, LocalAgentToolExecutionRoute, LocalAgentToolInvocationRecord,
    LocalAgentToolStatus,
};
use serde_json::{Map, Value};

use super::models::{ToolApproval, ToolApprovalDecision};

const MAX_COMMAND_CHARACTERS: usize = 4_000;
const MAX_PURPOSE_CHARACTERS: usize = 500;
const MAX_PATH_CHARACTERS: usize = 1_000;

pub fn pending_approval(invocation: &LocalAgentToolInvocationRecord) -> Option<ToolApproval> {
    if !invocation.requires_approval
        || invocation.approval_status != LocalAgentToolApprovalStatus::Pending
        || invocation.status != LocalAgentToolStatus::Pending
        || invocation.version == 0
    {
        return None;
    }

    let arguments = invocation.arguments.as_object();
    let presentation = presentation(invocation.tool_name.as_str(), arguments);
    Some(ToolApproval {
        invocation_id: invocation.invocation_id.clone(),
        invocation_version: invocation.version,
        run_id: invocation.run_id.clone(),
        tool_name: invocation.tool_name.clone(),
        command: presentation.command,
        purpose: presentation.purpose,
        cwd: presentation.cwd,
        source: match invocation.execution_route {
            LocalAgentToolExecutionRoute::Platform => "Local Agent Task".into(),
            LocalAgentToolExecutionRoute::Shared => "Local Agent Shared Runtime".into(),
        },
        risk: presentation.risk,
        reason: presentation.reason,
        created_at_unix_ms: invocation.created_at_unix_ms,
        updated_at_unix_ms: invocation.updated_at_unix_ms,
        available_decisions: available_decisions(&invocation.tool_name),
    })
}

fn available_decisions(tool_name: &str) -> Vec<ToolApprovalDecision> {
    let mut decisions = vec![ToolApprovalDecision::Decline, ToolApprovalDecision::Accept];
    if matches!(
        tool_name,
        "execute_command" | "process_write" | "process_kill"
    ) {
        decisions.insert(1, ToolApprovalDecision::AcceptForSession);
    }
    decisions
}

struct ApprovalPresentation {
    command: String,
    purpose: Option<String>,
    cwd: String,
    risk: String,
    reason: Option<String>,
}

fn presentation(tool_name: &str, arguments: Option<&Map<String, Value>>) -> ApprovalPresentation {
    match tool_name {
        "execute_command" => {
            let command = first_string(arguments, &["command", "common"])
                .map(|value| bounded(value, MAX_COMMAND_CHARACTERS))
                .unwrap_or_else(|| "本地终端命令（内容不可用）".into());
            let purpose = first_string(arguments, &["purpose"])
                .filter(|value| value.chars().count() <= MAX_PURPOSE_CHARACTERS);
            let cwd = first_string(arguments, &["path"])
                .map(|value| bounded(value, MAX_PATH_CHARACTERS))
                .unwrap_or_else(|| ".".into());
            let (risk, reason) = command_risk(&command);
            ApprovalPresentation {
                command,
                purpose,
                cwd,
                risk,
                reason,
            }
        }
        "process_write" => ApprovalPresentation {
            command: "process_write · interactive input hidden".into(),
            purpose: Some("向当前任务启动的本地命令写入输入".into()),
            cwd: ".".into(),
            risk: "medium".into(),
            reason: Some("本地任务请求向运行中的命令写入输入。".into()),
        },
        "process_kill" => ApprovalPresentation {
            command: "process_kill · task-owned process".into(),
            purpose: Some("终止当前任务启动的本地命令".into()),
            cwd: ".".into(),
            risk: "medium".into(),
            reason: Some("本地任务请求终止运行中的命令。".into()),
        },
        "capability_invoke" => ApprovalPresentation {
            command: "capability_invoke · selected Plugin tool".into(),
            purpose: Some("调用当前 Todo 可信计划中已查看定义的本机 Plugin 工具".into()),
            cwd: ".".into(),
            risk: "high".into(),
            reason: Some("Plugin 工具可能读写项目或产生外部副作用，本次调用需要人工确认。".into()),
        },
        "remote_connection_controller_run_command" => ApprovalPresentation {
            command: first_string(arguments, &["command"])
                .map(|value| format!("remote SSH command · {}", bounded(value, 500)))
                .unwrap_or_else(|| "remote SSH command".into()),
            purpose: Some("在选定的远程连接上执行命令".into()),
            cwd: "远程连接".into(),
            risk: "high".into(),
            reason: Some("本地任务请求在已保存的远程连接上执行命令。".into()),
        },
        "remote_connection_controller_upload_file" => ApprovalPresentation {
            command: first_string(arguments, &["path"])
                .map(|value| format!("remote file upload · {}", bounded(value, 500)))
                .unwrap_or_else(|| "remote file upload".into()),
            purpose: Some("向选定的远程连接上传文件".into()),
            cwd: "远程连接".into(),
            risk: "high".into(),
            reason: Some("本地任务请求向已保存的远程连接写入文件。".into()),
        },
        _ => ApprovalPresentation {
            command: format!("{} · commit staged project edits", bounded(tool_name, 256)),
            purpose: Some("修改当前本地项目中的已暂存文件".into()),
            cwd: ".".into(),
            risk: "medium".into(),
            reason: Some("本地任务请求提交已暂存的项目文件修改。".into()),
        },
    }
}

fn first_string(arguments: Option<&Map<String, Value>>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        arguments?
            .get(*key)?
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn bounded(value: impl AsRef<str>, maximum_characters: usize) -> String {
    let value = value.as_ref();
    if value.chars().count() <= maximum_characters {
        return value.to_owned();
    }
    let mut result = value
        .chars()
        .take(maximum_characters.saturating_sub(1))
        .collect::<String>();
    result.push('…');
    result
}

fn command_risk(command: &str) -> (String, Option<String>) {
    let normalized = command.to_lowercase();
    let high_risk_markers = [
        "rm -rf",
        "sudo ",
        "mkfs",
        "diskutil erase",
        "dd if=",
        "shutdown",
        "reboot",
        "kill -9",
        "git reset --hard",
        "git clean -fd",
        "curl | sh",
        "curl | bash",
        "chmod -r 777",
        "> /dev/",
    ];
    if let Some(marker) = high_risk_markers
        .iter()
        .find(|marker| normalized.contains(**marker))
    {
        return ("high".into(), Some(format!("命令包含高风险操作：{marker}")));
    }
    let padded = format!(" {normalized} ");
    let write_markers = [
        " rm ",
        " mv ",
        " cp ",
        "install ",
        "npm install",
        "cargo install",
        "git commit",
        "git push",
        "chmod ",
        "chown ",
        "mkdir ",
        "touch ",
    ];
    if write_markers.iter().any(|marker| padded.contains(marker)) {
        return (
            "medium".into(),
            Some("命令可能修改文件、依赖或远程状态。".into()),
        );
    }
    ("low".into(), None)
}
