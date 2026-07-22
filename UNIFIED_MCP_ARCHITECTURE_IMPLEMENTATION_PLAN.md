# 统一 MCP 架构与实施计划

## 1. 目标

将 ChatOS 仓库内的系统 MCP 收敛为一套与 `agent/` 对等的统一维护模型：

- 所有系统 MCP 只有一个权威目录（Catalog）。
- MCP 的身份、展示信息、工具 Schema、Provider Skills、安全属性、宿主支持范围和执行后端在同一个 Descriptor 中声明。
- 不再用 `builtin` 与旧路由分类表达两个互斥的系统 MCP 身份；统一使用 `system` 运行类型。
- 旧路由分类已从数据模型、Plugin Management、Task Runner 和客户端中删除。
- Rust 不使用类继承，改用稳定的 Trait + Adapter/Provider 组合实现必要抽象。
- 业务数据、鉴权、终端、浏览器和 UI 回调继续由所属宿主实现，但必须通过统一 Provider/Backend Contract 接入。
- Plugin Management 只作为 MCP 控制面，不再手写系统 MCP 清单、Schema 映射或路由分支。

## 2. 目标架构

```text
mcp/                                      # 系统 MCP 权威 crate
├── provider_skills/                      # Provider Skills 权威来源
└── src/
    ├── catalog.rs                        # SystemMcpDescriptor 权威目录
    ├── backend.rs                        # 执行后端与宿主声明
    ├── definition.rs                     # SystemMcpDefinition trait
    ├── provider.rs                       # Provider/Adapter 抽象
    ├── skills.rs                         # Provider Skills 加载
    ├── tool_catalog.rs                   # 所有系统 MCP Schema 工厂
    ├── contracts/
    │   └── project_management/           # 跨宿主项目管理参数、工具名与 Schema 合同
    └── implementations/
        ├── mod.rs                        # 19 个系统 MCP 定义对象
        ├── sandbox_images.rs             # Sandbox Images MCP 实现
        └── builtin/                      # 共享内嵌 MCP 的 Service + Store traits
            ├── code_maintainer/
            ├── terminal_controller.rs
            ├── task_manager.rs
            ├── browser_tools/
            ├── web_tools/
            ├── ask_user/
            └── ...

共享底层 crate
├── chatos_mcp_runtime                    # HTTP/stdio/in-process 执行器和协议
└── chatos_mcp_service                    # MCP JSON-RPC Server/Provider 基础设施

宿主服务
├── ChatOS                                # ChatOS Store/Browser/UI adapters
├── Task Runner                           # Task/Terminal/Project service adapters
├── Local Connector                       # 本地数据库、终端和浏览器 adapters
└── Project Management Service            # 项目与运行环境 owner adapters
```

## 3. 核心抽象

### 3.1 `SystemMcpKey`

为所有系统 MCP 提供稳定、可序列化的统一身份。既包括当前 `BuiltinMcpKind`，也包括原先由宿主路由的系统 MCP：

- `CodeMaintainerRead`
- `CodeMaintainerWrite`
- `TerminalController`
- `TaskManager`
- `ProjectManagement`
- `Notepad`
- `AgentBuilder`
- `AskUser`
- `RemoteConnectionController`
- `WebTools`
- `BrowserTools`
- `MemorySkillReader`
- `MemoryCommandReader`
- `MemoryPluginReader`
- `SandboxImages`
- `ProjectEnvironment`
- `ProjectRuntimeEnvironment`
- `LocalCommandApproval`
- `TaskRunnerService`

### 3.2 `SystemMcpDescriptor`

Descriptor 至少统一维护：

- `key`
- `resource_id`
- `server_name`
- `display_name`
- `description`
- `allow_writes`
- `tags`
- `category`
- `backend`
- `owner_service`
- `supported_hosts`
- `provider_skill_source`
- `tool_catalog_source`
- 与旧 `BuiltinMcpKind` 的可选映射

### 3.3 `SystemMcpBackend`

删除旧路由身份分类后，执行差异由 Descriptor 的后端描述，不暴露成资源类型：

- `Embedded`：当前进程内执行共享 Service。
- `ServiceHttp`：由所属服务提供 HTTP JSON-RPC MCP 端点。
- `ServiceDynamic`：工具清单或端点需要向所属服务动态发现。
- `HostAdapter`：由 Local Connector、ChatOS 等宿主提供本地 Provider。

`backend` 是系统内部执行信息，不是用户可选择的 MCP 类型。

### 3.4 Provider/Adapter Traits

- 继续复用 `BuiltinToolProvider` / `McpToolProvider`。
- 新增统一的 `SystemMcpProviderFactory`/`SystemMcpBackendResolver` 合约。
- 工具业务逻辑使用 `Service + Store trait`。
- 宿主仅实现 Store、鉴权、端点解析和回调，不复制 Schema 和身份元数据。

## 4. 数据模型迁移

### 4.1 新模型

系统 MCP 的 Plugin Management Runtime 统一为：

```json
{
  "kind": "system",
  "system_key": "project_runtime_environment",
  "server_name": "project_runtime_environment"
}
```

### 4.2 兼容读取

