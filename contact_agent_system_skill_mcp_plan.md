# 联系人智能体上下文、Agent-Plugin 关联与技能详情 MCP 改造方案

## 1. 最终结论

这次方案需要调整，核心结论是：

**agent 不应该只关联 skill，还应该显式关联 plugin。**

结合当前代码和你的产品语义，我建议最终模型改成：

1. `plugin` 是一级能力包
2. `skill` 是 plugin 下的具体技能项
3. `agent` 需要显式关联 `plugin_sources`
4. `agent` 仍然可以继续关联 `skill_ids`
5. 聊天运行时：
   - plugin 详情始终给模型
   - skill 索引始终给模型
   - skill 正文按需通过内置 MCP 获取

一句话：

**agent = 角色定义 + plugin 范围 + skill 细粒度引用。**

---

## 2. 当前代码里的真实关系

## 2.1 plugin 和 skill 当前本来就是分开的

当前 Memory 里：

1. `memory_skill_plugins` 存 plugin 元数据
2. `memory_skills` 存具体 skill
3. `skill.plugin_source` 指向 plugin 的 `source`

所以当前的真实关系是：

- 一个 plugin 对应多个 skills
- `plugin -> skills` 是明确的 `1:N`

关键代码：

- [memory_server/backend/src/repositories/skills.rs](/Users/lilei/project/my_project/chatos_rs/memory_server/backend/src/repositories/skills.rs)
- [memory_server/backend/src/services/skills/io_discovery.rs](/Users/lilei/project/my_project/chatos_rs/memory_server/backend/src/services/skills/io_discovery.rs)
- [memory_server/backend/src/services/skills/manage_service.rs](/Users/lilei/project/my_project/chatos_rs/memory_server/backend/src/services/skills/manage_service.rs)

## 2.2 agent 现在只关联了 skill，没有关联 plugin

当前 `MemoryAgent` 里只有：

- `skills`（内联技能）
- `skill_ids`
- `default_skill_ids`

没有：

- `plugin_sources`
- `plugin_ids`

关键代码：

- [memory_server/backend/src/models/agents.rs](/Users/lilei/project/my_project/chatos_rs/memory_server/backend/src/models/agents.rs)
- [memory_server/backend/src/repositories/agents.rs](/Users/lilei/project/my_project/chatos_rs/memory_server/backend/src/repositories/agents.rs)

所以现在 agent 的能力边界其实是不完整的。

## 2.3 为什么只存 skill_ids 不够

这也是你指出来的关键问题。

只存 `skill_ids` 会有几个问题：

1. 很多 skill 没有单独 description
2. plugin 才有更稳定的能力说明
3. 聊天时模型不知道一组 skill 属于哪个能力包
4. 如果一个 agent 基于某几个 plugin 搭建，当前模型看不到这层“能力域”
5. AI 创建 agent 时其实已经会看到 `visible_plugins + visible_skills`
   - 但最后落库只保留了 `skill_ids`
   - 这会让创建态和运行态的信息层级不一致

关键代码：

- [memory_server/backend/src/services/agent_builder.rs](/Users/lilei/project/my_project/chatos_rs/memory_server/backend/src/services/agent_builder.rs)

所以这次应该把“plugin 关联”补成 agent 的一等公民。

---

## 3. 应该怎么建模

## 3.1 建议 agent 显式增加 plugin_sources

我建议在 agent 上新增：

```rust
pub plugin_sources: Vec<String>
```

而不是新增 `plugin_ids`。

原因：

1. 当前 `skill` 用的是 `plugin_source`
2. plugin 的自然关联键也是 `source`
3. 运行时按 `plugin_source` 回查 plugin 元数据最直接
4. 如果用 `plugin_id`，还要额外做一层转换

所以建议统一用：

- `plugin_sources`
- `skill_ids`

## 3.2 为什么不是单独做一张关联表

从“关系表达”上讲，你说的没错，agent 和 plugin 需要有显式关联。

但在当前仓库里，Memory backend 已经是文档模型风格，agent 本身就是一份文档记录。基于这个现状，我建议：

1. 先在 `MemoryAgent` 里加 `plugin_sources: Vec<String>`
2. 不急着新建 `agent_plugin_links` 表/集合

原因：

1. 当前查询路径简单
2. 运行时直接取 agent 文档即可
3. 你的主要目标是把 plugin 这层能力正确透传给聊天模型
4. 不需要先把结构做复杂

如果以后要支持：

