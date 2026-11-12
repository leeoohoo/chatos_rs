# 模型请求预算的官方依据

核验日期：2026-11-12。用户要求查询 OpenAI、DeepSeek、Kimi、GLM 官方规格，使用当前支持型号的共同可用能力，并从可选模型移除 `gpt-5.3-codex-spark`、`codex-auto-review`。

下表是配置决策的证据，不是程序默认值、初始化种子或自动补值目录。运行时只读取配置中心已正式发布的版本。

| 厂商与适用型号 | 官方窗口 | 官方最大输出 | 官方来源 |
| --- | ---: | ---: | --- |
| OpenAI GPT-5.4-mini | 400,000 | 128,000 | [模型规格](https://developers.openai.com/api/docs/models/gpt-5.4-mini.md)，同时明确最大输入为 272,000 |
| OpenAI GPT-5.4、GPT-5.5 | 1,050,000 | 128,000 | [GPT-5.4](https://developers.openai.com/api/docs/models/gpt-5.4)、[GPT-5.5](https://developers.openai.com/api/docs/models/gpt-5.5.md) |
| OpenAI GPT-5.6-sol/luna/terra、GPT-6-sol/luna、GPT-6.1-sol | 1,050,000 | 128,000 | [GPT-6.1-sol](https://developers.openai.com/api/docs/models/gpt-6.1-sol.md)、[GPT-6-sol](https://developers.openai.com/api/docs/models/gpt-6-sol.md)、[GPT-6-luna](https://developers.openai.com/api/docs/models/gpt-6-luna.md)、[GPT-5.6-sol](https://developers.openai.com/api/docs/models/gpt-5.6-sol.md)、[GPT-5.6-luna](https://developers.openai.com/api/docs/models/gpt-5.6-luna.md)、[GPT-5.6-terra](https://developers.openai.com/api/docs/models/gpt-5.6-terra.md)，最大输入均为 922,000 |
| DeepSeek Flash、DeepSeek V4 Pro | 1M | 384K | [官方定价与规格](https://api-docs.deepseek.com/quick_start/pricing.md)，Flash 当前对应 V4.1-Flash-1017，V4 Pro 对应 V4-Pro-0813 |
| Kimi K3、K3.1 Pro | 1,048,576 | 可配至 1,048,576 | [K3](https://platform.kimi.ai/docs/guide/kimi-k3-quickstart.md)、[K3.1 Pro](https://platform.kimi.ai/docs/guide/kimi-k3-1-pro-quickstart.md)，须同时遵守输入与输出的窗口总和 |
| GLM-5.2、GLM-5.3 | 1M | 128K | [GLM-5.2](https://docs.bigmodel.cn/cn/guide/models/text/glm-5.2.md)、[GLM-5.3](https://docs.bigmodel.cn/cn/guide/models/text/glm-5.3.md) |

本次共同配置取上述能力的交集：窗口 400,000，输出预留 128,000，输入容量 272,000。瓶颈是 GPT-5.4-mini。这里的“共同”仅适用于表中核实的当前型号，不涵盖所有历史型号；新增其他型号前必须核对其规格。旧 GLM-5/4.6、旧 Kimi K2 等不能直接套用。

正式配置键为 `shared.ai.model_capabilities` 和 `shared.ai.active_model_capability_revision`。发布对象包含窗口、输出预留、估算器标识、来源 URL 和排除模型名单；配置中心是唯一数值来源。这是所有任务共用的全局容量，当前账户、所选执行模型及模型切换不会改变它；查看策略和计算派发容量不得依赖用户的执行模型设置。数据库旧副本、Memory 独立策略、Agent 全局预算以及客户端资源默认值全部移除。

完整请求中的系统提示词、工具定义、当前消息和输出预留都会占用模型窗口。补充记忆只能使用扣除这些内容后的可用输入；发送前再次校验完整请求。当前预检使用项目已有的文本/结构估算器，估算并不等于厂商精确计费 token；服务端明确报告超限时，Memory 分块会在正式上限以内缩小并重试。

`gpt-5.3-codex-spark` 的退役信息见 [Codex 官方型号说明](https://developers.openai.com/codex/models.md)；`codex-auto-review` 无可核实的公开规格。两者按用户要求从供应商导入与可选模型列表排除。图片、视频生成模型不依赖聊天 token 预算。
