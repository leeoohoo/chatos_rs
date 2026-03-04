# 远端连接 + SFTP 功能方案（独立菜单草案）

> 日期：2026-03-03  
> 目标：新增一个与 `TERMINALS` 同级的侧边栏菜单（例如 `REMOTE`），支持 SSH 命令终端、SFTP 双栏传输、跳板机、证书登录。

## 1. 需求拆解

1. 左侧新增与 `TERMINALS` 同级的独立菜单（示例名：`REMOTE`）。
2. 在该菜单内可新建“远端连接”。
3. 连接建立后，点击连接可进入命令终端（体验尽量与现有终端一致）。
4. 每个连接有 `SFTP` 按钮，进入双栏文件管理：左侧服务器目录，右侧本地目录，支持双向传输。
5. 新建连接时必须支持：
   - 跳板机（Bastion / Jump Host）
   - 证书/密钥登录（至少私钥，建议支持私钥+证书）

## 2. 总体设计

采用“连接配置 + 运行会话”两层模型：

- 连接配置（持久化）：保存主机、用户、认证方式、跳板机等。
- 运行会话（临时）：点击连接时创建/复用终端会话，继续走现有 WebSocket 终端通道。

这样后续扩展（测试连接、自动重连、收藏连接）会更清晰。

## 3. 与现有代码衔接点

### 前端（chat_app）

- 侧栏菜单与列表：`/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/SessionList.tsx`
- 终端主视图：`/Users/lilei/project/my_project/chatos_rs/chat_app/src/components/TerminalView.tsx`
- API 封装：`/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/api/client.ts`
- 终端状态管理：`/Users/lilei/project/my_project/chatos_rs/chat_app/src/lib/store/actions/terminals.ts`
- 类型定义：`/Users/lilei/project/my_project/chatos_rs/chat_app/src/types/index.ts`

### 后端（chat_app_server_rs）

- 终端 API：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/api/terminals.rs`
- 终端运行时：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/services/terminal_manager/mod.rs`
- 终端模型/仓储：
  - `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/models/terminal.rs`
  - `/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/repositories/terminals.rs`
- SQLite 初始化：`/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/db/sqlite.rs`

## 4. 数据模型方案

建议新增两张表（Mongo 对应新增 collection）：

### 4.1 `terminal_connections`（连接配置）

- `id`, `user_id`, `name`, `kind`（`ssh`）
- `host`, `port`, `username`, `default_remote_path`
- `auth_type`（`password` / `private_key` / `private_key_cert`）
- `host_key_policy`（`strict` / `accept_new`）
- `jump_enabled`
- `jump_host`, `jump_port`, `jump_username`, `jump_auth_type`
- `created_at`, `updated_at`, `last_active_at`

### 4.2 `terminal_connection_secrets`（敏感信息）

- `connection_id`
- `encrypted_secret_json`（加密存储密码/私钥/证书/passphrase）
- `updated_at`

说明：接口返回连接数据时，不返回明文敏感信息，只返回“是否已配置”标志。

## 5. 连接策略

### 5.1 SSH 终端

- 保留现有 WebSocket + xterm 模式。
- `TerminalManager` 增加模式：
  - `LocalShell`（现状）
  - `RemoteSsh`（新）
- 远端模式下，PTY 启动 `ssh` 子进程，前端交互保持一致。

### 5.2 跳板机

- 开启跳板机时，按连接配置生成 SSH 参数（或临时 ssh config）实现 `ProxyJump`。
- 主机与跳板机支持独立认证配置。

### 5.3 证书/密钥

- MVP 先支持私钥登录。
- 增强支持私钥+证书（OpenSSH cert）与私钥口令。

## 6. SFTP 双栏能力

新增 SFTP API（按连接 ID）：

- `GET /api/terminal-connections/:id/sftp/list?side=remote|local&path=...`
- `POST /api/terminal-connections/:id/sftp/upload`（local -> remote）
- `POST /api/terminal-connections/:id/sftp/download`（remote -> local）
- `POST /api/terminal-connections/:id/sftp/mkdir`
- `POST /api/terminal-connections/:id/sftp/delete`
- `POST /api/terminal-connections/:id/sftp/rename`

前端新增 `SftpPanel`：

- 双栏目录浏览（面包屑/返回上级/刷新）
- 单/多文件选择
- 中间上传/下载按钮
- 底部传输任务队列（进度/结果/重试）

本地侧可复用现有 `/api/fs/*`，远端侧走新 SFTP API。

## 7. 安全与稳定性

1. 密钥加密存储：使用服务端主密钥（环境变量）加密。  
2. 主机指纹策略：默认 `strict`，可选 `accept_new`。  
3. 前端永不持有明文私钥/密码。  
4. 传输限流：控制任务并发与单任务大小。  
5. 审计日志：记录连接与传输操作（不记录敏感明文）。

## 8. 交互草图（独立 REMOTE 菜单）

- 新增菜单分区：`REMOTE`（与 `TERMINALS` 同级）
- `+` 新建连接弹窗：
  - 基础：名称、主机、端口、用户名
  - 认证：密码 / 私钥 / 私钥+证书
  - 跳板机：开关 + 跳板机参数
  - 高级：默认远程目录、主机校验策略
  - 操作：`测试连接`、`保存`
- 连接列表项：
  - 点击 -> 打开 SSH 终端
  - `SFTP` -> 打开双栏 SFTP
  - 更多 -> 编辑 / 删除 / 重连

## 9. 分阶段实施

### Phase 1（MVP）

- 连接配置 CRUD
- SSH 远程终端（跳板机 + 私钥）
- SFTP 双栏基础（目录浏览 + 单文件上传/下载）

### Phase 2（增强）

- 私钥+证书、私钥口令
- 批量传输、覆盖冲突策略
- 传输任务队列与进度

### Phase 3（体验优化）

- 自动重连
- 最近目录记忆
- 收藏路径/快速传输

## 10. 验收标准（DoD）

1. 可在独立 `REMOTE` 菜单中创建 SSH 连接并保存。  
2. 点击连接可进入远端终端并执行命令。  
3. 跳板机场景可成功登录并执行命令。  
4. 私钥/证书登录可成功建立连接。  
5. `SFTP` 双栏可浏览本地+远端目录并双向传输。  
6. 敏感信息不明文落库、不在接口返回。

## 11. 风险与注意点

- 若后端在容器里，需确认 SSH 网络可达、密钥临时文件路径与权限。  
- 不同服务器 SFTP 权限差异较大，MVP 需做多系统回归。  
- 大文件建议流式/分块，避免内存峰值。

---

如果你确认这版方向，我下一步可给你“实现级方案”：
- 接口请求/响应字段
- 数据库 DDL
- 页面结构与状态流转
