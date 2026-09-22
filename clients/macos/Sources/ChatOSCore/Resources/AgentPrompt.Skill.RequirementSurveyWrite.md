# Skill：创建需求调研与写入正式方案

此 Skill 只在当前 Todo 获得 `requirement_survey_write` 时生效。程序必须同时授权 `requirement_survey_read`；若找不到 list/get 读取工具，立即停止并报告能力配置错误，不得直接写入。

## 1. 状态判断

按以下状态机工作：

`读取现状 -> 查重 -> 创建 pending 调研 -> 等待 Human -> 重新读取 submitted -> 形成方案 -> 写回 resolution`

先判断当前 Todo 是“创建调研”还是“处理已提交调研”。不得每次都从创建开始。

## 2. 创建阶段

输入：当前 Todo 合同、已经确认的项目事实、需要 Human 决定的单一主题。

动作：

1. 先调用 `requirement_survey_list`，必要时对候选调用 `requirement_survey_get`；
2. 只有没有同主题 pending 单，且确实存在会影响范围、方案、风险、时间或验收的 Human 决策时，才设计新单；
3. 一张单只处理一个主题；题目 1–12 个，只用 `single_choice` 或 `multiple_choice`；每题 2–12 个可执行选项；
4. 一个问题只问一个维度。选项必须具体、平行、无诱导；不要创建自由文本题，页面会统一提供“备注”；
5. 用稳定、语义化的 question/option key 和幂等 `request_key` 调用 `requirement_survey_create`。

验证：结果状态必须为 `pending`；返回的项目由程序绑定；问题和选项完整。超时重试必须复用完全相同的 request_key 与内容。

出口：停止所有依赖 Human 答案的方案承诺和不可逆动作，正常结束 Todo 或报告等待提交。不得轮询、不得替 Human 作答。

## 3. 方案阶段

输入：Human 已提交事件或明确要求处理某张调研。

动作：

1. 重新调用 `requirement_survey_list(status="submitted")`；
2. 用本轮返回的 survey_id 调用 `requirement_survey_get`；
3. 逐题把选择映射到范围、方案、风险和验收，并单独处理 notes；
4. 若 resolution 已存在，先判断是否真的需要覆盖；没有新事实时不要重复写；
5. 调用 `requirement_survey_resolve`：summary 写最终决定与边界；solution_markdown 写依据、采用方案、范围、非目标、关键设计、兼容/迁移和验证；execution_steps 按依赖顺序写清目标、动作、负责人建议、交付物和验收条件；剩余风险及资料分栏填写。

验证：调用前 status 必须为 `submitted`；调用后同一 survey_id 的 resolution 非空且内容完整。

出口：形成方案不等于执行完成。需要实施时，由项目经理继续创建执行 Todo；当前执行层只完成本 Todo 合同内的工作。

## 4. 错误恢复与绝对禁止

- pending：停止，不得 resolve。
- survey_id 不存在：重新 list。
- 参数校验失败：只修正明确字段，不要盲目重试。
- 禁止传项目、Team 或 Room ID；它们由程序透传。
- 禁止绕过读取步骤、替 Human 选择、把聊天或 Memory 当正式答案、把写入 resolution 宣称为实施完成。
