# 新一代 Memory Engine 重建方案

## 1. 方案定位

这次不建议继续在当前 `memory_server` 上做抽象升级，而是建议**新建一个真正通用的系统**，专门承接下面两类核心能力：

1. 总结
2. build 上下文

当前 `memory_server` 则从“核心记忆系统”降级为“业务接入方之一”：

1. 继续承载它现有还需要保留的业务能力
2. 通过接口把会话/消息同步给新的系统
3. 逐步删除自己内部关于会话总结、rollup、context compose 的实现

这个方向的关键价值是：

1. 新系统一开始就按平台思路设计，不背 `chatos` 的历史包袱。
2. 现有 `memory_server` 不需要一次性推翻，可以边接边迁。
3. 最终“总结”和“上下文构建”从业务系统中彻底解耦出来，成为公共基础能力。

---

## 2. 为什么要新建，而不是继续改老 memory

结合当前实现，`memory_server` 已经在几个层面深度定制化：

## 2.1 核心数据模型是 chatos 语义驱动的

当前核心主链路是：

1. `sessions`
2. `messages`
3. `session_summaries_v2`
4. `compose_context`

但这些模型和逻辑天然围绕下面这些假设展开：

1. `user_id` 是一级归属键
2. `project_id` 是重要维度
3. `contact / agent` 是上下文拼装的重要信息
4. `session` 是唯一中心实体

这对 `chatos` 合理，但对平台化系统不成立。

## 2.2 总结和上下文逻辑已经与旧领域缠在一起

现在的总结和上下文能力不是孤立模块，而是深嵌在：

1. `repositories/sessions/*`
2. `repositories/messages/*`
3. `repositories/summaries/*`
4. `jobs/summary.rs`
5. `services/context.rs`
6. `contacts_context_api/*`

尤其 `compose_context` 里会直接从 session metadata 推 `agent_id`，再拉 agent recall。这已经不是通用上下文引擎，而是 `chatos memory compose` 了。

## 2.3 继续抽象老系统，成本会越来越高

如果继续在老 `memory_server` 上做“去定制化”，会遇到几个问题：

1. 新旧语义长期共存，代码会越来越绕。
2. 每次抽一层都要照顾已有页面、已有接口、已有 Mongo 结构。
3. 最终即使“抽象成功”，新系统的边界也会被历史兼容逻辑污染。

所以这次更合理的做法是：

**把真正的公共内核单独新建，让老 memory 变成一个接入适配器。**

---

## 3. 目标架构

建议拆成两个系统：

1. `memory_engine`
2. `memory_server`

其中：

## 3.1 `memory_engine`

这是新的平台核心，只负责通用能力：

1. 接收任意子系统同步过来的会话/消息流
2. 生成增量总结
3. 生成更高层级总结
4. 构建可供上游模型使用的上下文
5. 管理长期记忆/主体记忆

它**不理解 chatos 联系人**，也**不理解老 memory 的业务页面语义**。

## 3.2 `memory_server`

这是现有系统，后续角色调整为：

1. 现有业务接口的 BFF / adapter
2. 老页面和老调用链的兼容层
3. 将 `sessions/messages` 等数据通过标准接口同步给 `memory_engine`
4. 调用 `memory_engine` 提供的总结和上下文服务

最终状态下：

1. `memory_server` 不再自己做总结
2. `memory_server` 不再自己做 rollup
3. `memory_server` 不再自己做 context compose
4. `memory_server` 只保留业务特定索引、视图和适配能力

---

## 4. 新系统的职责边界

## 4.1 `memory_engine` 应该负责什么

负责：

1. 通用会话流接入
2. 通用消息标准化
3. 总结 pipeline
4. 多层摘要 rollup
5. 上下文构建
6. 主体长期记忆沉淀
7. 任务调度与执行
8. 模型配置和 prompt/profile 配置

不负责：

1. `chatos` 联系人管理
2. `memory_server` 当前页面的项目列表语义
3. 特定业务 UI
4. 特定业务的权限页和操作流

## 4.2 `memory_server` 应该保留什么

保留：

