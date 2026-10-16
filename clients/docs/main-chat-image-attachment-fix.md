# 主聊天与 Task 图片附件修复（2026-10-14）

## 根因与范围

真实失败请求为“我用普通话 我要怎么回复这个邮件”。主聊天与 Task 的模型配置均为 `gpt-6-luna`、`supports_images=true`，但最终记录的模型请求均没有图片内容块。

1. 主聊天只收到附件清单，被告知内容需本地工具读取，但没有附件读取工具，因此主动创建了 Task；不是后端无条件自动创建。
2. Task 正确继承了附件。`local_attachment_read(delivery=workspace)` 校验并导入完整 PNG，却只返回路径；文件路径不是模型图片输入。Task 随后未获得视觉输入，报告 blocked。

本次修复 macOS 原生附件工具及共享 Host 的输入指导、回归测试。不改 Windows 原生实现，不重试历史任务，不发送邮件，不修改真实业务文件或受完整性保护的插件安装目录。

## 修复契约

- 保持八个 Task 编排业务工具不变。主聊天新增同名只读附件伴随能力 `local_attachment_read`，与 Task 复用授权及校验链路。
- 主聊天支持 `delivery=model|content`，默认 model；不允许 workspace。加载截图不需要 Task、项目或电脑操作插件。
- Task 支持 `delivery=workspace|content|model`，默认 workspace 保持旧调用兼容。workspace 导入图片时同时提供图片输入；model 不要求执行目录。
- model 与 workspace 使用完整 SHA256 校验后的原文件，不受文本分页 offset/limit 截断。
- 原生工具对图片进行真实解码，小型支持格式保持原始内容；较大图片执行方向校正、缩放或 JPEG 转码。源文件上限 20 MiB、64M 像素；模型图片上限 2 MiB，低于 4 MiB IPC 单帧上限。不能处理时返回具体错误，不假装已识图。
- 图片经 `_model_input` 转为共享运行时的 transient `input_image`，文字结果和结构化 Memory 只保留元数据；原始二进制仍在本地授权附件存储中。模型请求/checkpoint 中的视觉输入用于正常执行与续跑，不应被混入正文摘要。
- 显式 `supports_images=false` 的模型不发送图片块，获得具体的能力限制指导；不能通过盲目建 Task 绕过。
- 主聊天自动加载的 policy、按需编排 Skill 与附件清单指导一致：截图解释、翻译、短邮件拟稿和润色默认在主聊天完成；实际文件修改、外部执行、长时工作及独立跟踪交付物考虑 Task。默认不覆盖用户明确的后台任务要求，简单内容也可按明确要求建立 Task。
- 在线 bootstrap 和离线恢复都发布新不可变能力 revision，更新附件 schema；旧 Run 的冻结快照不改写。迁移去重且幂等，保留项目、插件、模型及外部 MCP 选择。伴随 schema 纳入 checksum，不能复用过期 revision。

## 验证与 Windows 对接

原生测试验证直接主聊天读取、Task workspace/model、完整图片字节、账户隔离、伪图片拒绝、大图处理、文字/结构化记录不含 base64，以及在线/离线 schema 发布。共享 Host 测试验证图片进入模型输入、后续步骤/重试保留、非视觉模型限制；回环 HTTP 测试捕获实际 Responses 请求，不只检查元数据。回环服务没有真实模型语义能力，不能冒充视觉效果评估。

`scripts/verify-repository.sh native-platform` 的 macOS 入口运行 `chatos_agent_profiles` 回归与原生 Swift 测试。手工端到端 QA 使用 `clients/macos/Tests/Integration/attachment-email-fixture.swift` 生成虚构邮件截图，普通自然请求拟稿，检查无 create_task、实际图片输入和草稿中的“蓝鲸-482／周五下午3点／30分钟／B203／最新版界面设计稿”；仅拟稿不发送。

Windows 开发需在原生工具 catalog、executor、恢复迁移及 policy 接入上述 delivery/model 及 `_model_input` 契约。共享附件清单指导要求遵循当前 schema，仅在实际广告 model 模式时使用，避免指示旧 Windows 调用不存在的枚举。不能通过增加 OCR 提示词或只返回文件路径替代多模态传输。

## 当前验收状态

- 共享 Host Profiles 41 项测试通过，含主聊天/Task 与旧 protocol flag 的真实回环 HTTP 图片传输断言。
- 共享 AI/MCP runtime 185+83 项测试通过。
- 原生扩展定向回归：XCTest 144 项通过、1 项可选 Document 集成跳过；Swift Testing 14 项通过。随后启用 Document 集成测试并通过。最终规则修订后再次运行附件/迁移/Skill catalog 定向 41 项（其中同一可选集成跳过），再启用附件 workspace 的完整 10 项，包括实际 Document MCP 读取合成 DOCX，全部通过。
- 传输测试复跑发现 Darwin 接受的 socket 可能继承非阻塞标志；测试服务显式切回阻塞读取，避免请求头尚未到达时发生 WouldBlock。修复后 Profiles 41 项再次通过。
- macOS debug 打包、SDK 及签名验证通过；最终安装版构建时间 `2026-10-14T02:07:11Z`。正常退出后仅原位重装 `/Applications/ChatOS.app`，旧版保存在 `/Applications/.chatos-task-image-fix-backup.7mmM52/ChatOS.app`，可恢复。核对仅一份正式 App 进程与其 Host，原账号、联系人和项目加载正常。
- 未运行全部仓库测试或远端 CI，不把本轮通过结果表述为全部插件业务验收通过。

### 正式安装版真实模型验收

均使用虚构邮件截图、原联系人会话和 `gpt-6-luna`，仅拟稿不发送；只读核对 Run、工具执行、实际请求图片内容块及界面回写。没有重试历史业务任务或修改财务项目文件。

1. 普通提问“我用普通话 我要怎么回复这个邮件”：首次 `run_90d1302f-4577-4493-8de0-2ac5f034970f` 通过；最终安装版复验 `run_e2b9c12c-9a40-4e45-b2eb-99bead40c599` succeeded。只调用一次 `local_attachment_read(delivery=model)`；最终请求有 1 个 `input_image`，该 source Run 下没有 Task。草稿准确包含蓝鲸-482、周五下午3点、30分钟、B203及最新版界面设计稿。
2. 明确要求“请建立一个后台任务，读取这张测试邮件截图的内容，用中文拟一版回复；只拟稿，不发送。”：中间版本 `run_7e9a1dc4-6e8f-4ee5-baf5-74ac7a248850` 忽略任务要求，暴露出附件清单指导与用户意图冲突；修订默认/显式意图区分后，最终版 `run_7848a8e2-0464-4ab0-a177-c344a4560550` 成功 activate → create_task → wait_for_task_completion。
3. 对应 Task `local-task-f97495b0-84fc-4fe1-a9f9-9dfd58160b91-1`、Run `task-run-d386052e-9b3a-4d51-9547-88e2724f9114` succeeded。Task 独立调用附件工具，最终模型请求有 1 个 `input_image`；没有桌面、邮箱或发送工具调用。界面观察到实际执行中的开始回调，完成后更新为“已完成”并可展开完整草稿。草稿准确识别时间、时长、地点、携带设计稿事项；未假设用户能否参加。