1. plugin 级开关
2. plugin 排序
3. plugin 权限继承
4. 关联审计

再单独拆表也不迟。

所以我的建议是：

**这次做“显式关联”，但先用 agent 文档里的 `plugin_sources` 数组实现，不额外加表。**

---

## 4. 调整后的 agent 模型语义

改造后，agent 的语义应该变成：

### 4.1 plugin_sources

表示：

**这个 agent 建立在哪些 plugin 能力包之上。**

作用：

1. 决定 agent 的能力域
2. 决定聊天时哪些 plugin 概览进入 system
3. 决定 skill 选择范围
4. 决定 skill 详情 MCP 的合法查询边界的一部分

### 4.2 skill_ids

表示：

**这个 agent 明确关联的具体技能。**

作用：

1. system 中展示 skill 索引
2. 作为更细粒度的能力引用
3. 作为模型后续按 `skill_id` 下钻的目标

### 4.3 两者关系

建议定义为：

1. `plugin_sources` 是粗粒度能力范围
2. `skill_ids` 是细粒度能力引用
3. `skill_ids` 中的技能应当属于 `plugin_sources` 中的某个 plugin
4. 内联 skill 不受 `plugin_sources` 约束

也就是说：

- plugin 决定“这个 agent 属于哪些能力包”
- skill 决定“这个 agent 明确挂了哪些具体技能”

---

## 5. 创建智能体时应该怎么做

## 5.1 AI 创建 agent 时，模型应该同时输出 plugin_sources 和 skill_ids

既然 agent 现在要显式关联 plugin，那么 AI 创建 agent 时，应该同时让模型决定：

1. 这个 agent 依赖哪些 plugin
2. 这个 agent 挂哪些 skills

所以 Memory 的 AI 创建 agent 请求结构建议扩展为：

```json
{
  "name": "...",
  "description": "...",
  "category": "...",
  "role_definition": "...",
  "plugin_sources": ["frontend_toolkit", "api_ops"],
  "skill_ids": ["skill_a", "skill_b"],
  "default_skill_ids": ["skill_a"],
  "enabled": true
}
```

## 5.2 创建时的校验规则

建议新增以下校验：

1. `plugin_sources` 去重
2. `skill_ids` 去重
3. `default_skill_ids` 必须属于 `skill_ids`
4. 每个技能中心 skill 必须满足：
   - `skill.plugin_source in plugin_sources`
5. 如果用户只选了 `skill_ids` 没选 `plugin_sources`
   - 后端自动把 skill 对应的 `plugin_source` 补进去
6. 如果用户只选了 `plugin_sources`，没选 `skill_ids`
   - 允许，但意味着这个 agent 只有 plugin 概览，没有明确 skill 索引
   - 这类情况后续要谨慎，建议 UI 仍鼓励至少选一个 skill

我的建议是：

**创建时允许自动补齐 plugin_sources，但最终 agent 记录里必须显式保存 plugin_sources。**

## 5.3 前端编辑体验也要分开

Memory 的 agent 编辑页不应该只让用户选 skill。

应该改成：

1. 先选 plugin
2. 再选 skill
3. skill 选项按已选 plugin 过滤

这会比当前只选 `skill_ids` 更符合认知。

---

## 6. 联系人聊天时模型应该看到什么

当用户在 ChatOS 里添加联系人“小林”并对话时，模型应该看到一个单独的联系人 system 消息，内容分三层：

1. agent 基本信息
2. plugin 概览
3. skill 索引

并且这条 system 消息不要和 Memory 总结混在一起。

## 6.1 推荐结构

```text
你正在以联系人智能体身份参与对话。

联系人名称：小林
联系人简介：...
联系人分类：...

角色定义：
...

关联插件：
1. plugin_source=frontend_toolkit | 名称=前端工具箱 | 分类=frontend | 简介=用于组件设计、渲染排查、状态管理分析
2. plugin_source=api_ops | 名称=接口运维包 | 分类=backend | 简介=用于接口链路排查、请求分析和报错定位

关联技能：
1. skill_id=xxx | plugin_source=frontend_toolkit | 名称=... | 简介=...
2. skill_id=yyy | plugin_source=api_ops | 名称=... | 简介=...

如果需要查看某个 skill 的完整内容，请调用内置工具 `memory_skill_reader_get_skill_detail`。
```

## 6.2 为什么 plugin 详情应该始终给模型

这里我同意你的判断，但要精确定义“给哪些 plugin”。

我建议：