1. `contacts/projects/agent links` 这一类强业务语义索引
2. 老接口兼容
3. 老前端管理台（如果还要继续用）
4. 对 `memory_engine` 的调用封装

删除：

1. 本地 `summary job`
2. 本地 `summary rollup job`
3. 本地 `agent memory job`
4. 本地 `compose_context`
5. 与会话总结直接耦合的 worker 逻辑

---

## 5. 新系统的核心抽象

新系统的建模必须从一开始就摆脱 `chatos` 的领域词汇。

## 5.1 四个核心概念

建议新系统只围绕下面四个核心概念建模：

1. `Source`
2. `Subject`
3. `Thread`
4. `Record`

分别对应：

1. 谁接入了数据
2. 这些记忆属于谁
3. 这些数据属于哪条连续对话/事件流
4. 每条对话记录本身

## 5.2 Source

`Source` 表示接入系统来源。

建议字段：

1. `tenant_id`
2. `source_id`
3. `source_type`
4. `name`
5. `config`

示例：

1. `source_id = chatos`
2. `source_id = support_widget`
3. `source_id = code_review_bot`

## 5.3 Subject

`Subject` 是记忆归属主体，不强制等于 user。

建议字段：

1. `tenant_id`
2. `source_id`
3. `subject_id`
4. `subject_type`
5. `display_name`
6. `attributes`

`subject_type` 允许：

1. `user`
2. `agent`
3. `contact`
4. `workspace`
5. `anonymous`
6. `custom`

这样对没有用户概念的系统，也可以正常接入。

## 5.4 Thread

`Thread` 是通用对话流，不用 `session` 这个偏 chat 的名字也可以。

建议字段：

1. `thread_id`
2. `tenant_id`
3. `source_id`
4. `subject_id`
5. `thread_type`
6. `external_thread_id`
7. `title`
8. `labels`
9. `metadata`
10. `status`
11. `created_at`
12. `updated_at`

说明：

1. `labels` 用来承载 project/channel/agent/topic 这类扩展信息
2. `metadata` 存业务原始字段

## 5.5 Record

`Record` 用于统一承载消息或事件。

建议字段：

1. `record_id`
2. `thread_id`
3. `external_record_id`
4. `role`
5. `record_type`
6. `content`
7. `structured_payload`
8. `metadata`
9. `summary_status`
10. `created_at`

这样未来不只是 chat message，连事件流也能接。

---

## 6. 新系统的能力模块

## 6.1 Ingestion API

负责让所有子系统把数据标准化写进来。

最少要有：

1. 注册 source
2. upsert subject
3. upsert thread
4. batch sync records

## 6.2 Summary Engine

负责：

1. 从 thread 的新增 records 生成增量总结
2. 多轮总结 rollup
3. 形成 thread 级 summary timeline

## 6.3 Memory Builder

负责：

1. 从多个 thread summary 中提炼 subject 级长期记忆
2. 沉淀更稳定的偏好、约束、背景信息

注意这里不要再叫 `agent memory`，因为新系统不该天然偏 agent。

## 6.4 Context Composer

负责：

1. 按策略组合 thread summary
2. 组合最近 records
3. 组合 subject-level memory
4. 组合可选的 related memory blocks

输出给上游系统用于 prompt 构建。

## 6.5 Execution Worker

负责：

1. 扫描待总结 thread
2. 扫描待构建 subject memory 的主体
3. 调度模型调用
4. 记录 job run

---

## 7. 标准接口设计

新系统建议直接上新的 API 前缀：

`/api/memory-engine/v1`

## 7.1 Source / Subject / Thread 接入

### `PUT /sources/:source_id`

注册来源系统。

### `PUT /subjects/:subject_id`

注册或更新主体。

### `PUT /threads/:thread_id`

幂等创建/更新 thread。

## 7.2 批量同步记录

### `PUT /threads/:thread_id/records:batchSync`

请求体建议：

