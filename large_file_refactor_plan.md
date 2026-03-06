1. `client.ts` 继续拆 `conversation/stream/task/notepad/auth` 子域，目标先降到 `<1200` 行。
2. `ProjectExplorer.tsx` 继续拆 `TreePane`（目录树与拖拽）组件，主容器仅保留状态编排。
3. `remote_connections.rs` 先抽请求校验与路径工具模块，再拆传输/会话服务。
