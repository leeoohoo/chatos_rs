# AI 模型供应商与输入栏选择改造方案

日期：2026-06-03

## 结论

1. 输入栏里的“服务器: 不选择”不建议直接删除。它现在不是 AI 模型服务器选择，而是远程 SSH/工具上下文选择，会通过 `remote_connection_id` 透传给后端工具运行时。建议改名为“远程: 不使用”或放进工具上下文菜单，并在没有远程连接或没有启用远程/MCP 工具时隐藏，避免和 AI provider/base URL 混淆。
2. 模型名称和思考等级可以、也应该放到输入框附近选择。当前它们绑定在“AI 模型管理”弹窗的持久配置里，切模型必须编辑配置，体验很差。更合理的拆法是：弹窗管理供应商连接和密钥，输入栏选择本次或本会话使用的具体模型与思考等级。
3. `/models` 这件事可以做，但必须做 provider 适配。OpenAI 是 `GET /v1/models`，Kimi 是 `GET https://api.moonshot.ai/v1/models`，DeepSeek 官方文档当前写的是 `GET https://api.deepseek.com/models`。用 OpenAI SDK 时，关键是按 provider 设置正确 `base_url`，再走 `client.models.list()`。
4. DeepSeek、Kimi、GPT 都能走 OpenAI-compatible 体系，但“思考等级”不是同一套参数。GPT 用 Responses API 的 `reasoning.effort`；DeepSeek 用 Chat Completions 的 `reasoning_effort` 加 `extra_body.thinking`；Kimi 主要是模型级 thinking/非 thinking 与 `reasoning_content`。不能继续硬编码“只有 gpt 支持 thinking_level”。
5. 严格“统一都用 OpenAI SDK”的最佳落点不是在 Rust 后端里继续手写 HTTP。当前主链路在 `chat_app_server_rs` 使用 `reqwest` 手写 `/responses`、`/chat/completions`、SSE 解析。若要真正统一 SDK，应新增一个服务端内部 LLM gateway（Python 或 Node，使用官方 OpenAI SDK），Rust 后端只调用这个内部 gateway。

## 官方能力确认

- OpenAI 官方 API 支持 `GET https://api.openai.com/v1/models` 获取当前可用模型；Responses API 的 reasoning 支持 `effort`，值包含 `none/minimal/low/medium/high/xhigh`，并支持 reasoning summary。参考：
  - https://platform.openai.com/docs/api-reference/models/list
  - https://platform.openai.com/docs/api-reference/responses
  - https://platform.openai.com/docs/guides/reasoning
- DeepSeek 官方文档说明 API 兼容 OpenAI/Anthropic 格式，可通过 OpenAI SDK 修改 `base_url` 调用；模型列表接口是 `GET https://api.deepseek.com/models`；thinking 模式通过 `extra_body={"thinking": {"type": "enabled/disabled"}}` 和 `reasoning_effort` 控制。参考：
  - https://api-docs.deepseek.com/
  - https://api-docs.deepseek.com/zh-cn/api/list-models
  - https://api-docs.deepseek.com/guides/thinking_mode
- Kimi/Moonshot 官方文档说明 Kimi Open Platform 提供 OpenAI-compatible HTTP API，SDK 使用 `base_url=https://api.moonshot.ai/v1`；模型列表接口是 `/v1/models`，返回字段包含 `supports_image_in`、`supports_video_in`、`supports_reasoning`。thinking 模型通过 `kimi-k2-thinking` 或 `kimi-k2.5` 等模型能力体现，reasoning 内容通过 `reasoning_content` 返回。参考：
  - https://platform.kimi.ai/docs/api/overview
  - https://platform.kimi.ai/docs/api/list-models
  - https://platform.moonshot.ai/docs/guide/use-kimi-k2-thinking-model.en-US

## 现有代码判断

前端：