```json
{
  "tenant_id": "tenant-1",
  "source_id": "chatos",
  "records": [
    {
      "record_id": "r1",
      "external_record_id": "msg-001",
      "role": "user",
      "record_type": "message",
      "content": "你好",
      "created_at": "2026-05-07T10:00:00Z"
    }
  ]
}
```

## 7.3 触发总结

### `POST /threads/:thread_id/summaries:run`

用于手动触发 thread 级总结。

### `POST /subjects/:subject_id/memories:run`

用于手动触发主体级记忆构建。

## 7.4 构建上下文

### `POST /context:compose`

请求体建议：

```json
{
  "tenant_id": "tenant-1",
  "source_id": "chatos",
  "subject_id": "contact:c1",
  "thread_id": "thread-1",
  "policy": {
    "include_recent_records": true,
    "include_thread_summary": true,
    "include_subject_memory": true,
    "recent_record_limit": 20,
    "summary_limit": 4
  }
}
```

返回：

1. thread summary blocks
2. subject memory blocks
3. recent records
4. compose meta

---

## 8. `memory_server` 如何对接新系统

这里是迁移成败的关键。

## 8.1 `memory_server` 作为 source adapter

建议把现有 `memory_server` 定义成 `memory_engine` 的一个 source adapter。

它做三件事：

1. 把自己的 session/message 数据同步到 `memory_engine`
2. 在需要总结和上下文时调用 `memory_engine`
3. 对旧接口返回兼容格式

## 8.2 具体映射关系

现有 `memory_server` 字段到新系统字段映射建议：

1. `user_id -> tenant_id`
2. `memory_server -> source_id`
3. `session.id -> thread_id`
4. `session metadata 中的联系人/agent/project -> labels + metadata`
5. `message -> record`

也就是说：

1. 老 memory 里的 session 还可以继续存在一段时间
2. 但“总结”和“上下文构建”的事实来源逐步迁到 `memory_engine`

## 8.3 同步方式

建议先用同步式 API，对后续再看是否要事件化。

可分两类同步：

1. 写时同步
   - 创建/更新 session 时 upsert thread
   - 写消息时 batch sync record
2. 回填同步
   - 扫描历史 sessions/messages 全量导入

---

## 9. 迁移策略

建议分五期做。

## Phase 1：新系统独立落地

目标：

1. 新建 `memory_engine`
2. 实现自己的存储模型
3. 实现 ingestion API
4. 实现 summary engine
5. 实现 context composer

这个阶段不要被 `memory_server` 旧结构牵着走。

交付结果：

1. 新系统能单独接收一条 thread
2. 能生成摘要
3. 能输出 compose context

## Phase 2：让 `memory_server` 接入新系统

目标：

1. 在 `memory_server` 新增 `memory_engine_client`
2. session/message 写入时同步到 `memory_engine`
3. `compose_context` 改为调用新系统

此时旧 `memory_server` 还能保留原有总结逻辑兜底，但默认走新系统。

## Phase 3：summary 责任切换

目标：

1. 关闭 `memory_server` 本地 summary worker
2. 关闭本地 rollup worker
3. 关闭本地 agent memory worker

由 `memory_engine` 成为唯一总结执行方。

## Phase 4：删除老 memory 中的总结与上下文内核

重点删除：

1. `jobs/summary.rs`
2. `summary_generation/*`
3. `summary_rollup/*`
4. `services/context.rs` 里的核心 compose 逻辑
5. `summary_job_configs / summary_rollup_job_configs / agent_memory_job_configs`
6. 与 `session_summaries_v2` 强绑定的 worker 逻辑

保留兼容层：

1. 如仍需对外暴露 `/context/compose`
2. 则在 `memory_server` 中转调新系统

## Phase 5：收缩旧数据模型

等稳定后再评估：

1. `memory_server` 是否还需要保留自己的 sessions/messages 副本
2. 是否将其变成纯业务索引层
3. 是否将 summary 相关集合完全下线

---

## 10. 新旧职责对比

## 10.1 改造前

`memory_server` 自己负责：

1. session/message 存储
2. summary job
3. summary rollup
4. agent memory
5. compose context
6. 业务索引和业务页面

## 10.2 改造后

`memory_engine` 负责：