**始终给当前 agent 显式关联的 plugin_sources 对应的 plugin 概览。**

不建议给“当前账号所有 plugin”，因为那会：

1. 噪音太大
2. 和 agent 边界不一致
3. 提高 prompt 成本

所以准确说法应该是：

**plugin 详情应该始终给到模型，但范围是当前 agent 关联的 plugin，而不是全量 plugin。**

---

## 7. 运行时接口应该怎么变

## 7.1 Memory runtime-context 需要扩展

当前 runtime-context 只够支撑 role_definition + skill_ids，不够支撑 plugin 层。

建议扩展为：

```rust
pub struct MemoryAgentRuntimePluginSummary {
    pub source: String,
    pub name: String,
    pub category: Option<String>,
    pub description: Option<String>,
    pub updated_at: Option<String>,
}

pub struct MemoryAgentRuntimeSkillSummary {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub plugin_source: Option<String>,
    pub source_type: String,
    pub source_path: Option<String>,
    pub updated_at: Option<String>,
}

pub struct MemoryAgentRuntimeContext {
    pub agent_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: String,
    pub plugin_sources: Vec<String>,
    pub skill_ids: Vec<String>,
    pub runtime_plugins: Vec<MemoryAgentRuntimePluginSummary>,
    pub runtime_skills: Vec<MemoryAgentRuntimeSkillSummary>,
    pub skills: Vec<MemoryAgentSkill>,
    pub mcp_policy: Option<Value>,
    pub project_policy: Option<Value>,
    pub updated_at: String,
}
```

## 7.2 runtime-context 的解析规则

建议逻辑：

1. 读取 agent
2. 取出 `plugin_sources`
3. 查询这些 plugin 元数据，生成 `runtime_plugins`
4. 取出 `skill_ids`
5. 查询这些 skill，生成 `runtime_skills`
6. 校验 skill 的 `plugin_source` 是否属于 `plugin_sources`
7. 内联 skill 单独补到 `runtime_skills`

这样运行时上下文就是显式的，而不是再从 skill 反推 plugin。

这点很重要：

**既然决定 agent 要显式关联 plugin，运行时就应该优先以 agent.plugin_sources 为准，而不是再靠 skill_ids 推导。**

---

## 8. skill 详情 MCP 方案

这块仍然保留，而且仍然有必要。

## 8.1 新的内置 MCP

新增一个内置 builtin MCP，例如：

- `memory_skill_reader`

提供工具：

- `get_skill_detail`

最终暴露给模型的函数名会自动带前缀，类似：

- `memory_skill_reader_xxx_get_skill_detail`

## 8.2 工具输入输出

输入：

```json
{
  "skill_id": "xxx"
}
```

输出：

```json
{
  "agent_id": "...",
  "skill_id": "xxx",
  "name": "...",
  "description": "...",
  "content": "...",
  "plugin_source": "frontend_toolkit",
  "source_path": "skills/.../SKILL.md",
  "source_type": "skill_center",
  "updated_at": "..."
}
```

## 8.3 工具边界

这个工具不应该允许查当前账号下任意 skill。

建议限制为：

1. 当前联系人 agent 的 `skill_ids`
2. 当前联系人 agent 的内联 skills

如果某个 `skill_id` 不属于这个 agent，则报错。

这个边界和 `plugin_sources` 是一致的。

---

## 9. chat_app_server_rs 应该怎么改

## 9.1 联系人 system 消息要独立，不再只走 instructions

这点不变。

联系人上下文不能继续只走 `instructions`，应该变成独立 `system` item。

原因：

1. 你明确要求与记忆总结分开
2. 未来调试更直观
3. plugin 层和 skill 层分开展示更合适

涉及文件：

- [chat_app_server_rs/src/api/chat_v3.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/api/chat_v3.rs)
- [chat_app_server_rs/src/core/chat_runtime.rs](/Users/lilei/project/my_project/chatos_rs/chat_app_server_rs/src/core/chat_runtime.rs)

## 9.2 必须支持 prefixed input items

这点也不变，而且现在更重要。

因为联系人 system item 如果不进入 `AiClient` 的统一上下文重建链路，那么在以下场景会丢：

1. `previous_response_id`
2. fallback 到 stateless
3. request error recovery
4. completion error recovery

所以仍建议在 `ProcessOptions` 中新增：

```rust
pub prefixed_input_items: Option<Vec<Value>>
```

让联系人 system item 和后续 Memory summary system item 都能被稳定带入。

