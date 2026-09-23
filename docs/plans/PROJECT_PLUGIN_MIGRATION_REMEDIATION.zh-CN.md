# 项目管理插件与 Task Runner Bridge 删除清单

状态：代码删除与跨平台静态验证完成，待客户端真机验收
更新日期：2026-09-10

本清单记录最终删除范围。此前“完成项目管理插件”的方案已经取消，验收目标改为彻底移除插件及其专属基础设施。

## 已确认的问题

- 删除原生 Plan 后曾错误地把项目管理插件放入项目页面，造成产品入口重复。
- 项目管理插件的界面与功能未达到可交付产品标准。
- 插件退出后，专门为它增加的 Task Runner Bridge、任务工作区和默认模型字段失去消费者。
- Windows 与 macOS、共享 SDK、部署脚本、官网和文档都可能留下单边残留，不能只删除一个插件目录。
- 项目会话虽然已经保存客户端 `project_context`，但 ChatOS 的 Task Plugin 目录请求以及 MCP Management 到 Task Runner 的内部请求曾只透传 `project_id`，导致项目内消息创建 Task 时被 Task Runner 拒绝。

## 删除项

- [x] 删除 `plugins/project-management` 全部受跟踪源码和本机构建产物。
- [x] 删除插件构建、测试、打包、发布、镜像和官网展示入口。
- [x] 删除 macOS Task Runner Bridge、插件任务工作区及默认模型设置。
- [x] 删除 Windows Task Runner Bridge、插件任务工作区及默认模型设置。
- [x] 删除共享 SDK 中三个专属 Bridge capability 与反序列化枚举。
- [x] 删除 User Service / ChatOS 的 `task_runner_default_model_config_id` 字段、校验和透传。
- [x] 删除两端旧项目清单导入协议、macOS 导入界面及本地导入回执存储。
- [x] 删除旧 Connector 工作区自动迁移和插件未声明上下文时的隐式 workspace 注入。
- [x] 将 Windows 插件 capability 白名单收缩为 `host.context.read`。
- [x] 更新项目绑定文档，明确客户端项目注册表是唯一权威。
- [x] 完成 Rust、macOS 与 Windows 非 WinUI 目标编译测试及发布脚本验证。
- [x] 补齐“项目会话 → ChatOS → MCP Management → Task Runner”的客户端项目快照透传；Task Runner 必须重新校验快照，禁止仅凭 `project_id` 执行项目任务。
- [ ] 完成 macOS 重装验收和 Windows 真机验收。
- [x] 提交删除变更。
- [ ] GitHub 网络恢复后推送变更。

## 2026-09-10 项目会话未创建新任务事故

### 现象

会话 `97005733-2e33-4f7c-8c6c-03ca23635a40` 的轮次
`turn_5070d8f6-6c6d-4028-b520-fe91ce5e033c` 中，用户要求基于当前项目用设计工具重新设计页面。主对话 Agent 只读取了旧任务
`cf6eea55-88a0-4cdb-874c-ec9132a9bab3`，没有为当前消息创建 Task，却回复成已经完成设计。

### 根因

这不是“项目消息没有被强制建任务”，也不能用“所有项目消息一律建任务”修复。事故由以下上下文链路同时断裂造成：

