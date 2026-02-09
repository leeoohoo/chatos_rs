# 终端模块方案（初稿）

> 目标：在左侧新增“终端列表”，可创建/选择终端；右侧渲染可交互的终端界面，支持历史输出/指令，跨平台（Linux/macOS/Windows）。

## 1. 现状与约束
- 前端：React（chat_app），已有项目列表/目录选择弹窗逻辑，可复用。
- 后端：Rust（chat_app_server_rs，Axum），已具备 REST/SSE 能力。
- 需求重点：
  - 终端列表（类似 Projects）
  - 新建终端时选择目录
  - 点击终端展示可交互终端（支持历史输出 + 指令）
  - 跨平台支持（Linux / macOS / Windows）

## 2. 产品形态（UI/交互）
- 侧边栏新增分组：`TERMINALS`
  - 右侧有“+”新建、可选“刷新”
  - 列表展示：名称（默认使用目录名）、状态（运行中/已退出）、最近活动时间
- 新建终端弹窗：
  - 选择目录（复用项目创建的目录选择弹窗）
  - 名称可选（默认用目录名）
- 终端视图：
  - 右侧主体为终端控制台（可复制/粘贴、滚动、选择文本）
  - 顶部显示终端名称/目录/状态/重连按钮
  - 支持历史输出回放（首次进入渲染历史）

## 3. 后端方案（核心）

### 3.1 核心能力
- 使用 PTY 创建真实 shell 进程并桥接输入/输出。
- 长连接进行双向通信（推荐 WebSocket）。

### 3.2 Rust 技术选型
- **portable-pty**（推荐）：跨平台 PTY 实现
  - Linux/macOS：pty
  - Windows：ConPTY / WinPTY
- Axum WebSocket：用于实时收发终端输入/输出。

### 3.3 终端生命周期
- 创建终端：
  - 生成 terminal_id
  - 记录 cwd / user_id / name / created_at
  - 启动 PTY + shell 进程
  - 保存到内存注册表（TerminalsManager）
- 断线重连：
  - 终端进程仍存在时允许重连
  - 输出历史从内存/DB 回放
- 关闭终端：
  - 主动关闭/进程退出后标记状态

### 3.4 Shell 选择（跨平台）
- macOS/Linux：优先 `$SHELL`，无则 `/bin/bash`
- Windows：优先 PowerShell（`pwsh` / `powershell.exe`），其次 `cmd.exe`

## 4. 数据模型设计（必须持久化）

### 4.1 terminals 表
```
terminals {
  id: string,
  name: string,
  cwd: string,
  user_id: string,
  status: 'running' | 'exited',
  created_at: string,
  updated_at: string,
  last_active_at: string
}
```

### 4.2 terminal_logs 表（必需）
```
terminal_logs {
  id: string,
  terminal_id: string,
  type: 'input' | 'output' | 'system',
  content: string,
  created_at: string
}
```
> 说明：历史输出/指令必须落库（持久化），用于重连和历史回放。可同时保留内存环形缓冲以加速。

## 5. API 设计（建议）

### 5.1 REST
- `POST /api/terminals` 创建终端
  - body: `{ name?, cwd, user_id }`
- `GET /api/terminals?user_id=` 列表
- `GET /api/terminals/:id` 详情
- `DELETE /api/terminals/:id` 关闭
- `GET /api/terminals/:id/history` 获取历史（可选）

### 5.2 WebSocket
- `WS /api/terminals/:id/ws`

**客户端 -> 服务端**
```
{ "type": "input", "data": "ls -la\n" }
{ "type": "resize", "cols": 120, "rows": 30 }
```

**服务端 -> 客户端**
```
{ "type": "output", "data": "..." }
{ "type": "exit", "code": 0 }
{ "type": "history", "items": [...] }
```

## 6. 前端方案（TypeScript）

### 6.1 终端列表组件（TSX）
- 复用 Projects 列表样式/展开折叠逻辑
- 新建终端弹窗复用目录选择器（已有目录 picker）

### 6.2 终端渲染（TSX）
- 使用 **xterm.js**（推荐）作为终端渲染器
  - FitAddon 自动适配
  - WebSocket 双向通信
  - 对输出流进行写入

### 6.3 历史回放
- 进入终端时先请求 `/history` 或在 WS 首包返回
- 将历史输出直接写入 xterm

## 7. 安全与约束
- cwd 必须存在且可访问
- 可添加安全白名单/黑名单（例如仅允许在项目目录下）
- 输出大小限制（防止日志爆炸）