迁移期间 SDK 保留：

- `builtin_kind`
- 对历史 `runtime.kind = builtin`

统一解析函数仅继续把历史内嵌记录映射为 `SystemMcpKey`。旧路由记录不再兼容，部署前必须完成数据迁移。写接口不再允许创建旧类型。

### 4.3 客户端

- `RuntimeKind` 增加 `system`，删除旧路由类型。
- 系统 MCP 统一显示“系统 MCP / System MCP”。
- 第二行显示 Descriptor 的系统 Key 或 server name，不再显示路由类型。
- 系统 MCP 继续只允许启停和受控配置，不允许修改身份与执行后端。

## 5. 需要修改的区域

### 5.1 新增统一 `mcp/` crate

- 加入 Cargo workspace。
- 建立 Key、Descriptor、Catalog、Backend、Definition、Provider Skills 和 Tool Catalog 抽象。
- 为全部现有系统 MCP 建立 Descriptor。
- 提供旧 `BuiltinMcpKind`、resource ID、server name、kind name 到 `SystemMcpKey` 的统一解析。

### 5.2 `chatos_mcp_runtime`

- 保留底层执行器与协议职责。
- `BuiltinMcpKind` 暂时保留为嵌入式执行兼容类型。
- 将原 `system_tool_catalog` 的 Schema 由统一 MCP Catalog 对外提供；底层函数可暂时作为实现来源。
- 后续移除重复的系统 MCP 身份常量。

### 5.3 内嵌 MCP 实现归一

- 将共享 Service + Store trait 全部迁入 `mcp/src/implementations/builtin/`。
- `chatos_mcp` 直接拥有 Tool Schema、Provider 和执行实现，不再反向依赖独立工具 crate。
- 所有宿主直接依赖 `chatos_mcp`；删除旧兼容 crate 与 workspace 成员。

### 5.4 Plugin Management Backend

- 删除内嵌种子与路由种子的双路径。
- 改为遍历 `system_mcp_catalog()` 统一种子写入。
- 删除旧路由专用 Tool Catalog。
- `live_mcp_descriptor()` 对系统 MCP 直接调用 Descriptor。
- 统一 Provider Skills 来源。
- 增加旧 Mongo 记录的幂等迁移。
- API 校验只接受新的 `system` 类型；历史旧类型只读兼容。

### 5.5 Plugin Management Frontend

- 删除旧路由 RuntimeKind 和国际化文本。
- 增加统一 `system` 标签。
- 系统资源判断改为 `source_kind === system_seed || runtime.kind === system`。
- 工具清单与 Provider Skills UI 不区分内置/路由。

### 5.6 Task Runner

- Capability Policy 通过 `SystemMcpKey` 判断系统 MCP，不再用 `plugin_builtin_kind().is_none()` 推断外部 MCP。
- 内嵌系统 MCP 进入 Provider Registry。
- `ProjectRuntimeEnvironment` 等服务型系统 MCP 由统一 Backend Resolver 生成 HTTP Server。
- 删除旧路由执行分支和资源 ID 特判。
- Project Management Provider 继续使用统一契约并通过宿主 Adapter 调服务。

### 5.7 Project Management Service

- Project Management、Project Environment、Project Runtime Environment 和 Sandbox Images 均注册为统一 System MCP definitions。
- JSON-RPC 端点使用 `chatos_mcp_service::McpJsonRpcService` 或统一兼容封装，减少自建协议代码。
- Schema 从统一 Catalog 获取。
- 业务 Store、用户鉴权和项目范围校验保留在本服务。

### 5.8 Local Connector

- 使用 `SystemMcpKey` 选择和实例化系统 MCP。
- Local Project Management、Task Manager、Ask User 等作为 Host Adapter 注册。
- 删除本地重复的系统 MCP 支持清单，改读 Descriptor 的 host support。

### 5.9 ChatOS

- 使用统一 Catalog 生成内置配置与展示名称。
- Provider Factory 只负责 ChatOS Store/Callback Adapter。
- 不再维护独立的系统 MCP 身份映射。

### 5.10 SDK 与其他客户端

- `McpRuntime` 增加 `system_key`。
- 提供 `resolved_system_mcp_key()` 兼容解析。
- 更新 Task Runner、ChatOS、Local Connector 的 DTO 消费逻辑。

## 6. 分阶段实施顺序

### Phase 1：统一身份和目录

- [x] 新建 `mcp/` crate。
- [x] 定义 `SystemMcpKey`、`SystemMcpDescriptor`、Backend/Host enums。
- [x] 建立全部系统 MCP Catalog。
- [x] 建立统一工具 Schema 与 Provider Skills 入口。
- [x] 加入完整性、唯一性和旧身份映射测试。

### Phase 2：控制面统一

- [x] SDK 增加 `system_key` 和 `system` runtime。
- [x] Plugin Management 使用统一 Catalog 种子。
- [x] 幂等迁移历史内嵌与路由记录。
- [x] 删除 Plugin Management 的双清单与 Schema match。
- [x] 更新前端显示和 RuntimeKind。

### Phase 3：Task Runner 执行统一