1. 客户端项目注册表已经提供 `projectId`、`projectName` 和执行目标，但删除 Project Service 后，ChatOS 仍尝试从已不存在的服务侧项目记录补项目名称，导致 Agent Run 的 `resolved_project_name` 为空，当前项目 Prompt 缺失。
2. Cloud Agent、实时事件和回执仍优先保留请求原始 `project_id`。续聊请求没有重复提交 `project_id` 时，已经从会话快照解析出的项目身份没有继续进入完整运行作用域。
3. ChatOS 获取 Task Plugin 目录，以及 MCP Management 调用 Task Runner 时，只传了 `project_id`，没有传客户端权威项目快照。Task Runner 无法重新验证项目与设备执行目标，也无法按当前项目解析可用插件。
4. 本地 Plugin Management 目录只有 Browser CDP、Computer Use、Document Tools 和 Diagram Studio，没有已经安装在客户端的 Web Design Studio。客户端保留的是另一环境生成的 Plugin/Release ID，服务端因找不到该 ID 拒绝安装上报，Task Runner 因此看不到设计插件。
5. Agent 在“修改之前结果”的语境中读取了旧 Task。Prompt 没有明确指出“历史读取只建立背景，不代表当前执行已经完成”，模型把旧 Task 的成功状态错误复用到当前请求。
6. `get_task` 把旧任务中包含数百条路径的 `result_summary` 整包返回，显著挤占当前轮模型上下文，却没有增加当前设计能力。
7. 对话结束后的任务生命周期复查仍读取 ChatOS 旧集合 `task_manager_tasks`，真实任务已经迁到 Task Runner；因此即使 Task Runner 有任务，ChatOS 也可能记录 `task_turn_review.attempted=false`。
8. Task Runner 的 `list_tasks` 工具描述仍声称存在 `Chatos Plan profile`，Plugin Management 数据库也保留了四个 `chatos_plan` Prompt。旧概念虽然不在客户端界面上，仍可能进入模型上下文或被内部 Prompt 解析接口选中。
9. ChatOS 会话运行设置和消息元数据仍接受 `auto_create_task`。这个字段已经没有生产决策用途，却继续制造“项目消息是否自动建任务”的错误心智模型。
10. ChatOS 主对话 Agent 仍被直接绑定到 Notepad，实际运行快照因此暴露了 12 个 Notepad 工具和 4 个 Task Runner 工具。主对话与执行层的边界没有真正收口。
11. 上游兼容 Responses 接口可能返回请求工具清单之外的供应商原生调用。事故轮次返回了已完成的 `image_generation_call` 和约 2.1 MB PNG base64，但 ChatOS 没有授权该工具，也没有把图片持久化为用户可见附件，随后却接受了模型的“图片已显示”声明。
12. 客户端重装后 ProjectRegistry 已把项目迁移到新设备和新 workspace，但已有会话仍保存旧设备快照。项目页复用已有会话时没有重新执行上下文对账，导致 Task Plugin 目录继续按离线旧设备解析并返回空目录。

### 修复原则

- 不恢复 Project Service；客户端 ProjectRegistry 继续作为项目唯一权威。
- 不为普通项目聊天强制创建任务。只有用户意图确实需要读取、修改、运行或验证真实项目时，主对话 Agent 才应使用 Task Runner。
- 项目是否绑定与是否创建 Task 是两件事：项目绑定只提供上下文和授权边界，Task 的创建只由当前请求是否需要真实执行决定。
- 历史 Task 只能作为当前意图的背景；当前消息需要执行时，必须创建或复用与当前消息绑定的 Task，不能复用旧 Task 的完成结论。
- Task Runner 对客户端项目快照重新校验；任何只有 `project_id`、没有匹配授权快照的项目执行都失败关闭。
- 主对话 Agent 只允许直接看到 Task Runner；Notepad、文件、终端和所有 Plugin MCP 都只能在 Task Runner 的受审计运行中按任务选择。
- 模型供应商返回的任何未授权原生工具调用都视为权限越界：丢弃调用结果和二进制载荷，不持久化、不作为交付证据，并引导模型改走 Task Runner；连续越界则终止当前轮。
- 每次激活项目会话都用客户端 ProjectRegistry 的最新项目快照对账，客户端重装、设备重配对和 workspace 迁移不能继续沿用旧执行目标。
- 生命周期只审查当前 turn 在 Task Runner 中的真实任务，不再用旧 ChatOS 任务集合冒充运行事实。
- 面向模型的历史结果采用有明确截断标记的轻量投影，完整记录仍由产品详情 API 保留。

### 已实施

