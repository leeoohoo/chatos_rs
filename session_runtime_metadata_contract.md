# Session Runtime Metadata Contract

更新时间：2026-03-18

## 1. 唯一读写入口

- 读取：`chat_app/src/lib/store/helpers/sessionRuntime.ts` 中 `readSessionRuntimeFromMetadata`
- 写入：`chat_app/src/lib/store/helpers/sessionRuntime.ts` 中 `mergeSessionRuntimeIntoMetadata`
- UI 统一状态同步：`chat_app/src/features/sessionRuntime/useSessionRuntimeSettings.ts`

禁止在业务组件里直接拼接 `metadata.chat_runtime` JSON 结构。

## 2. chat_runtime 字段

当前前端会读写以下字段：

- `contactId: string | null`
- `contactAgentId: string | null`
- `projectId: string | null`
- `projectRoot: string | null`
- `workspaceRoot: string | null`
- `mcpEnabled: boolean`
- `enabledMcpIds: string[]`
- `selectedModelId: string | null`

## 3. 行为约定

- `projectId` 空值统一视为 `'0'`（非项目对话）。
- `enabledMcpIds` 必须去重、去空字符串。
- `workspaceRoot` 必须做 trim，空字符串归一化为 `null`。
- 会话切换时，运行态只从 `currentSession.metadata` 回填，不能混用其他组件缓存。

## 4. 当前接入模块

- `ChatInterface.tsx`：MCP 开关、MCP 选择、工作目录
- `TeamMembersPane.tsx`：MCP 开关、MCP 选择
- `SessionList.tsx`：联系人会话首次创建时写入 `chat_runtime`

## 5. 后续扩展建议

- 若新增字段（例如 `toolPolicy` / `permissionScope`），先在 `sessionRuntime.ts` 增补解析与合并，再由业务层消费。
- 为 `useSessionRuntimeSettings` 增加单元测试，覆盖“切会话后状态恢复”与“重复值不触发 updateSession”。
