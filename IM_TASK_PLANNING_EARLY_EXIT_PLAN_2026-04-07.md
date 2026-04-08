# IM 与原项目在“任务已创建后立即结束本轮”的实施方案

## 1. 你的真实需求

这次需要解决的不是“执行阶段重复探索”，而是下面这件事：

> 当 IM 把用户消息发给原项目做联系人聊天/任务规划时，只要这一轮已经成功创建任务、确认任务，或者成功发出了暂停/停止请求，这一轮就必须立刻结束。  
> 不能继续在这一轮里查看任务是否完成，也不能继续轮询任务状态。  
> 任务后续执行与完成，应该走异步任务链路，再由 IM 单独推送结果。

也就是说：

1. **IM 规划 run** 的职责：
   - 理解用户本轮需求
   - 必要时查看最少上下文
   - 创建任务 / 确认任务 / 发暂停停止请求
   - 给用户一个简短结果回复
   - 立刻结束本轮

2. **任务执行链路** 的职责：
   - 后台调度执行任务
   - 任务状态变化时通过 IM 异步通知
   - 任务完成后再给用户推送结果

这两条链路不能混在同一个 IM run 里。

---

## 2. 当前代码现状

## 2.1 其实已经有“强制结束”的雏形，但不够彻底

关键位置：