- [x] ChatOS 从已验证的客户端项目快照补全 `resolved_project_name`。
- [x] Cloud Agent、实时事件和回执作用域优先使用运行时解析后的项目 ID。
- [x] ChatOS 的 Task Plugin 目录请求透传客户端项目快照。
- [x] MCP Management 运行会话持久化项目快照，并在每次 Task Runner 请求中透传。
- [x] Task Runner 校验项目 ID 与客户端项目快照一致，并把快照冻结进新 Task。
- [x] 主对话 Prompt 明确历史任务读取与当前任务执行的边界，禁止用旧结果冒充当前结果。
- [x] `get_task` 等 Agent 工具结果对超长摘要和描述做有标记的字符级截断。
- [x] 对话生命周期改为按当前 session + turn 查询 Task Runner 任务并继续/复查。
- [x] 删除已经失去生产消费者的旧 Task Board Prompt 实现。
- [x] 删除 ChatOS 会话设置和消息元数据中的 `auto_create_task`；旧字段不再被读取、返回或接受，服务启动时直接清除全部数据库残留。
- [x] 删除 Task Runner 面向模型的 `Chatos Plan profile` 描述。
- [x] Plugin Management 启动 seed 删除系统 Agent 不受支持的 Prompt profile 及其版本引用；内部解析接口拒绝选择已删除的 `chatos_plan`。
- [x] Web Design Studio 3.0.2 已发布到本地 Plugin Management；其 artifact SHA-256 与客户端现有安装一致，可在客户端重连时迁移为本地 Plugin/Release 身份。
- [x] 删除 ChatOS 主对话 Agent 的全部直接 MCP 绑定并只重建 Task Runner 白名单绑定；启动 seed 会清理数据库中的旧 Notepad 或人工覆盖残留。
- [x] 增加主对话供应商原生工具越权守卫；`image_generation_call`、`computer_call`、`mcp_call` 等非函数调用不会进入持久化上下文或最终成功消息，首次改走 Task Runner，重复越权失败关闭。
- [x] macOS 项目会话激活时强制用本地 ProjectRegistry 最新快照重新对账已有会话，刷新设备、workspace、相对根目录和项目 revision。
- [x] 重启 Plugin Management、Task Runner 和 ChatOS；确认服务健康，新版 `gpt/default` Prompt revision 为 5。
- [x] 验证数据清理：删除 4 条 `chatos_plan` Prompt、3 个版本引用和 45 个会话 `auto_create_task` 字段，三项残留计数均为 0。
- [ ] 重启或重新连接客户端 Local Connector，使插件安装身份迁移和安装状态上报生效。
- [ ] 用同一项目复测“读取上一轮任务后，使用 Web Design Studio 创建当前轮新任务”的完整链路。

### 回归验收

1. 项目续聊请求即使不重复传 `project_id`，运行快照仍包含客户端项目 ID、项目名称和执行目标。
2. Task Plugin 目录包含 `chatos-web-design-studio@chatos-marketplace`，且只在设备已安装、启用并符合项目执行目标时可选。
3. 用户要求重新设计时，Agent 可以先读历史 Task，但随后必须为当前消息创建或复用 Task；旧 Task ID 不能成为当前完成证据。
4. 新 Task 的插件选择审计包含 Web Design Studio 的 Plugin ID、Release ID、版本、artifact SHA-256、设备 ID 和选择原因。
5. 当前 Task 尚未完成时，对话不能直接结束；Task 成功后必须触发同轮复查。
6. 模型输入中不再出现无界的历史路径清单；发生截断时返回 `_truncated_fields`，完整详情仍可由产品详情接口读取。
7. 最终回复中的“已创建”“已执行”“已完成”分别能在当前轮工具调用、Task Runner Run 和验收结果中找到直接证据。
8. ChatOS 主对话运行快照的工具清单只包含 `task_runner_service_*`，不包含 Notepad、文件、终端或任何 Plugin MCP。
9. 即使上游供应商越权返回 `image_generation_call`，该 item 与 base64 结果也不会进入 Cloud Agent 的下一步输入、Memory Engine 最终消息或用户界面；模型必须改走当前轮 Task。
10. 客户端重装或 Local Connector device/workspace 变化后，已有项目会话在下一次激活时更新为 ProjectRegistry 的最新 `project_context`，Task Plugin 目录能看到该设备已安装的 Web Design Studio。

## 不删除项

- 通用 Task Runner 服务及普通会话的后台任务能力。
- 客户端“应用”侧栏和通用插件应用宿主。
- 其他插件使用的项目 scope、运行数据隔离和 `host.context.read`。
- 客户端项目注册表、Git、文件、终端与运行能力。

## 搜索门禁

生产代码必须不再出现：

```text
task.batch.prepare
task.batch.status
task.workspace.open
TaskRunnerHostService
PluginTaskWorkspace
task_runner_default_model_config_id
plugins/project-management
ProjectManagementPlugin
```

历史设计资料如保留，必须明确标注已废弃，不能被 README、构建或发布流程引用。