- `chat_app/src/components/AiModelManager.tsx` 与 `chat_app/src/components/aiModelManager/AiModelManagerForm.tsx` 现在把 provider、base_url、api_key、model_name、thinking_level 都放在 AI 模型配置弹窗里。
- `chat_app/src/components/inputArea/InlineWidgets.tsx` 里的 `InputAreaFloatingModelPicker` 只选择已保存的 `AiModelConfig.id`，显示 `Model: ${selectedModel.name}`。
- `chat_app/src/components/inputArea/pickerWidgets/InputAreaRemoteConnectionPicker.tsx` 的“服务器: 不选择”是远程连接选择，不是模型 provider 选择。
- `chat_app/src/lib/api/client/stream.ts` 发送聊天时只传 `model_config_id`，`ai_model_config` 目前只传 `temperature`。
- `chat_app/src/lib/store/actions/sendMessage.ts` 会把 `selectedModelId` 解成 `selectedModel`，再调用 `client.sendChatCommand(...)`。

后端：

- `chat_app_server_rs/src/models/ai_model_config.rs` 的 `AiModelConfig` 是“一个配置绑定一个具体 model”。
- `chat_app_server_rs/src/api/configs/ai_model.rs` 的 provider 白名单是 `gpt/deepseek/kimik2/minimax`，并且 `thinking_level` 只允许 `gpt`。
- `chat_app_server_rs/src/services/model_runtime_resolver.rs` 会按 `model_config_id`、会话 `selected_model_id`、唯一启用模型三种方式解析运行时模型配置，但 `merge_safe_request_overrides` 只允许覆盖 `temperature/system_prompt/use_active_system_context`。
- `chat_app_server_rs/src/services/agent_runtime/ai_request_handler` 目前用 `reqwest` 手写 OpenAI-compatible 请求，不是 OpenAI SDK。

## 建议的数据模型

短期不做破坏性重构，保留 `ai_model_configs`，把它语义调整为“供应商连接配置 + 默认模型”：

- `provider`: 标准化为 `gpt/deepseek/kimi/minimax/openai_compatible`。保留 `kimik2 -> kimi` 兼容别名。
- `base_url`: provider 默认值可自动填充，但允许用户覆盖。
- `api_key`: 继续只保存在后端，不下发明文。
- `model`: 改为默认模型，不再代表这个配置只能使用这一个模型。
- `thinking_level`: 改为默认思考等级。
- 新增缓存字段或独立表：
  - `models_cache_json`
  - `models_last_refreshed_at`
  - `models_fetch_error`
  - 可选：`provider_options_json`

中期可以拆成两张表：

- `ai_provider_profiles`: provider/base_url/api_key/enabled/user_id
- `ai_model_presets`: provider_profile_id/model_name/default_thinking/capability flags

## 后端 API 设计

新增模型发现接口：

```http
GET /api/ai-model-configs/:config_id/models?refresh=false
```

返回：

```json
{
  "provider_config_id": "cfg_123",
  "provider": "kimi",
  "base_url": "https://api.moonshot.ai/v1",
  "source": "live|cache|fallback",
  "fetched_at": "2026-06-03T00:00:00Z",
  "models": [
    {
      "id": "kimi-k2.5",
      "owned_by": "moonshot",
      "context_length": 262144,
      "supports_images": true,
      "supports_video": true,
      "supports_reasoning": true,
      "supports_responses": false,
      "reasoning_modes": ["auto", "off"]
    }
  ],
  "error": null
}
```

发送消息扩展：

```json
{
  "model_config_id": "cfg_123",
  "ai_model_config": {
    "temperature": 0.7,
    "model_name": "kimi-k2.5",
    "thinking_level": "high"
  }
}
```

后端合并规则：

- 只允许覆盖非密钥字段：`model_name/thinking_level/temperature/system_prompt/use_active_system_context`。
- `model_config_id` 仍是权限边界，API Key/base_url/provider 从服务端保存配置读取。
- 如果 `model_name` 来自 `/models` 缓存，优先标记能力；如果是手动输入，允许发送但按 provider 默认能力保守处理。
- 对 `thinking_level` 做 provider 映射，不再用“非 gpt 一律报错”。

## Provider 适配策略

GPT/OpenAI：