- [x] Capability Policy 改为 SystemMcpKey。
- [x] 建立 Task Runner Backend Resolver。
- [x] 迁移 Project Runtime Environment 服务端点解析。
- [x] 删除旧路由执行分支。

### Phase 4：宿主 Provider 统一

- [x] ChatOS Provider Factory 受统一 Descriptor 和 host support 约束。
- [x] Local Connector Provider Factory 迁移为 `SystemMcpHostAdapter`。
- [x] Project Management Service Provider/JSON-RPC 统一。
- [x] 收敛重复 host support 列表和 kind 映射。

### Phase 5：清理旧架构

- [x] 删除 `RUNTIME_KIND_SYSTEM_ROUTED`。
- [x] 删除旧前端类型和国际化文本。
- [x] 删除重复 resource/server 常量、Provider Skill 文件与 seed 函数。
- [x] 删除只为旧分类存在的测试和文档描述。
- [x] 评估 `BuiltinMcpKind`：保留为底层嵌入式执行兼容类型；系统身份与新增代码统一使用 `SystemMcpKey`，不再新增基于它的控制面分类。

### Phase 6：实现代码物理归一

- [x] 将原 `crates/chatos_builtin_tools/src` 全部迁入 `mcp/src/implementations/builtin/`。
- [x] 反转依赖方向，使统一 MCP crate 直接拥有实现。
- [x] ChatOS、Task Runner、Local Connector、Project Management 与 Sandbox MCP Server 改为直接依赖 `chatos_mcp`。
- [x] 更新 Sandbox Agent 独立 workspace 与 Docker 构建上下文。
- [x] 删除 `crates/chatos_builtin_tools` workspace 成员及兼容 crate。
- [x] 将 `chatos_project_mcp_contract` 迁入 `mcp/src/contracts/project_management/` 并删除旧 crate。
- [x] 将 `chatos_sandbox_image_mcp` 迁入 `mcp/src/implementations/sandbox_images.rs` 并删除旧 crate。
- [x] Project Management、Task Runner、Local Connector、Plugin Management 与 Sandbox Manager 全部改用统一 `chatos_mcp` API。

## 6.1 实施结果（2026-07-21）

- 19 个系统 MCP 已全部进入统一 Catalog，并具有 Definition 对象。
- Plugin Management 启动时会把历史系统记录幂等更新为 `runtime.kind = system` 和 `runtime.system_key`。
- 旧路由类型已从服务端、统一 Catalog、Task Runner 和前端中彻底删除，不再保留历史字符串或兼容读取分支。
- 系统工具 Schema 和 Provider Skills 已从 Plugin Management、Task Runner 与 `chatos_mcp_runtime` 迁入 `mcp/`。
- Project Management 与 Project Runtime Environment 已使用共享 `McpJsonRpcService`。
- Task Runner 服务型后端和 Local Connector 内嵌后端已通过 `SystemMcpHostAdapter` 接入。
- 原 `chatos_builtin_tools` 的 93 个实现源文件已迁入 `mcp/src/implementations/builtin/`，所有宿主已改为直接使用 `chatos_mcp`。
- 旧 `chatos_builtin_tools` crate 已删除，仓库内不再存在代码或构建配置依赖。
- Project Management 跨宿主合同与 Sandbox Images 执行实现已进入 `chatos_mcp`，两个旧 crate 与空目录均已删除。
- `cargo check --workspace` 通过；统一 `chatos_mcp` 105 个测试、Project Management 77 个非数据库测试和 Sandbox MCP Server 39 个测试全部通过（Project Management 另有 11 个 MongoDB 测试按设计忽略）；此前 Plugin Management 前端生产构建已通过。
- Local Connector 生产代码随 workspace check 通过；其全量 lib test 当前被既有 `save_agent_prompt_manifest` 测试调用缺少第三个 `bool` 参数阻塞，与本次 MCP 迁移无关。

## 7. 验收标准

- 新增系统 MCP 只需在 `mcp/src/implementations/` 增加定义/实现并加入 Catalog，不再同时修改 Plugin Management seed/tool_catalog。
- Catalog 中每个系统 MCP 的 key、resource ID 和 server name 全局唯一。
- 每个系统 MCP 都有非空工具目录，或明确声明动态发现机制。
- Plugin Management 数据中不再产生旧路由类型。
- Plugin Management 客户端不再展示“系统路由”。
- Task Runner 不再包含旧路由类型判断。
- 所有宿主通过 Descriptor 判断支持范围，通过 Factory/Adapter 实例化 Provider。
- 云端 Project Management、Project Runtime Environment 与本地 Project Management 行为保持一致的工具契约。
- Workspace Cargo check、相关 Rust tests 和 Plugin Management 前端测试/构建通过。

## 8. 实施约束

- 当前工作树包含大量既有改动，迁移必须保持增量、幂等，不覆盖无关文件。
- 每个 Phase 完成后先运行局部测试，再进入下一阶段。
- 旧数据库记录必须可自动迁移，不能要求人工清库。
- 不在 Plugin Management 中执行具体 MCP 业务逻辑。
- 不因统一目录而破坏服务鉴权、项目隔离、本地设备边界或沙箱边界。
