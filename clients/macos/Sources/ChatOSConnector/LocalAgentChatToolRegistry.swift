import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension LocalAgentChatToolProvider {
    static let toolDefinitions: [AgentToolDefinition] = [
        .init(
            name: bootstrapToolName,
            description: "连接本地 Relay MCP 后读取当前 Agent 身份、绑定项目、团队、成员和本次唤醒消息。身份与范围由客户端固定，不能由参数切换。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: workspaceSnapshotToolName,
            description: "读取当前账户在本机已有的全部活跃 Agent、项目团队、项目临时引用、成员关系和显式项目经理。私聊中的 relay_bootstrap 只描述当前会话。需求调研在私聊中使用这里返回的 project_ref 绑定项目；项目真实 ID 不暴露给模型。非项目经理收到需要任务化或分配成员的请求时，先用本工具判断自己是否属于目标团队并取得显式项目经理的 agent_ref：团队成员走 chat_team_send，非团队成员走 chat_direct_open → chat_direct_send。所有引用仅在同一 Run 内有效。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: getTriggerToolName,
            description: "读取唤醒当前 Agent 的群聊消息。项目、房间和消息身份由运行上下文固定。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: listMembersToolName,
            description: "列出当前项目群聊中的 Agent 成员及职责。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: readUnreadToolName,
            description: "读取当前 Agent 在这个群聊中的未读消息。已读位置按 Agent 独立持久化；读取不会自动确认，处理后调用 chat_mark_read。",
            schema: Data(#"{"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: readAllUnreadToolName,
            description: "读取当前 Agent 在全部群聊和私聊中的未读消息。返回内容即视为已读并自动推进各会话游标；只返回同一 Run 内有效的临时引用，不暴露真实会话、消息或项目 ID，暂停、重启并恢复该 Run 后引用仍可使用。消息不一定需要行动，请自行判断是否回复、忽略或加入 TodoList。",
            schema: Data(#"{"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":500}},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: inboxSendToolName,
            description: "使用 chat_read_all_unread 或 todo_list 返回的临时引用回复原会话。notify_project_manager=true 仅适用于当前 Agent 可访问、且已绑定显式项目经理的项目团队会话；Human-Agent 私聊或 Agent 私聊不能用它通知项目经理。非项目经理在私聊收到任务化请求时，应先调用 agent_workspace_snapshot，再按成员关系使用 chat_team_send 或 chat_direct_open → chat_direct_send。Todo 私聊来源不可访问时也不要重试本工具。",
            schema: Data("""
            {"type":"object","properties":{"conversation_ref":{"type":"string","minLength":1,"maxLength":600},"reply_to_message_ref":{"type":"string","minLength":1,"maxLength":600},"content":{"type":"string","minLength":1,"maxLength":\(AgentCommunicationPolicy.standard.maximumMessageCharacters)},"document_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"maxItems":\(AgentCommunicationPolicy.standard.maximumDocumentsPerMessage),"uniqueItems":true},"notify_project_manager":{"type":"boolean","default":false}},"required":["conversation_ref","reply_to_message_ref","content"],"additionalProperties":false}
            """.utf8),
            effect: .write
        ),
        .init(
            name: readMessagesToolName,
            description: "从最近一页开始，向更早方向分页读取当前会话记录。需要更早消息时，把响应中的 next_before_message_ref 作为 before_message_ref 继续读取。所有引用在同一 Run 内有效，客户端暂停、重启并恢复该 Run 后仍可继续使用。",
            schema: Data(#"{"type":"object","properties":{"before_message_ref":{"type":"string","minLength":1,"maxLength":600},"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: readAttachmentToolName,
            description: "按消息和附件在同一 Run 内有效的临时引用读取当前会话附件。文本可用 offset/limit 分段读取；当前触发消息中的图片或 PDF 已由客户端直接作为多模态输入交给模型。",
            schema: Data(#"{"type":"object","properties":{"message_ref":{"type":"string","minLength":1,"maxLength":600},"attachment_ref":{"type":"string","minLength":1,"maxLength":600},"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":12000}},"required":["message_ref","attachment_ref"],"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: createDocumentToolName,
            description: "创建当前 Run 内的 UTF-8 Markdown 文档草稿。客户端清洗文件名、计算大小和 SHA-256，只返回临时 document_ref；创建后必须在同一 Run 的下一条发送消息中通过 document_refs 附加。",
            schema: Data("""
            {"type":"object","properties":{"name":{"type":"string","minLength":1,"maxLength":512},"title":{"type":"string","minLength":1,"maxLength":512},"markdown":{"type":"string","minLength":1,"maxLength":\(AgentCommunicationPolicy.standard.maximumDocumentBytes)}},"required":["name","title","markdown"],"additionalProperties":false}
            """.utf8),
            effect: .write
        ),
        .init(
            name: markReadToolName,
            description: "把当前 Agent 的独立已读游标推进到指定本轮消息引用。游标单调前进，旧调用或重试不会把已读位置回退。",
            schema: Data(#"{"type":"object","properties":{"through_message_ref":{"type":"string","minLength":1,"maxLength":600}},"required":["through_message_ref"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: openDirectToolName,
            description: "使用 agent_workspace_snapshot 或成员列表返回的临时 Agent 引用打开或复用私聊。非项目经理不属于目标团队、无法 chat_team_send 时，用本工具打开与该团队显式项目经理的私聊，再用 chat_direct_send 转交完整任务简报。不能与自己私聊；同一团队成员之间的项目协作仍优先使用 chat_team_send。",
            schema: Data(#"{"type":"object","properties":{"target_agent_ref":{"type":"string","minLength":1,"maxLength":600}},"required":["target_agent_ref"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: sendDirectToolName,
            description: "向 chat_direct_open 返回的 Agent 私聊发送消息，成功后通过本地 delivery 唤醒对方。非项目经理向目标团队项目经理转交任务时，消息必须包含 Human 原始目标与来源、目标团队、范围、交付物、验收建议和关键 URL；不得声称 Todo 已创建。当前 Agent 必须是该私聊参与者。",
            schema: Data("""
            {"type":"object","properties":{"conversation_ref":{"type":"string","minLength":1,"maxLength":600},"content":{"type":"string","minLength":1,"maxLength":\(AgentCommunicationPolicy.standard.maximumMessageCharacters)},"document_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"maxItems":\(AgentCommunicationPolicy.standard.maximumDocumentsPerMessage),"uniqueItems":true}},"required":["conversation_ref","content"],"additionalProperties":false}
            """.utf8),
            effect: .write
        ),
        .init(
            name: sendTeamToolName,
            description: "向 agent_workspace_snapshot 返回的项目团队主动发送群消息并精确 @ 成员。当前 Agent 必须是该团队活跃成员。非项目经理属于目标团队且需要项目经理任务化新增工作时，用本工具发送完整任务简报，并在 mention_agent_refs 中传该团队显式项目经理的 agent_ref；若当前 Agent 不属于该团队，改用 chat_direct_open → chat_direct_send。",
            schema: Data("""
            {"type":"object","properties":{"team_ref":{"type":"string","minLength":1,"maxLength":600},"content":{"type":"string","minLength":1,"maxLength":\(AgentCommunicationPolicy.standard.maximumMessageCharacters)},"document_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"maxItems":\(AgentCommunicationPolicy.standard.maximumDocumentsPerMessage),"uniqueItems":true},"mention_agent_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"maxItems":64,"uniqueItems":true}},"required":["team_ref","content"],"additionalProperties":false}
            """.utf8),
            effect: .write
        ),
        .init(
            name: Self.proposeMemberToolName,
            description: "使用已授予的人员管理权限，向 Human 提交一个新 Agent 草案。该工具只持久化待确认提案，绝不会直接创建 Agent；私聊中确认后只创建独立 Agent，团队会话中确认后才加入当前团队。模型配置由客户端继承并透传，AI 不填写模型 ID；thinking_level 省略时继承当前 Agent。",
            schema: Data(#"{"type":"object","properties":{"name":{"type":"string","minLength":1,"maxLength":120},"role":{"type":"string","minLength":1,"maxLength":160},"responsibility":{"type":"string","maxLength":8000},"role_prompt":{"type":"string","minLength":1,"maxLength":32000},"thinking_level":{"type":"string","enum":["auto","none","minimal","low","medium","high","xhigh","max"]},"rationale":{"type":"string","maxLength":4000}},"required":["name","role","role_prompt"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: proposeExistingMemberToolName,
            description: "使用人员管理权限，把账户中已有 Agent 邀请进指定项目团队。先调用 agent_workspace_snapshot，使用其中同一轮返回的 team_ref 和 agent_ref；真实 ID 由客户端解析，不得猜测。该工具只生成待确认提案，Human 确认后才建立成员关系；一个 Agent 可以加入多个团队。",
            schema: Data(#"{"type":"object","properties":{"team_ref":{"type":"string","minLength":1,"maxLength":600},"target_agent_ref":{"type":"string","minLength":1,"maxLength":600},"role":{"type":"string","minLength":1,"maxLength":160},"responsibility":{"type":"string","maxLength":8000}},"required":["team_ref","target_agent_ref","role"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: proposeMemberRemovalToolName,
            description: "使用已授予的人员管理权限，向 Human 提交把一个 Agent 移出当前项目团队的提案。必须提供事实理由和可选交接计划；该工具不会删除可复用的 Agent profile，也不会绕过 Human 确认。",
            schema: Data(#"{"type":"object","properties":{"target_agent_ref":{"type":"string","minLength":1,"maxLength":600},"reason":{"type":"string","minLength":1,"maxLength":4000},"handoff_plan":{"type":"string","maxLength":8000}},"required":["target_agent_ref","reason"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: sendMessageToolName,
            description: "以当前 Agent 身份回复当前会话。需要 @ 成员或回复指定消息时，只能使用同一 Run 内的成员和消息临时引用；暂停、重启并恢复该 Run 后旧引用仍可使用。发送不会结束通讯周期，仍需检查未读和任务调度并调用 agent_cycle_complete。",
            schema: Data("""
            {"type":"object","properties":{"content":{"type":"string","minLength":1,"maxLength":\(AgentCommunicationPolicy.standard.maximumMessageCharacters)},"document_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"maxItems":\(AgentCommunicationPolicy.standard.maximumDocumentsPerMessage),"uniqueItems":true},"mention_agent_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"maxItems":32,"uniqueItems":true},"reply_to_message_ref":{"type":"string","minLength":1,"maxLength":600}},"required":["content"],"additionalProperties":false}
            """.utf8),
            effect: .write
        ),
        .init(
            name: completeHeartbeatToolName,
            description: "仅用于主动巡检：当前会话没有需要汇报或执行的事项时，安静完成本次巡检，不向聊天记录发送消息。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: completeManagerCycleToolName,
            description: "结束一次消息、主动巡检或 Todo 状态唤醒的通讯周期。调用前必须完成必要回复和任务调整，并确认有执行中任务、已调用 todo_start_next，或当前没有 ready 任务。不会向聊天记录写入内部状态消息。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: todoListToolName,
            description: "读取当前 Agent 所属项目团队的共享任务板，并标明团队、负责人、来源、依赖和是否分配给自己。来源只表示项目经理创建任务时引用的会话，并不保证当前负责人是该来源私聊的参与者；chat_inbox_send 若返回 source_conversation_not_accessible，应改用 agent_workspace_snapshot 与 chat_team_send 向所属团队公开汇报。普通成员只能查看；只有团队明确指定的项目经理可以修改。默认不返回已完成或已取消任务。",
            schema: Data(#"{"type":"object","properties":{"include_terminal":{"type":"boolean","default":false}},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: todoScheduleStateToolName,
            description: "读取当前 Agent 的任务调度状态：是否已有执行中的 Todo，以及自己最高优先级且所有前置均已完成的 ready Todo。真实 ID 不会返回。每个通讯周期结束前应调用。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: todoStartNextToolName,
            description: "由当前 Agent 的通讯线程原子启动自己最高优先级的 ready Todo。若已有 executor 则返回 executor_busy；没有 ready Todo 则返回 no_ready_todo。模型不能指定 Todo ID，因此不能绕过优先级和前置校验。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: todoExecutionOptionsToolName,
            description: "仅供项目经理读取自己管理的团队、可分配成员、基础能力和本机 Plugin 临时选项。Human 要求‘建立/创建任务’、‘找个人/分配成员’，或要求下载、克隆、查看、运行、分析 GitHub/GitLab 等远程仓库时，必须先调用本工具，再用 todo_add 创建团队 Todo；不要调用 team_propose_*。真实团队、项目、Agent 和 Plugin ID 不会返回。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: todoDependencyOptionsToolName,
            description: "仅供项目经理读取某个团队可作为前置任务的 Todo 临时引用。创建或更新依赖前调用；只能建立同团队依赖，客户端会拒绝自依赖、重复和环。",
            schema: Data(#"{"type":"object","properties":{"team_ref":{"type":"string","minLength":1,"maxLength":600}},"required":["team_ref"],"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: todoAddToolName,
            description: "仅供项目经理在共享团队任务板创建 Todo。适用于 Human 要求建立任务、找成员执行，以及下载/克隆/查看/运行/分析远程 Git 仓库；仓库 URL 应原样写入 objective、scope 或 detail，需要 git clone 或命令行时在 builtin_capabilities 选择 terminal。此工具不创建 ChatOS 项目或团队，不得因正文出现‘项目’或 Git URL 而改用 team_propose_*。必须明确目标、范围、交付物、验收条件和约束，并选择负责人、前置任务及可信执行能力。team_ref/assignee_ref/plugin_ref 必须来自 todo_execution_options，source_message_refs 必须来自 chat_get_trigger、relay_bootstrap、chat_read_unread、chat_read_all_unread 或 chat_read_messages；真实 ID 由客户端解析和校验。",
            schema: Data(#"{"type":"object","properties":{"title":{"type":"string","minLength":1,"maxLength":500},"detail":{"type":"string","maxLength":16000},"objective":{"type":"string","minLength":1,"maxLength":8000},"scope":{"type":"string","minLength":1,"maxLength":16000},"expected_outputs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":4000},"minItems":1,"maxItems":64},"acceptance_criteria":{"type":"array","items":{"type":"string","minLength":1,"maxLength":4000},"minItems":1,"maxItems":64},"constraints":{"type":"array","items":{"type":"string","minLength":1,"maxLength":4000},"maxItems":64},"priority":{"type":"integer","minimum":0,"maximum":100,"default":50},"team_ref":{"type":"string","minLength":1,"maxLength":600},"assignee_ref":{"type":"string","minLength":1,"maxLength":600},"depends_on_todo_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"maxItems":64,"uniqueItems":true},"source_message_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"minItems":1,"maxItems":64,"uniqueItems":true},"requires_execution":{"type":"boolean","default":true},"builtin_capabilities":{"type":"array","description":"任务级基础工具。选择 requirement_survey_write 时客户端会强制同时加入 requirement_survey_read。","items":{"type":"string","enum":["project_read","project_write","terminal","requirement_survey_read","requirement_survey_write"]},"maxItems":5,"uniqueItems":true},"plugin_hints":{"type":"array","items":{"type":"object","properties":{"plugin_ref":{"type":"string","minLength":1,"maxLength":600},"reason":{"type":"string","maxLength":1000}},"required":["plugin_ref"],"additionalProperties":false},"maxItems":32}},"required":["title","objective","scope","expected_outputs","acceptance_criteria","team_ref","assignee_ref","source_message_refs","requires_execution","builtin_capabilities"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: todoUpdateToolName,
            description: "仅供项目经理更新团队 Todo 的标题、说明、优先级、前置任务，或将普通阻塞任务重开、取消过时任务。若任务因写入或计费步骤结果不明而进入 needsReview，只有 Human 能在运行详情中点击“重试中断步骤”，本工具会明确返回 human_retry_required，Agent 不得改回 pending。这里管理的是任务结构和调度状态；负责人即使不是项目经理，仍在独立执行线程中通过 todo_progress_append、todo_block、todo_complete 记录过程并修改自己任务的执行状态。",
            schema: Data(#"{"type":"object","properties":{"todo_ref":{"type":"string","minLength":1,"maxLength":600},"title":{"type":"string","minLength":1,"maxLength":500},"detail":{"type":"string","maxLength":16000},"objective":{"type":"string","minLength":1,"maxLength":8000},"scope":{"type":"string","minLength":1,"maxLength":16000},"expected_outputs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":4000},"minItems":1,"maxItems":64},"acceptance_criteria":{"type":"array","items":{"type":"string","minLength":1,"maxLength":4000},"minItems":1,"maxItems":64},"constraints":{"type":"array","items":{"type":"string","minLength":1,"maxLength":4000},"maxItems":64},"priority":{"type":"integer","minimum":0,"maximum":100},"status":{"type":"string","enum":["pending","cancelled"]},"depends_on_todo_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"maxItems":64,"uniqueItems":true},"source_message_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"maxItems":64,"uniqueItems":true}},"required":["todo_ref"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: todoReorderToolName,
            description: "仅供项目经理显式调整同一个团队任务板中未完成 Todo 的优先顺序。数组中靠前的任务优先；未完成前置任务仍不会被调度。",
            schema: Data(#"{"type":"object","properties":{"todo_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"minItems":1,"maxItems":500,"uniqueItems":true}},"required":["todo_refs"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: todoReadProgressToolName,
            description: "通讯线程读取一个 Todo 执行线程持续写入的阶段、动作、结果或阻塞记录。使用 todo_list 返回的临时 todo_ref。",
            schema: Data(#"{"type":"object","properties":{"todo_ref":{"type":"string","minLength":1,"maxLength":600},"limit":{"type":"integer","minimum":1,"maximum":500,"default":100}},"required":["todo_ref"],"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: teamAssetListToolName,
            description: "列出项目团队的共享资产目录，并返回本轮临时 asset_ref。Todo 执行线程固定读取当前任务所属团队；通讯线程在私聊中先用 agent_workspace_snapshot 获取 team_ref。",
            schema: Data(#"{"type":"object","properties":{"team_ref":{"type":"string","minLength":1,"maxLength":600}},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: teamAssetGetToolName,
            description: "读取 team_asset_list 返回的某一版团队共享资产 Markdown。引用包含 revision，资产更新后必须重新列出，禁止猜测真实资产 ID。",
            schema: Data(#"{"type":"object","properties":{"asset_ref":{"type":"string","minLength":1,"maxLength":600}},"required":["asset_ref"],"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: teamAssetCreateToolName,
            description: "仅供目标团队明确指定的项目经理首次创建一项共享资产。调用前先用 team_asset_list 确认没有同类资产，并用 agent_workspace_snapshot 取得本轮 team_ref。创建时只提供 team_ref、category、title、markdown；绝对不要提供 asset_ref、new、create 或 revision。适用于空资产目录，也适用于新增另一项独立资产。成功后返回当前 Run 的 asset_ref 和 revision。项目概览与当前进度只能依据真实 Human 消息、团队目标、Todo 和已核验结果创建，不得写空模板或臆测。",
            schema: Data(#"{"type":"object","properties":{"team_ref":{"type":"string","minLength":1,"maxLength":600},"category":{"type":"string","enum":["overview","current_progress","tech_stack","architecture","conventions","decision","reference"]},"title":{"type":"string","minLength":1,"maxLength":240},"markdown":{"type":"string","minLength":1,"maxLength":128000}},"required":["team_ref","category","title","markdown"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: teamAssetUpdateToolName,
            description: "仅供目标团队明确指定的项目经理更新已有共享资产。必须先调用 team_asset_list 取得当前 Run 的 asset_ref，并用 team_asset_get 读取现有正文后再提交完整合并结果。只提供 asset_ref、category、title、markdown；不要提供 team_ref 或 expected_revision，工具会从 asset_ref 中读取并校验当前 revision。若引用过期，重新 list/get 后再更新。此工具不能用于空目录首次创建；没有可用 asset_ref 时改用 team_asset_create。",
            schema: Data(#"{"type":"object","properties":{"asset_ref":{"type":"string","minLength":1,"maxLength":600},"category":{"type":"string","enum":["overview","current_progress","tech_stack","architecture","conventions","decision","reference"]},"title":{"type":"string","minLength":1,"maxLength":240},"markdown":{"type":"string","minLength":1,"maxLength":128000}},"required":["asset_ref","category","title","markdown"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: teamAssetArchiveToolName,
            description: "仅供团队明确指定的项目经理归档共享资产。只能使用 team_asset_list 返回的当前 asset_ref，客户端按 revision 防止覆盖并发修改。",
            schema: Data(#"{"type":"object","properties":{"asset_ref":{"type":"string","minLength":1,"maxLength":600}},"required":["asset_ref"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: projectDashboardGetToolName,
            description: "读取项目总览、里程碑、项目经理登记的问题，以及团队 Todo 的实时状态。通讯线程在团队群可省略 team_ref；私聊中先用 agent_workspace_snapshot 取得 team_ref。返回的事实由程序生成，项目经理更新看板前必须先调用本工具。",
            schema: Data(#"{"type":"object","properties":{"team_ref":{"type":"string","minLength":1,"maxLength":600}} ,"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: projectDashboardUpdateToolName,
            description: "仅供目标团队明确指定的项目经理更新结构化项目总览。必须先调用 project_dashboard_get，并把当前 revision 作为 expected_revision；首次创建时省略 expected_revision。Todo 只能使用同一轮返回的 todo_ref。进度和健康判断必须基于真实任务、Run、调研或 Human 消息，不得用总结文字冒充已验收交付。",
            schema: Data(#"{"type":"object","properties":{"team_ref":{"type":"string","minLength":1,"maxLength":600},"expected_revision":{"type":"integer","minimum":1},"phase":{"type":"string","minLength":1,"maxLength":240},"health":{"type":"string","enum":["on_track","at_risk","blocked","completed"]},"summary":{"type":"string","minLength":1,"maxLength":16000},"next_steps":{"type":"array","items":{"type":"string","minLength":1,"maxLength":2000},"maxItems":32},"milestones":{"type":"array","maxItems":64,"items":{"type":"object","properties":{"id":{"type":"string","minLength":1,"maxLength":512},"title":{"type":"string","minLength":1,"maxLength":240},"detail":{"type":"string","maxLength":8000},"status":{"type":"string","enum":["pending","in_progress","blocked","completed"]},"progress_percent":{"type":"integer","minimum":0,"maximum":100},"acceptance_criteria":{"type":"array","items":{"type":"string","minLength":1,"maxLength":2000},"maxItems":32},"todo_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"maxItems":128,"uniqueItems":true},"target_at_unix_ms":{"type":"integer","minimum":0}},"required":["id","title","status","progress_percent"],"additionalProperties":false}},"issues":{"type":"array","maxItems":64,"items":{"type":"object","properties":{"id":{"type":"string","minLength":1,"maxLength":512},"title":{"type":"string","minLength":1,"maxLength":240},"detail":{"type":"string","maxLength":8000},"requested_action":{"type":"string","minLength":1,"maxLength":4000},"severity":{"type":"string","enum":["info","warning","critical"]},"owner":{"type":"string","enum":["human","agent","external"]},"todo_ref":{"type":"string","minLength":1,"maxLength":600}},"required":["id","title","requested_action","severity","owner"],"additionalProperties":false}}},"required":["team_ref","phase","health","summary"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: agentSkillActivateToolName,
            description: "激活系统提示或产品工具目录返回的当前 Run Skill，返回完整主说明和按需参考目录。只能使用当次返回的 skill_ref；身份 Skill 不能切换，产品 Skill 不能扩大权限。 Activate one run-scoped identity or product Skill returned by its Router; unlisted Skills are rejected.",
            schema: Data(#"{"type":"object","properties":{"skill_ref":{"type":"string","minLength":1,"maxLength":240}},"required":["skill_ref"],"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: agentSkillListResourcesToolName,
            description: "列出已激活身份或产品 Skill 的详细参考资料；必须先调用 agent_skill_activate。 List detailed references for an activated run-scoped Skill.",
            schema: Data(#"{"type":"object","properties":{"skill_ref":{"type":"string","minLength":1,"maxLength":240}},"required":["skill_ref"],"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: agentSkillReadResourceToolName,
            description: "分页读取已激活身份或产品 Skill 的一项参考资料，只在当前决策需要时读取。 Read one activated Skill reference on demand with character pagination.",
            schema: Data(#"{"type":"object","properties":{"skill_ref":{"type":"string","minLength":1,"maxLength":240},"relative_path":{"type":"string","minLength":1,"maxLength":1000},"offset":{"type":"integer","minimum":0},"max_chars":{"type":"integer","minimum":1,"maximum":64000}},"required":["skill_ref","relative_path"],"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: todoGetContextToolName,
            description: "仅用于 Todo 执行线程：读取当前 delivery 绑定的任务、可信能力计划和来源消息，不接受任何 ID 参数。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: todoProgressAppendToolName,
            description: "仅用于 Todo 执行线程：当前任务负责人记录阶段、已执行动作、观察结果和下一步，使通讯线程可以随时查看进度；不要求负责人是项目经理。",
            schema: Data(#"{"type":"object","properties":{"stage":{"type":"string","maxLength":240},"detail":{"type":"string","minLength":1,"maxLength":16000}},"required":["detail"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: todoCompleteToolName,
            description: "仅用于 Todo 执行线程：当前任务负责人保存完成总结，把自己负责的任务置为 completed 并结束执行；不要求负责人是项目经理。发现应长期保留的项目背景、进度、技术栈、架构、规范、决策或参考资料时，通过 asset_update_suggestions 提交完整 Markdown 和事实理由。建议只进入完成事件，由项目经理审核后才能写入团队共享资产；不要为没有持久价值的过程信息提交建议。",
            schema: Data(#"{"type":"object","properties":{"summary":{"type":"string","minLength":1,"maxLength":16000},"asset_update_suggestions":{"type":"array","maxItems":8,"items":{"type":"object","properties":{"category":{"type":"string","enum":["overview","current_progress","tech_stack","architecture","conventions","decision","reference"]},"title":{"type":"string","minLength":1,"maxLength":240},"markdown":{"type":"string","minLength":1,"maxLength":128000},"rationale":{"type":"string","minLength":1,"maxLength":4000}},"required":["category","title","markdown","rationale"],"additionalProperties":false}}},"required":["summary"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: todoBlockToolName,
            description: "仅用于 Todo 执行线程：当前任务负责人保存阻塞原因和执行现场，把自己负责的任务置为 blocked 并结束执行；不要求负责人是项目经理。随后由通讯线程向来源会话沟通。",
            schema: Data(#"{"type":"object","properties":{"reason":{"type":"string","minLength":1,"maxLength":8000}},"required":["reason"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
    ]
}