- 默认 `base_url=https://api.openai.com/v1`。
- 模型列表走 `client.models.list()`。
- 生成优先走 Responses API。
- 思考等级映射到 `reasoning: { effort, summary: "auto" }`。
- 若模型不支持 reasoning，UI 不显示或置灰思考等级。

DeepSeek：

- 默认 OpenAI SDK `base_url=https://api.deepseek.com`。
- 模型列表走 `client.models.list()`，对应官方 `GET /models`。
- 生成走 Chat Completions。
- thinking 关闭：`extra_body: { "thinking": { "type": "disabled" } }`。
- thinking 开启：`extra_body: { "thinking": { "type": "enabled" } }`，并传 `reasoning_effort`。
- 等级映射：`none/off -> disabled`，`low/medium/high -> high`，`xhigh -> max`。DeepSeek 官方说明 low/medium 会兼容映射到 high，xhigh 映射到 max。
- thinking 模式下不要主动传 `temperature/top_p/presence_penalty/frequency_penalty`，避免无效参数造成误解。
- 从 `reasoning_content` 抽取推理内容，延续当前前端“思考”展示。

Kimi/Moonshot：

- 默认 `base_url=https://api.moonshot.ai/v1`。
- 模型列表走 `client.models.list()`，模型能力字段直接读取 `supports_image_in/supports_video_in/supports_reasoning`。
- 生成走 Chat Completions。
- `kimi-k2-thinking` 是强 thinking 模型；`kimi-k2.5` thinking 默认开启，可通过 provider-specific thinking toggle 关闭。
- 从 `reasoning_content` 抽取推理内容。
- UI 上不要把 Kimi 展示成 OpenAI 的 `low/medium/high/xhigh` 完全等价能力，建议用 `自动/关闭`，后续再加更细粒度。

通用 OpenAI-compatible：

- 允许用户自定义 provider key、base_url、model name。
- `/models` 获取失败时允许手动输入模型名。
- 默认只假设 Chat Completions、文本输入输出；图片、Responses、reasoning 需要用户显式打开或由模型列表能力字段确认。

## “统一 OpenAI SDK”落地方案

因为当前主服务是 Rust，且仓库里没有 Rust OpenAI SDK 依赖，严格统一 SDK 建议采用服务端内部 gateway：

1. 新增 `llm-provider-gateway/`，使用官方 OpenAI Python SDK 或 Node SDK。
2. Rust 后端不再直接请求第三方 provider，只请求内部 gateway。
3. 内部 gateway 对外提供：
   - `POST /internal/llm/models`
   - `POST /internal/llm/chat-completions`
   - `POST /internal/llm/responses`
   - 流式统一输出为现有 Rust 能消费的 SSE 事件。
4. provider-specific 参数由 gateway 适配：
   - SDK 标准字段走标准参数。
   - DeepSeek/Kimi 的 `thinking`、`reasoning_content` 走 SDK 的 `extra_body` 或 raw response 兼容读取。
5. Rust 的 `AiRequestHandler` 改成 `InternalLlmGatewayClient`，保留现有回调、工具调用循环、消息持久化逻辑。
6. 先加开关：
   - `LLM_PROVIDER_GATEWAY_ENABLED=false`
   - 灰度验证通过后删除 Rust 直连 provider 的老路径。

如果短期不想引入新进程，可以先用 Rust `reqwest` 完成 `/models` 和 UI 改造；但这不满足“统一都用 OpenAI SDK”的严格要求，只能算 OpenAI-compatible HTTP。

## 输入栏 UI 方案

把当前浮动 `Model: xxx` 改成输入框上方或左侧的紧凑 AI 控制组：

- 第一层：供应商连接配置选择，例如 `GPT / DeepSeek / Kimi`，显示配置名，不显示 API Key。
- 第二层：模型 ComboBox，打开时调用 `/api/ai-model-configs/:id/models`，支持搜索、刷新、手动输入。
- 第三层：思考等级控制：
  - GPT: `自动/无/minimal/low/medium/high/xhigh`
  - DeepSeek: `关闭/high/max`，也可显示为 `关闭/思考/深度思考`
  - Kimi: `自动/关闭`，thinking-only 模型显示锁定
