# Cloud / Local / Portable Plugin Runtime 架构

本文描述 ChatOS Plugin schema v2 的最终运行合同。该合同一次性替换“所有 Plugin 都必须绑定 Local Connector”的旧假设，不包含 feature flag、云端失败后回退本地、双 Prompt 注入或长期兼容旁路。

## 1. 执行位置

执行位置属于组件，不属于整个 Plugin：

| `execution_host` | 准备与执行位置 | 是否需要本地安装 | 首批允许组件 |
| --- | --- | --- | --- |
| `cloud` | Task Runner Cloud Runtime | 否 | Skill、Command、Agent |
| `local` | Local Connector Runtime | 是 | 现有本地组件 |
| `portable` | 随当前 Task Runner Agent plane 选择云端或本地 | 仅本地执行时需要 | Skill、Command、Agent |

Plugin 展示类型由 Release 中的组件执行位置集合推导：单一位置显示 Cloud、Local 或 Portable；同时存在多个位置显示 Hybrid。

schema v1 没有 `execution` 字段，读取后隐式规范化为 `local`，序列化和历史签名/hash 不增加该字段。schema v2 必须显式提供 `execution.defaultHost`，并可用 `componentHosts` 覆盖真实组件 key。

## 2. 安全边界

Cloud 与 Portable 组件必须同时满足：

- 只能是 Skill、Command 或 Agent；
- 不申请运行时权限，不绑定 OAuth、Connected App、MCP、Hook、UI 或 Native Adapter；
- 云端不执行 artifact 中的脚本、二进制或 stdio 进程；
- Skill 只展开 `skills/`、`references/`、`schemas/`、`licenses/` 下可达的受限文本资源；
- 所有第三方 Prompt 都放入安全 envelope，不能覆盖平台政策、系统/开发者指令、用户授权、审批要求和验收标准。

共享 `chatos_plugin_package` crate 是 ZIP、checksum、SBOM、路径、文本资源和 canonical Bundle hash 的唯一权威实现。Plugin Management 和 Local Connector 对同一 Release 必须生成相同 Bundle hash。

## 3. 发布与数据模型

Plugin Management 在摄取包含 Cloud/Portable 组件的 Release 时执行：

1. 下载并限制 artifact；
2. 校验 artifact SHA-256、Release 签名和发布者身份；
3. 校验 Manifest、精确 checksums、SBOM、ZIP 路径/类型/大小；
4. 构造每个 Cloud/Portable 组件的 canonical text Bundle；
5. 对照 component snapshot 的 `content_sha256`；
6. 保存不可变 Bundle、Release 和 snapshot；
7. 最后把 publication state 标为 ready。

MongoDB 新增：

- `plugin_cloud_component_bundles`：以 `plugin_id + release_id + component_key` 唯一索引保存不可变 Bundle；
- `plugin_release_publication_states`：记录新 Release 是否完成全部物化。

新 Release 在 `ready=false` 时不会被公开读取或执行。失败可能留下不可见的 staging/orphan 数据，后续相同 revision 可继续修复；已有 ready stable Release 不因新版本物化失败而被隐藏。历史 Release 没有 publication state 记录时按 ready 读取，以保持既有数据可用。

所有执行读取都必须走 readiness-gated `get_plugin_release`；`get_plugin_release_any_state` 只允许发布和 Catalog materialization 内部流程使用。Cloud-only Release 不创建 Local Connector installation record。

## 4. Run 路由与不可变快照

Task Runner 根据当前 Agent plane 路由 Portable 组件：

- Cloud Agent：Portable 走 Cloud Runtime；
- Local Agent：Portable 走 Local Connector；
- 路由不由“请求里是否碰巧带了 device_id”决定。

Cloud-only 或 Cloud-plane Portable Run 不创建 Relay。只有实际选择了 Local/Local-plane Portable 组件时才要求精确 `device_id`、`workspace_id`、active installation，并建立 Relay。Hybrid 中任一 prepare 失败会取消已准备 session 并整体失败，不会只保留云端部分。

Run snapshot 固定：

- plugin/release/version/artifact identity；
- component kind 与 `execution_host`；
- Cloud/Portable canonical `bundle_sha256`；
- 本地 installation/device/workspace（仅在实际需要时）；
- permission 与 auth snapshot。

Task Runner 每次使用 Bundle 时重算正文、资源、size 和 canonical hash；身份、正文或 hash 漂移均失败关闭。缓存是有界 LRU（最多 256 个 Bundle、64 MiB），缓存命中不能绕过 snapshot、Release identity 或撤销检查。

Prompt 顺序固定为 Skill → Command → Agent，同类再按完整文本身份稳定排序。Run 日志和审计只记录 identity、host、size 和 hash，不记录第三方 Prompt 正文。

## 5. Local Connector

Local 组件保持既有签名安装与运行合同。Portable 组件在本地执行时必须：

1. 已安装与 Run snapshot 相同的签名 Release；
2. 从已安装目录重新执行共享 package 校验；
3. 重建 canonical Bundle；
4. 使用 `bundle_sha256` 对照 Run snapshot，而不是使用单个源文件 raw hash。

Cloud-only Plugin 在本地 Catalog 中显示 `cloud_ready`，不提供安装按钮；installer 和 Plugin Management installation sync 都会拒绝为它创建本地安装。

## 6. ChatOS 选择合同

ChatOS 后端只要用户选择了 Plugin 就发送 selected-plugin header，device header 为可选。前端在没有 Desktop Local Runtime Bridge 时仍可发现和选择 Cloud Plugin，不再自动绑定第一台设备。

只有用户显式选择设备时才加载本地组件。切换或清除设备会移除依赖该设备的选择，但保留 Cloud 选择。会话和 plan/run 模式分别持久化 device/workspace、Plugin、Command 和 Agent 选择。

界面必须显示执行类型、组件 host/readiness/reason、prepare provider、content SHA-256，以及是否要求 workspace/device；不得展示 Bundle Prompt 正文。

## 7. 部署与回滚边界

同一变更集按以下顺序发布：

1. Plugin Management Mongo indexes/collections；
2. Plugin Management schema v2、Bundle ingestion 和 readiness gate；
3. 使用同一 SDK 合同的 Task Runner、ChatOS 后端与 Local Connector Service；
4. ChatOS 前端与桌面 Local Connector Client；
5. 外部生产 signer 签署并发布 schema v2 Release；
6. 执行 Cloud、Local 和 Hybrid E2E。

全部运行服务升级前不得发布 schema v2 Release。回滚服务版本时也必须先停止发布新的 schema v2 Release；已经创建的 Run 继续依赖其不可变 snapshot，不能通过修改 Bundle 或 installation 静默迁移。

## 8. 最小 E2E

- 无 Local Connector 的纯云端会话选择 Ponytail 并完成任务；
- Cloud Run 的 Plugin snapshot 没有 device，Portable 走 Cloud Runtime；
- 本地项目安装同一 Ponytail Release，Portable 走 Local Connector；
- Hybrid 同时选择 Cloud Prompt 与 Local MCP，绑定设备后成功；
- Hybrid 缺设备、离线、Release 错配、Bundle hash 错配分别失败关闭；
- Release 升级后旧 Run 保持旧 Bundle，新 Run 使用新 Bundle；
- Release 撤销后拒绝新 Run，历史 Run 和审计仍可读。