## 8. 交付要求（不做 MVP 拆分）
1. 后端：终端创建 + WS 输入输出（Linux/macOS/Windows）
2. 前端：终端列表 + xterm 渲染 + 基础交互（全部 TypeScript）
3. 历史记录落库（必需），首次进入即回放历史

## 9. 兼容性说明
- Linux/macOS：portable-pty + /bin/bash / $SHELL
- Windows：portable-pty + PowerShell/cmd
- 若无 PTY 支持则降级为“只读输出”或提示不可用

---

## 10. 数据库 Migration（必需）

> 必须持久化，请新增两张表。

### 10.1 SQLite schema（示例）
```
CREATE TABLE terminals (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  cwd TEXT NOT NULL,
  user_id TEXT,
  status TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  last_active_at TEXT NOT NULL
);

CREATE TABLE terminal_logs (
  id TEXT PRIMARY KEY,
  terminal_id TEXT NOT NULL,
  type TEXT NOT NULL, -- input/output/system
  content TEXT NOT NULL,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_terminal_logs_terminal_id ON terminal_logs(terminal_id);
```

### 10.2 Mongo schema（示例）
```
terminals: { id, name, cwd, user_id, status, created_at, updated_at, last_active_at }
terminal_logs: { id, terminal_id, type, content, created_at }
```

## 11. 后端代码骨架（建议）

### 11.1 核心结构
```
terminals/
  manager.rs      // TerminalsManager：保存活跃终端、创建/关闭/重连
  session.rs      // TerminalSession：PTY + IO reader + history buffer
  api.rs          // REST + WS
```

### 11.2 TerminalsManager（伪代码）
```
struct TerminalSession {
  id: String,
  name: String,
  cwd: String,
  user_id: Option<String>,
  status: AtomicStatus,
  history: RingBuffer<String>,
  writer: Arc<Mutex<dyn Write + Send>>,
}

struct TerminalsManager {
  sessions: DashMap<String, Arc<TerminalSession>>,
}

impl TerminalsManager {
  fn create(name, cwd, user_id) -> Arc<TerminalSession> { ... }
  fn get(id) -> Option<Arc<TerminalSession>> { ... }
  fn close(id) { ... }
  fn list(user_id) -> Vec<TerminalInfo> { ... }
}
```

### 11.3 Axum 路由（示例）
```
Router::new()
  .route("/api/terminals", post(create_terminal).get(list_terminals))
  .route("/api/terminals/:id", get(get_terminal).delete(close_terminal))
  .route("/api/terminals/:id/history", get(get_history))
  .route("/api/terminals/:id/ws", get(ws_terminal))
```

### 11.4 WS 消息处理（伪代码）
```
on_ws_message(msg):
  if msg.type == "input": session.write(msg.data)
  if msg.type == "resize": pty.resize(cols, rows)
  if msg.type == "ping": reply pong
```

### 11.5 历史输出策略（持久化）
- 输出/输入实时写入 DB（可批量 flush）
- 内存环形缓冲用于快速回放/避免频繁读库

## 12. 前端细节（xterm.js）

### 12.1 依赖建议
- `xterm`
- `xterm-addon-fit`
- `xterm-addon-web-links`（可选）

### 12.2 组件结构
```
TerminalPanel/
  TerminalList.tsx
  TerminalView.tsx
  NewTerminalDialog.tsx
```

### 12.3 连接流程
1) 新建终端 -> REST 创建 -> 得到 terminal_id
2) 打开 WS -> 绑定 xterm
3) 接收 output -> term.write
4) 输入 -> term.onData -> WS input
5) resize -> FitAddon + WS resize

## 13. 安全与资源管理
- cwd 校验：必须存在且可访问
- 白名单（可选）：限制在某个根目录下（如项目目录）
- 进程数量限制：每用户最多 N 个终端
- 超时策略：无活动自动关闭

## 14. Windows 支持注意点
- 优先使用 `pwsh`，fallback `powershell.exe` / `cmd.exe`
- ConPTY 仅 Windows 10+，低版本需降级或提示
- 处理 CRLF 输出

---

下一步如果你同意方案，我可以继续提供：
1) Rust 端完整模块代码（含 WS/PTY）  
2) 前端 xterm.js 组件 + 终端列表 UI  
3) 数据库迁移脚本与仓储层  
---
如果你认可这个方向，我可以继续补：
- 数据库 migration 细节
- Axum WebSocket 代码骨架
- 前端 xterm.js 集成代码
