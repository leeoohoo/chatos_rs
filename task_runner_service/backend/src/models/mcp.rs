use chatos_ai_runtime::{TaskBuiltinMcpPromptMode, TaskMcpInitMode};
use chatos_mcp_runtime::{configurable_builtin_kinds, BuiltinMcpKind, BuiltinMcpPromptBuildResult};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpUnavailableTool {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCatalogEntry {
    pub kind: String,
    pub server_name: String,
    pub config_id: Option<String>,
    pub command: Option<String>,
    pub description: String,
    pub use_cases: Vec<String>,
    pub capabilities: Vec<String>,
    pub implemented: bool,
    pub runtime_default: bool,
    pub default_allow_writes: bool,
    pub available_tool_names: Vec<String>,
    pub unavailable_tools: Vec<McpUnavailableTool>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy)]
pub struct McpBuiltinKindGuide {
    pub description: &'static str,
    pub use_cases: &'static [&'static str],
    pub capabilities: &'static [&'static str],
}

pub fn mcp_builtin_kind_guide(kind: BuiltinMcpKind) -> McpBuiltinKindGuide {
    match kind {
        BuiltinMcpKind::CodeMaintainerRead => McpBuiltinKindGuide {
            description: "只读代码仓库工具，适合理解项目结构、查找实现和做代码审查，不会修改文件。",
            use_cases: &["理解现有代码", "查找实现位置", "审查代码或定位问题"],
            capabilities: &["读取文件", "搜索代码", "查看目录结构", "汇总代码片段"],
        },
        BuiltinMcpKind::CodeMaintainerWrite => McpBuiltinKindGuide {
            description: "代码维护写入工具，适合需要修改仓库文件、生成补丁或完成工程变更的任务；选择它时必须同时选择 CodeMaintainerRead。",
            use_cases: &["修改代码", "修复缺陷", "更新配置或文档", "生成可落地补丁"],
            capabilities: &["编辑文件", "应用补丁"],
        },
        BuiltinMcpKind::TerminalController => McpBuiltinKindGuide {
            description: "本地终端工具，适合需要运行命令、编译检查、执行脚本或查看环境状态的任务。",
            use_cases: &["运行编译检查", "执行脚本", "查看命令输出", "排查本地环境"],
            capabilities: &["执行 shell 命令", "读取命令输出", "管理长运行命令会话"],
        },
        BuiltinMcpKind::TaskManager => McpBuiltinKindGuide {
            description: "任务管理工具，适合在运行过程中拆分、跟踪和维护子任务。",
            use_cases: &["拆分复杂任务", "跟踪待办", "记录任务进度"],
            capabilities: &["创建子任务", "更新任务状态", "查询任务列表"],
        },
        BuiltinMcpKind::Notepad => McpBuiltinKindGuide {
            description: "临时笔记工具，适合在长任务中记录计划、观察结果、中间结论和待确认事项。",
            use_cases: &["保存中间结论", "记录计划", "整理上下文", "跨步骤保留笔记"],
            capabilities: &["写入笔记", "读取笔记", "更新笔记内容"],
        },
        BuiltinMcpKind::AgentBuilder => McpBuiltinKindGuide {
            description: "Agent 构建工具，适合维护 agent 配置、能力描述和相关构建材料。",
            use_cases: &["创建 agent 配置", "维护 agent 能力", "调整 agent 构建材料"],
            capabilities: &["读取 agent 配置", "生成配置草案", "更新 agent 相关文件"],
        },
        BuiltinMcpKind::UiPrompter => McpBuiltinKindGuide {
            description: "人工确认工具，适合任务执行时需要向用户请求选择、输入或确认的场景。",
            use_cases: &["请求用户确认", "让用户选择方案", "补充缺失参数"],
            capabilities: &["发起 UI 提问", "等待用户提交", "读取用户选择"],
        },
        BuiltinMcpKind::RemoteConnectionController => McpBuiltinKindGuide {
            description: "远程服务器控制工具，适合需要连接 Task Runner 服务器清单中的远程机器并执行命令或读写文件的任务。",
            use_cases: &["操作远程服务器", "读取远程日志", "执行远程命令", "排查部署环境"],
            capabilities: &["列出远程连接", "执行远程命令", "读写远程文件", "查看远程状态"],
        },
        BuiltinMcpKind::WebTools => McpBuiltinKindGuide {
            description: "网页检索和内容提取工具，适合需要查找外部资料、阅读网页或获取最新公开信息的任务。",
            use_cases: &["搜索资料", "读取网页内容", "核对外部信息", "整理来源摘要"],
            capabilities: &["网页搜索", "提取网页正文", "汇总搜索结果"],
        },
        BuiltinMcpKind::BrowserTools => McpBuiltinKindGuide {
            description: "浏览器自动化和观察工具，适合需要打开页面、检查 UI 状态、操作网页或读取浏览器控制台的任务。",
            use_cases: &["检查页面显示", "操作网页", "观察浏览器状态", "调试前端交互"],
            capabilities: &["打开页面", "点击输入", "截图观察", "读取控制台信息"],
        },
        BuiltinMcpKind::MemorySkillReader => McpBuiltinKindGuide {
            description: "记忆中的 skill 读取工具，适合查找当前上下文可复用的技能说明。",
            use_cases: &["读取技能记忆", "查找可复用工作流"],
            capabilities: &["检索 skill 记录", "读取 skill 内容"],
        },
        BuiltinMcpKind::MemoryCommandReader => McpBuiltinKindGuide {
            description: "记忆中的命令读取工具，适合查找历史命令、脚本片段和可复用命令经验。",
            use_cases: &["查找历史命令", "复用命令经验"],
            capabilities: &["检索命令记录", "读取命令说明"],
        },
        BuiltinMcpKind::MemoryPluginReader => McpBuiltinKindGuide {
            description: "记忆中的插件读取工具，适合查找插件能力、配置方式和使用说明。",
            use_cases: &["查找插件说明", "了解插件能力"],
            capabilities: &["检索插件记录", "读取插件说明"],
        },
    }
}

pub fn mcp_builtin_kind_values() -> Vec<String> {
    configurable_builtin_kinds()
        .into_iter()
        .map(|kind| kind.kind_name().to_string())
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub server_name: String,
    pub transports: Vec<String>,
    #[serde(default)]
    pub http_endpoint_path: Option<String>,
    #[serde(default)]
    pub stdio_command: Option<String>,
    #[serde(default)]
    pub stdio_args: Vec<String>,
    pub tool_names: Vec<String>,
    #[serde(default)]
    pub tool_profiles: Vec<McpServerToolProfileInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerToolProfileInfo {
    pub key: String,
    pub label: String,
    pub description: String,
    pub tool_names: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct McpPromptPreviewRequest {
    pub enabled: Option<bool>,
    pub init_mode: Option<TaskMcpInitMode>,
    pub builtin_prompt_mode: Option<TaskBuiltinMcpPromptMode>,
    pub builtin_prompt_locale: Option<String>,
    pub enabled_builtin_kinds: Option<Vec<String>>,
    pub workspace_dir: Option<String>,
    pub default_remote_server_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpPromptPreviewResponse {
    pub enabled: bool,
    pub init_mode: TaskMcpInitMode,
    pub builtin_prompt_mode: TaskBuiltinMcpPromptMode,
    pub builtin_prompt_locale: String,
    pub selected_builtin_kinds: Vec<String>,
    pub build: BuiltinMcpPromptBuildResult,
}