- [execution_loop.rs](./agent_orchestrator/src/services/v3/ai_client/execution_loop.rs#L31)
- [execution_loop.rs](./agent_orchestrator/src/services/v3/ai_client/execution_loop.rs#L495)
- [execution_loop.rs](./agent_orchestrator/src/services/v3/ai_client/execution_loop.rs#L635)

现在已经有：

- `IM_PLANNING_MUTATION_REPLY_PROMPT`
- `should_force_finish_im_planning_turn(...)`

也就是：

- 如果当前 turn 是 `im-run-*`
- 且工具结果被识别为“成功的任务规划变更”

那么系统会：

1. 清空工具
2. 再追加一条“本轮已经完成，请直接总结并结束”的提示
3. 让模型再生成一次自然语言回复

这说明方向是对的，但现在它还是**“强提醒模型收尾”**，还不是**“程序直接结束本轮”**。

---

## 2.2 当前判断“该不该结束”的条件太窄

在：

- [execution_loop.rs](./agent_orchestrator/src/services/v3/ai_client/execution_loop.rs#L654)

当前只把这些情况视为成功变更：

1. `create_tasks` 返回 `confirmed=true`
2. `confirm_task` 返回了 `task_id`
3. `request_pause_running_task` / `request_stop_running_task` 返回 `requested=true`

这有几个问题：

### 问题 1：create_tasks 只有“确认成功”才会触发强制收尾

如果 `create_tasks` 这一轮产生的是“待用户确认”的任务创建请求，当前 run 仍可能继续。

但从产品语义上，这一轮其实已经完成了：

- 原项目已经完成任务规划
- 现在只差用户确认
- 没必要继续在当前 run 里盯任务

所以：

- `create_tasks` 只要成功产生了 review / draft，也应该结束本轮

### 问题 2：现在还是再交给模型“最后说一句”

虽然已经把工具清空了，但本质上还是：

- 让模型再跑一轮
- 希望它自然结束

这仍然不够硬。

你要的是：

- 一旦任务规划动作成功，程序就判定本轮完成
- 不再让模型自己决定“要不要继续”

---

## 2.3 IM run 与任务生命周期还没有完全解耦

IM run 主流程在：

- [im_orchestrator.rs](./agent_orchestrator/src/services/im_orchestrator.rs#L42)

当前流程是：

1. IM 收到用户消息
2. 创建 `im-run-*`
3. 调 `stream_chat_v3` / `stream_chat_v2`
4. 从事件里提取最终文本
5. 创建一条联系人消息回复给用户
6. 把 run 标记为 `completed`

问题在于：

- 这个“completed”依然依赖原聊天 run 自己自然走完
- 而不是在“任务规划变更已经发生”时由程序直接收口

也就是说，现在是：

- 任务规划成功后，**尝试**让 run 快点结束

你要的是：

- 任务规划成功后，**程序直接结束 run**

---

## 2.4 后台任务异步更新链路其实已经是独立的

这部分在：

- [im_task_runtime_bridge.rs](./agent_orchestrator/src/services/im_task_runtime_bridge.rs#L1)

说明：

- 任务状态更新本来就已经能通过 IM 独立发布
- 所以更应该把“当前 IM 规划 run”与“后续任务状态推送”彻底分开

也就是说，从架构上，这个改造是顺势的，不是逆势的。

---

## 3. 正确的目标行为

## 3.1 IM 到原项目的一轮规划 run，只负责“把任务体系改好”

一轮 IM 规划 run 的终点应该是下面任意一种：

1. 成功创建了待确认任务
2. 成功确认了待确认任务
3. 成功发起了暂停请求
4. 成功发起了停止请求
5. 成功做完本轮任务重排

只要命中任意一种，就必须：

1. 当前 run 立刻结束
2. 返回给 IM 一条简短自然语言总结
3. 不再继续查询 `list_tasks`
4. 不再继续等待任务完成
5. 不再继续查询授权和运行时资产

---

## 3.2 后续任务执行与完成属于异步链路

任务后续状态变化应该这样走：

1. 调度器拿任务去执行
2. 任务执行过程异步更新状态
3. 任务完成/失败后，单独给 IM 发消息
4. IM 再把这条异步消息推给用户

而不是：

- 当前这轮 IM 规划 run 一直挂着，等后台任务跑完

---

## 3.3 当任务正在执行时，用户发送新消息，允许“看状态 + 重排”

这一点必须明确：

> “任务规划成功后立即结束本轮” 不等于 “任务执行期间不能再和联系人交互”。

如果当前联系人 scope 下已经存在：

- `running`
- `pending_execute`
- `pending_confirm`
- `paused`

这些任务，而用户此时又发来一条新消息，那么 IM 仍然应该允许发起一轮新的**规划/调整 run**。

这轮新的 run 允许做两类事：

### A. 查看当前任务状态

例如：

- 当前有哪些任务
- 哪个任务正在执行
- 哪些任务待执行 / 待确认 / 已暂停
- 当前任务的大致标题、状态、更新时间

这类“看任务状态”是允许的，因为它属于**任务编排视角**，不是在等待任务执行完成。

### B. 基于新消息重新编排任务

例如：

- 创建新的后续任务
- 确认已有待确认任务
- 请求暂停当前 running 任务
- 请求停止当前 running 任务
- 后续扩展到调整优先级、恢复暂停任务、取消待执行任务

所以这里的正确边界是：

1. 允许 `list_tasks` 看当前任务状态
2. 允许做任务编排相关 mutation
3. 不允许在这轮里继续等任务跑完
4. 一旦本轮编排动作提交成功，这轮 run 仍然必须立即结束

也就是说：

- 用户在任务执行期间发消息，不是被拒绝
- 也不是把当前 run 一直挂着
- 而是开启一轮新的“任务编排 run”，看状态、做调整、立刻结束

---

## 4. 我建议的改造方案

## 4.1 保留“清空工具后的最后一轮总结”，但必须由程序硬控

当前做法：

- 成功 mutation 后，追加 `IM_PLANNING_MUTATION_REPLY_PROMPT`
- 清空 tools
- 再让模型输出一次话术

这个方向本身是对的，我建议保留。  
因为相比纯程序模板，模型做最后一句总结会更自然，也更符合“联系人在收尾回复你”的体验。

但这里不能只是“尽量提醒模型结束”，而必须升级成：

- 程序强制清空 tools
- 程序强制进入最后一轮 `finalize_only`
- 程序只允许这一轮产出自然语言总结
- 这一轮结束后，run 必须立即结束

建议改成：

### 新增一个“规划变更已提交，进入 finalize_only”的硬终止结果

例如：

```rust
enum PlanningTurnTermination {
    None,
    FinalizeOnly {
        mutation_kind: String,
        payload: Value,
    },
}
```

一旦命中：

- `create_tasks`
- `confirm_task`
- `request_pause_running_task`
- `request_stop_running_task`

且结果成功，就不再继续正常工具循环，而是：

1. 程序把本轮标记为 `planning_committed=true`
2. 程序清空 tools
3. 程序注入固定的“只能总结并结束”的强提示
4. 只允许模型再输出最后一轮自然语言总结
5. 总结产出后立刻返回给 IM orchestrator
6. orchestrator 立即落联系人消息
7. run 立刻标记 `completed`

### 这样做的好处

1. 保留模型自然语言收尾能力
2. 又不会让它继续查任务状态
3. 收尾轮次是程序强控的，不靠模型自由发挥

### 关键限制

这里一定要做硬限制：

1. `finalize_only` 模式下最多只跑 1 轮
2. 这一轮 tools 必须为空
3. 这一轮如果仍试图调用工具，直接拒绝并结束
4. 这一轮如果返回空文本或异常文本，程序模板兜底收尾并结束

也就是说：

- 首选方案：清空工具后让模型给最后总结
- 兜底方案：如果这轮总结失败，程序模板立即收尾

---

## 4.2 扩大“终止条件”覆盖范围

当前太窄，只认：

- `create_tasks.confirmed=true`

建议改成：

### `create_tasks` 只要返回以下任一情况，都视为本轮规划已完成

1. `confirmed=true`
2. `review_required=true`
3. `created_count > 0`
4. 返回了 review payload / action request

也就是说：

- 不管任务是“已创建并确认”
- 还是“已创建但待用户确认”

都应该结束本轮。

### `confirm_task`

只要成功确认，也立刻结束本轮。

### `request_pause_running_task` / `request_stop_running_task`

只要请求已经成功登记，也立刻结束本轮。

### 后续如果有“任务重排/调整任务”工具

也一样：

- 只要 mutation 成功，就结束本轮

### 如果只是查看当前任务状态，也必须快速结束

如果用户这次消息只是问：

- “现在执行到哪了？”
- “当前有哪些任务？”
- “是不是还在跑？”

那么这轮 run 可以调用 `list_tasks` 获取当前状态，并给用户一个简短回答。  
但回答完也必须立即结束，不允许继续等待后台任务状态变化。

---

## 4.3 结束本轮后的回复，优先走“无工具模型总结”，模板只做兜底

建议优先让模型在无工具条件下做最后一句总结，但程序要准备好模板兜底。

### 推荐的最终收尾顺序

1. 命中成功 mutation
2. 清空 tools
3. 注入固定 finalize prompt
4. 让模型输出最后一句总结
5. 如果总结成功，直接用这句回复用户
6. 如果总结失败，再使用程序模板回复用户并结束

### finalize prompt 的语义应该是硬性的

推荐类似下面这类语义：

> 本轮任务规划动作已经提交成功。现在禁止再调用任何工具，禁止继续查看任务状态，禁止等待后台执行结果。你只能用简短自然语言告诉用户：本轮已经创建/确认/调整了哪些任务，以及后续结果会异步通知。回复后立刻结束。

注意这里的关键词应该是：

- 禁止再调用工具
- 禁止继续查看任务状态
- 禁止等待后台执行结果
- 回复后立刻结束

### 程序模板只做异常兜底

如果模型最后这一轮失败，再按 mutation 类型走模板：

### create_tasks 成功且待确认

模板：

> 我已经把本轮需求整理成任务，接下来请你确认。确认后任务会进入待执行并由后台异步处理。

### create_tasks 成功且已确认

模板：

> 我已经创建并确认了本轮任务，任务会在后台异步执行；执行进度和结果会再单独通知你。

### confirm_task 成功

模板：

> 我已经确认该任务，它会进入后台异步执行；执行结果会再单独通知你。

### pause / stop 请求成功

模板：

> 我已经提交了暂停/停止请求，系统会在安全点处理；后续状态会再单独通知你。

如果需要，也可以把任务标题简短拼进去。

但正常情况下，还是优先用“清空工具后的最后一轮模型总结”。

---

## 4.4 在工具层面禁止“成功 mutation 后继续调用工具”

除了程序直接终止 loop，还建议加一层保险：

一旦本轮 IM 规划 turn 命中成功 mutation：

1. 标记 `planning_committed=true`
2. 后续任何工具调用请求直接拒绝

统一返回：

```json
{
  "error": true,
  "code": "im_planning_turn_already_committed",
  "message": "本轮任务规划已经提交，当前 run 必须立即结束，不能继续查询任务状态或调用其他工具"
}
```

这是兜底保险，防止以后别的路径又把 loop 接回去。

---

## 4.5 在 IM orchestrator 层把 run 结束语义改清楚

改动位置：

- [im_orchestrator.rs](./agent_orchestrator/src/services/im_orchestrator.rs)

建议把 run 的结束语义改成：

### run 的 `completed`

只表示：

- 这一轮“联系人聊天/任务规划”已经完成

不表示：

- 任务已经执行完

### 不允许再把任务状态回写成 run 级生命周期

也就是说：

- `pending_execute`
- `running`
- `completed`
- `failed`

这些都是**任务状态**，不是当前 IM run 的状态。

当前 IM run 在创建/确认完任务并给用户回一句话后，就应直接 `completed`。

---

## 4.6 异步任务结果必须只走独立通知链路

后续任务完成、失败、暂停、恢复等，都应该通过：

- [im_task_runtime_bridge.rs](./agent_orchestrator/src/services/im_task_runtime_bridge.rs)

或其扩展链路发给 IM。

建议明确规则：

1. 规划 run 只发“本轮我已经为你创建/确认/调整好了任务”
2. 后台任务链路再发“任务开始执行 / 已暂停 / 已完成 / 已失败”

绝不允许：

- 规划 run 一直挂着等任务结束后再回用户

---

## 5. 推荐实施步骤

## Phase 1：把“成功 mutation 后进入无工具 finalize_only 单轮收尾”做硬

改动：

1. 修改 [execution_loop.rs](./agent_orchestrator/src/services/v3/ai_client/execution_loop.rs)
2. 保留“清空工具后再总结一次”的思路，但升级成程序硬控的 `finalize_only`
3. 限制这轮只能输出自然语言，不能再进工具循环
4. 如果这轮失败，程序模板兜底并结束 loop

目标：

- 让 IM 规划 run 的最后收口自然，但仍然是程序层硬结束

## Phase 2：放宽终止触发条件

改动：

1. `create_tasks` 只要成功产出 review / task draft / created_count，就结束
2. `confirm_task` / `pause` / `stop` 成功就结束

目标：

- 不再只覆盖“confirmed=true”这一种窄情况

## Phase 3：工具层兜底拦截

改动：

1. 一旦 `planning_committed=true`
2. 后续工具调用统一拒绝

目标：

- 避免未来回归

## Phase 4：前端与状态文案对齐

目标：

- 用户看到的是“联系人已接收并创建任务，后续异步通知”
- 不是“联系人还在本轮里盯任务跑完”

---

## 6. 具体需要重点改的文件

### 核心执行收口

1. [execution_loop.rs](./agent_orchestrator/src/services/v3/ai_client/execution_loop.rs)

### IM 主流程

1. [im_orchestrator.rs](./agent_orchestrator/src/services/im_orchestrator.rs)

### 任务规划工具返回规范

1. [review_flow.rs](./agent_orchestrator/src/builtin/task_planner/review_flow.rs)
2. [task_planner/mod.rs](./agent_orchestrator/src/builtin/task_planner/mod.rs)

### 异步任务通知链路

1. [im_task_runtime_bridge.rs](./agent_orchestrator/src/services/im_task_runtime_bridge.rs)

---

## 7. 最终规则

建议把系统规则明确成下面这句：

> IM 与原项目之间的一轮联系人聊天，只负责把本轮任务规划动作提交完成；一旦任务已创建、已确认、已暂停或已停止请求成功，本轮必须立即结束。任务后续执行与完成只走异步通知链路，不允许当前 run 继续等待。

---

## 8. 结论

这次要改的核心不是“随便提示模型别继续”，而是：

**把“任务规划成功”定义成 IM run 的强终点，并允许模型只在“无工具、单轮、受控”的条件下做最后总结。**

也就是：

1. 本轮把任务体系改好
2. 清空工具
3. 让模型只做最后一句总结
4. 立刻结束本轮
5. 后续任务状态异步通知

这样既符合你要的产品语义，也能从程序上彻底杜绝“它一直在看任务有没有完成”。