1. thread/record 通用存储
2. summary
3. subject memory
4. compose context
5. worker
6. 模型配置与执行

`memory_server` 负责：

1. chatos/memory 兼容接口
2. contacts/projects/links 这类业务化索引
3. source adapter
4. 调用 `memory_engine`

---

## 11. 为什么这个拆法更合理

## 11.1 真正把公共能力提纯出来了

总结和上下文构建本质上是平台能力，不应该被任何一个业务系统绑死。

## 11.2 `memory_server` 可以轻装上阵

它不再承担“大而全的记忆内核”职责，而是专注在：

1. 业务视图
2. 兼容
3. 索引
4. 适配

## 11.3 迁移风险可控

不是一把删老逻辑，而是：

1. 先建新内核
2. 再接老系统
3. 再切执行权
4. 最后删旧实现

这个顺序稳很多。

---

## 12. 风险与应对

## 12.1 风险一：双写与双事实源

迁移期会同时存在：

1. `memory_server` 本地数据
2. `memory_engine` 新数据

应对：

1. 明确阶段性“谁是 summary 的唯一事实源”
2. 写路径双写，读路径单读
3. 通过回填和校验脚本兜底

## 12.2 风险二：老接口返回格式不兼容

应对：

1. `memory_server` 继续做 compatibility facade
2. 新系统输出 block 化结果
3. 在 `memory_server` 做旧格式转换

## 12.3 风险三：新系统又被 chatos 绑住

应对：

1. 新系统禁止出现 `contact/project/agent` 作为核心 schema 字段
2. 只能存在 `source/subject/thread/record/labels/metadata`
3. chatos 语义只能在 adapter 层出现

---

## 13. 实施建议

如果按优先级排，我建议这样做：

### 第一优先级

先定义并实现 `memory_engine` 的最小闭环：

1. thread/record ingest
2. thread summary
3. context compose

### 第二优先级

让 `memory_server` 的 `compose_context` 改成转调新系统。

这个点一旦打通，说明“build 上下文”已经从老系统内核中抽离出来了。

### 第三优先级

切 summary job 执行权。

一旦 summary 也切走，`memory_server` 的“记忆内核”其实就已经空心化了。

### 第四优先级

删除老 worker 和 summary 相关集合依赖。

---

## 14. 推荐的新项目结构

建议在仓库里新增一个独立目录，例如：

`memory_engine/`

结构可参考：

1. `memory_engine/backend`
2. `memory_engine/docs`
3. `memory_engine/sdk`（可选）

`backend` 内建议分层：

1. `api/`
2. `models/`
3. `repositories/`
4. `services/`
5. `jobs/`
6. `adapters/`

其中 `adapters/` 不是给业务系统放逻辑，而是放：

1. 通用 LLM adapter
2. prompt/profile adapter
3. storage adapter

业务系统适配还是放在各业务系统侧更合理。

---

## 15. 验收标准

这套方案成立的标志是：

1. 新系统可以不依赖 `chatos` 语义，独立接入一个最简单的对话型子系统。
2. `memory_server` 可以通过接口把 session/message 同步给新系统。
3. `memory_server` 的上下文构建改为调用新系统。
4. `memory_server` 的总结任务可以停掉而不影响功能。
5. `memory_server` 中会话/总结相关核心逻辑删除后，剩余系统仍能正常工作。

---

## 16. 结论

按你的新思路，最合理的路线不是“把当前 memory 去定制化”，而是：

1. **新建一个 `memory_engine`**
2. **把总结和 build 上下文逻辑整体抽出来放进去**
3. **让现有 `memory_server` 通过接口接入它**
4. **再把现有 `memory_server` 中会话和总结相关内核逐步删掉**

这是一次职责重划，不是一次普通重构。

如果你认可这个方向，我下一步可以继续直接给你写第二份文档：

1. `memory_engine` 的详细数据模型
2. API contract
3. `memory_server` 到 `memory_engine` 的迁移改造清单
4. 哪些老代码可以删，哪些阶段删

这样就可以从“方向方案”进入“实施方案”。