- 继续保留推理开关，但它应变成 provider 能力下的快捷启停；不要和 thinking_level 冲突。
- 当前远程服务器选择改名为“远程: 不使用”，放在工具/MCP 控制附近，tooltip 写明“给终端、SSH、远程工具使用”。

会话持久化：

- 复用现有 session metadata 机制，保存：
  - `selectedProviderConfigId`
  - `selectedModelName`
  - `selectedThinkingLevel`
- 兼容现有 `selectedModelId`，迁移期将其视为 provider config id。

## 实施阶段

阶段 1：低风险体验修正

- 改名或移动 `InputAreaRemoteConnectionPicker`，避免“服务器”误解。
- 输入栏模型选择器显示为 `配置名 / 模型名`，不再只显示配置名。
- 会话 metadata 增加模型名和思考等级记忆。

阶段 2：模型发现

- 后端新增 `/api/ai-model-configs/:id/models`。
- 新增 provider registry：OpenAI、DeepSeek、Kimi、通用 OpenAI-compatible。
- 前端模型 ComboBox 支持刷新、缓存、失败回退手动输入。

阶段 3：发送覆盖与 reasoning 映射

- 前端 `SendMessageRuntimeOptions` 增加 `modelName`、`thinkingLevel`。
- `sendChatCommand` 把覆盖值写进 `ai_model_config`。
- 后端 `merge_safe_request_overrides` 允许 `model_name/thinking_level`。
- 替换 `normalize_thinking_level` 的 gpt-only 逻辑为 provider adapter。

阶段 4：SDK gateway

- 新增内部 `llm-provider-gateway`。
- 先只接管 `/models`，再接管 Chat Completions，最后接管 Responses。
- Rust 保留工具循环和消息持久化，但上游 provider 访问统一交给 SDK gateway。

阶段 5：清理旧模型配置语义

- 将“AI 模型管理”文案改成“模型供应商管理”或“AI 供应商连接”。
- `model` 字段文案改为“默认模型”，并允许为空；如果为空，输入栏必须选模型。
- `kimik2` provider key 迁移为 `kimi`，保留读取兼容。

## 测试计划

- Rust 单测：
  - provider alias：`openai -> gpt`、`kimik2 -> kimi`
  - DeepSeek/Kimi/GPT thinking 映射
  - `merge_safe_request_overrides` 只允许安全字段
  - 无权访问 `model_config_id` 时不能通过覆盖字段绕过权限
- 前端单测：
  - 输入栏模型 ComboBox 可选择 provider 下模型
  - `/models` 失败时可手动输入
  - 不同 provider 显示不同 thinking 控件
  - 远程选择器文案改名后仍传 `remoteConnectionId`
- 集成测试：
  - mock OpenAI/Kimi/DeepSeek `/models` 响应
  - mock Chat Completions streaming 中的 `reasoning_content`
  - GPT Responses reasoning summary 仍能显示
  - 会话切换后恢复上次 provider/model/thinking

## 风险

- OpenAI-compatible 不代表完全兼容。尤其是 reasoning、tool calls、streaming delta 字段，各 provider 都有差异。
- `/models` 不一定返回完整能力字段。OpenAI 和 DeepSeek 基本只给 id/owned_by，Kimi 会给更多能力字段，所以 capability 需要 provider preset + 用户手动覆盖。
- 直接把前端 `openai` npm 包用于 provider 请求不合适，因为 API Key 会暴露在浏览器。SDK 必须在服务端使用。
- 一次性把主聊天流从 Rust `reqwest` 迁到 SDK gateway 风险较高，应先灰度模型列表，再灰度生成。

## 我建议的最终形态

- 输入栏负责“本会话/本次使用哪个 provider 配置、哪个模型、什么思考模式”。
- AI 供应商管理负责“API Key、Base URL、默认模型、能力开关、模型列表缓存”。
- 远程服务器选择继续存在，但定位为“工具运行上下文”，不再叫“服务器”。
- 上游 LLM 调用逐步迁移到服务端内部 OpenAI SDK gateway，DeepSeek/Kimi/GPT 统一通过 OpenAI SDK 初始化不同 `base_url` 调用。