---

## 10. Memory 侧的数据结构改动建议

## 10.1 agent 增加 plugin_sources

建议改为：

```rust
pub struct MemoryAgent {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub role_definition: String,
    pub plugin_sources: Vec<String>,
    pub skills: Vec<MemoryAgentSkill>,
    pub skill_ids: Vec<String>,
    pub default_skill_ids: Vec<String>,
    ...
}
```

## 10.2 API 请求结构同步扩展

创建和编辑请求都需要支持：

- `plugin_sources`

涉及文件：

- [memory_server/backend/src/models/agents.rs](/Users/lilei/project/my_project/chatos_rs/memory_server/backend/src/models/agents.rs)
- [memory_server/backend/src/api/agents_api.rs](/Users/lilei/project/my_project/chatos_rs/memory_server/backend/src/api/agents_api.rs)
- [memory_server/backend/src/repositories/agents.rs](/Users/lilei/project/my_project/chatos_rs/memory_server/backend/src/repositories/agents.rs)
- [memory_server/frontend/src/api/client.ts](/Users/lilei/project/my_project/chatos_rs/memory_server/frontend/src/api/client.ts)
- [memory_server/frontend/src/types/index.ts](/Users/lilei/project/my_project/chatos_rs/memory_server/frontend/src/types/index.ts)
- [memory_server/frontend/src/pages/AgentsPage.tsx](/Users/lilei/project/my_project/chatos_rs/memory_server/frontend/src/pages/AgentsPage.tsx)

## 10.3 迁移策略

现有 agent 没有 `plugin_sources`，需要补迁移逻辑。

建议回填规则：

1. 遍历现有 agent
2. 根据 `skill_ids` 找到对应 skill
3. 收集 skill 的 `plugin_source`
4. 回填到 `plugin_sources`
5. 内联 skill 不参与 plugin_sources 回填

这样历史数据可以平滑兼容。

---

## 11. 前端编辑页应该怎么调整

你前面已经指出，agent 编辑页应该更结构化，这里也要跟着升级。

建议改成：

1. `插件引用` 多选
2. `技能引用` 多选
   - 只展示所选 plugin 下的 skills
3. 若已有 skill 但 plugin 未选
   - 自动补选对应 plugin

这样用户体验会比当前只有 skill 多选清晰很多。

推荐顺序：

1. 先选 plugin
2. 再选 skill
3. skill preview 仍可查看 `.md` 内容

---

## 12. 最终的运行时分层

这次改完以后，系统层次应该是：

### 持久化层

- `plugin`：能力包元数据
- `skill`：plugin 下的具体技能
- `agent.plugin_sources`：agent 绑定的能力包
- `agent.skill_ids`：agent 绑定的具体技能

### 运行时上下文层

- agent 基本信息
- plugin 概览
- skill 索引

### 工具层

- skill 详情按 `skill_id` 获取

这是比“只存 skill_ids”更完整的一层。

---

## 13. 推荐实施顺序

### 第 1 步

先改 Memory agent 模型：

1. 新增 `plugin_sources`
2. 创建/更新 API 支持它
3. 历史数据回填

### 第 2 步

改 Memory runtime-context：

1. 返回 `runtime_plugins`
2. 返回 `runtime_skills`
3. 返回扩展后的 agent 基本信息

### 第 3 步

改 Memory AI 创建 agent：

1. 模型输出 `plugin_sources`
2. 创建时校验 `skill_ids` 属于这些 plugins
3. 自动补齐 plugin_sources

### 第 4 步

改 Memory 前端 agent 编辑页：

1. 增加 plugin 多选
2. skill 受 plugin 过滤

### 第 5 步

改 chat_app_server_rs：

1. 联系人 system message 独立化
2. system message 中加入 plugin 概览 + skill 索引
3. `AiClient` 支持 prefixed input items
4. 加 `memory_skill_reader` builtin MCP

---

## 14. 我的最终建议

如果按你的产品语义来做，我现在的最终建议是：

1. **是的，agent 应该和 plugin 建立显式关联**
2. **创建 agent 时也应该把 plugin 选出来并保存**
3. **skill 和 plugin 不应该混成一个字段**
4. **plugin 是能力包层，skill 是具体能力层**
5. **聊天时 plugin 概览应始终进入模型 system**
6. **skill 正文通过 MCP 按需拉取**

所以这次方案的核心修正就是：

**从“agent 只绑 skill”升级为“agent 显式绑 plugin + skill”。**
