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
            description: "读取当前账户在本机已有的全部活跃 Agent、项目团队、成员关系和显式项目经理。私聊中的 relay_bootstrap 只描述当前会话，不能据此判断其他团队或 Agent 不存在；回答组织现状、既有团队、成员或人员缺口前必须调用本工具。仅返回本轮临时引用，不暴露真实 Agent、团队或项目 ID。",
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
            description: "读取当前 Agent 在全部群聊和私聊中的未读消息。返回内容即视为已读并自动推进各会话游标；只返回本轮临时引用，不暴露真实会话、消息或项目 ID。消息不一定需要行动，请自行判断是否回复、忽略或加入 TodoList。",
            schema: Data(#"{"type":"object","properties":{"limit":{"type":"integer","minimum":1,"maximum":500}},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: inboxSendToolName,
            description: "使用 chat_read_all_unread 本轮返回的临时引用回复原群聊或私聊。普通成员需要把新增工作交给项目经理任务化时，在项目团队会话设置 notify_project_manager=true，由客户端解析并唤醒该团队明确绑定的项目经理。",
            schema: Data("""
            {"type":"object","properties":{"conversation_ref":{"type":"string","minLength":1,"maxLength":600},"reply_to_message_ref":{"type":"string","minLength":1,"maxLength":600},"content":{"type":"string","minLength":1,"maxLength":\(AgentCommunicationPolicy.standard.maximumMessageCharacters)},"document_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"maxItems":\(AgentCommunicationPolicy.standard.maximumDocumentsPerMessage),"uniqueItems":true},"notify_project_manager":{"type":"boolean","default":false}},"required":["conversation_ref","reply_to_message_ref","content"],"additionalProperties":false}
            """.utf8),
            effect: .write
        ),
        .init(
            name: readMessagesToolName,
            description: "从最近一页开始，向更早方向分页读取当前会话记录。需要更早消息时，把响应中的 next_before_message_ref 作为 before_message_ref 继续读取。所有引用只在本轮有效。",
            schema: Data(#"{"type":"object","properties":{"before_message_ref":{"type":"string","minLength":1,"maxLength":600},"limit":{"type":"integer","minimum":1,"maximum":100}},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: readAttachmentToolName,
            description: "按消息和附件的本轮临时引用读取当前会话附件。文本可用 offset/limit 分段读取；当前触发消息中的图片或 PDF 已由客户端直接作为多模态输入交给模型。",
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
            description: "使用 agent_workspace_snapshot 或成员列表返回的临时 Agent 引用打开或复用私聊。不能与自己私聊；A 到 B 和 B 到 A 会得到同一个 conversation_ref。私聊用于一对一补充、敏感事项或非共同团队协作；同一项目团队的启动、分工、依赖、进度、阻塞和交付应优先使用 chat_team_send 在团队群内沟通。",
            schema: Data(#"{"type":"object","properties":{"target_agent_ref":{"type":"string","minLength":1,"maxLength":600}},"required":["target_agent_ref"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: sendDirectToolName,
            description: "向已经打开的 Agent 私聊发送消息。当前 Agent 必须是该私聊参与者，成功后会通过本地 delivery 唤醒对方。不得用多个私聊替代同一项目团队本应公开的协作；项目协作默认使用 chat_team_send。",
            schema: Data("""
            {"type":"object","properties":{"conversation_ref":{"type":"string","minLength":1,"maxLength":600},"content":{"type":"string","minLength":1,"maxLength":\(AgentCommunicationPolicy.standard.maximumMessageCharacters)},"document_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"maxItems":\(AgentCommunicationPolicy.standard.maximumDocumentsPerMessage),"uniqueItems":true}},"required":["conversation_ref","content"],"additionalProperties":false}
            """.utf8),
            effect: .write
        ),
        .init(
            name: sendTeamToolName,
            description: "向 agent_workspace_snapshot 返回的项目团队主动发送一条新群消息，可用同一快照中的 Agent 临时引用精确 @ 团队成员并通过本地 delivery 唤醒他们。当前 Agent 必须是该团队活跃成员，被 @ 的 Agent 也必须属于该团队。项目启动、分工、依赖、进度、阻塞、决策和交付默认使用本工具公开协作；无需唤醒成员的状态同步可不传 mention_agent_refs。",
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
            description: "以当前 Agent 身份回复当前会话。需要 @ 成员或回复指定消息时，只能使用本轮成员和消息临时引用；发送不会结束通讯周期，仍需检查未读和任务调度并调用 agent_cycle_complete。",
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
            description: "读取当前 Agent 所属项目团队的共享任务板，并标明团队、负责人、依赖和是否分配给自己。普通成员只能查看；只有团队明确指定的项目经理可以修改。默认不返回已完成或已取消任务。",
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
            description: "仅供项目经理读取自己管理的团队、可分配成员、基础能力和本机 Plugin 临时选项。创建 Todo 前必须调用；真实团队、项目、Agent 和 Plugin ID 不会返回。",
            schema: Data(#"{"type":"object","properties":{},"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: todoDependencyOptionsToolName,
            description: "仅供项目经理读取某个团队可作为前置任务的 Todo 临时引用。创建或更新依赖前调用；只能建立同团队依赖，客户端会拒绝自依赖、重复和环。",
            schema: Data(#"{"type":"object","properties":{"team_ref":{"type":"string","minLength":1,"maxLength":600}},"required":["team_ref"],"additionalProperties":false}"#.utf8)
        ),
        .init(
            name: todoAddToolName,
            description: "仅供项目经理在共享团队任务板创建 Todo。必须明确目标、范围、交付物、验收条件和约束，并选择负责人、前置任务及可信执行能力。team_ref/assignee_ref/plugin_ref 必须来自 todo_execution_options，真实 ID 由客户端解析和校验。",
            schema: Data(#"{"type":"object","properties":{"title":{"type":"string","minLength":1,"maxLength":500},"detail":{"type":"string","maxLength":16000},"objective":{"type":"string","minLength":1,"maxLength":8000},"scope":{"type":"string","minLength":1,"maxLength":16000},"expected_outputs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":4000},"minItems":1,"maxItems":64},"acceptance_criteria":{"type":"array","items":{"type":"string","minLength":1,"maxLength":4000},"minItems":1,"maxItems":64},"constraints":{"type":"array","items":{"type":"string","minLength":1,"maxLength":4000},"maxItems":64},"priority":{"type":"integer","minimum":0,"maximum":100,"default":50},"team_ref":{"type":"string","minLength":1,"maxLength":600},"assignee_ref":{"type":"string","minLength":1,"maxLength":600},"depends_on_todo_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"maxItems":64,"uniqueItems":true},"source_message_refs":{"type":"array","items":{"type":"string","minLength":1,"maxLength":600},"minItems":1,"maxItems":64,"uniqueItems":true},"requires_execution":{"type":"boolean","default":true},"builtin_capabilities":{"type":"array","items":{"type":"string","enum":["project_read","project_write","terminal"]},"maxItems":3,"uniqueItems":true},"plugin_hints":{"type":"array","items":{"type":"object","properties":{"plugin_ref":{"type":"string","minLength":1,"maxLength":600},"reason":{"type":"string","maxLength":1000}},"required":["plugin_ref"],"additionalProperties":false},"maxItems":32}},"required":["title","objective","scope","expected_outputs","acceptance_criteria","team_ref","assignee_ref","source_message_refs","requires_execution","builtin_capabilities"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: todoUpdateToolName,
            description: "仅供项目经理更新团队 Todo 的标题、说明、优先级、前置任务，或将阻塞任务重开、取消过时任务。这里管理的是任务结构和调度状态；负责人即使不是项目经理，仍在独立执行线程中通过 todo_progress_append、todo_block、todo_complete 记录过程并修改自己任务的执行状态。",
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
            name: teamAssetUpsertToolName,
            description: "仅供团队明确指定的项目经理创建或更新团队共享资产。新建时提供 team_ref；更新时提供 asset_ref 和 expected_revision。Markdown 应维护项目背景、进度、技术栈、架构、规范、决策或参考资料。",
            schema: Data(#"{"type":"object","properties":{"team_ref":{"type":"string","minLength":1,"maxLength":600},"asset_ref":{"type":"string","minLength":1,"maxLength":600},"category":{"type":"string","enum":["overview","current_progress","tech_stack","architecture","conventions","decision","reference"]},"title":{"type":"string","minLength":1,"maxLength":240},"markdown":{"type":"string","minLength":1,"maxLength":128000},"expected_revision":{"type":"integer","minimum":1}},"required":["category","title","markdown"],"additionalProperties":false}"#.utf8),
            effect: .write
        ),
        .init(
            name: teamAssetArchiveToolName,
            description: "仅供团队明确指定的项目经理归档共享资产。只能使用 team_asset_list 返回的当前 asset_ref，客户端按 revision 防止覆盖并发修改。",
            schema: Data(#"{"type":"object","properties":{"asset_ref":{"type":"string","minLength":1,"maxLength":600}},"required":["asset_ref"],"additionalProperties":false}"#.utf8),
            effect: .write
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
            description: "仅用于 Todo 执行线程：当前任务负责人保存完成总结，把自己负责的任务置为 completed 并结束执行；不要求负责人是项目经理。客户端同时记录完成事件。",
            schema: Data(#"{"type":"object","properties":{"summary":{"type":"string","minLength":1,"maxLength":16000}},"required":["summary"],"additionalProperties":false}"#.utf8),
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
