import ChatOSAgentRuntime
import ChatOSCore
import Foundation

extension LocalAgentChatToolProvider {
    struct MemberResponse: Encodable {
        let agentReference: String
        let name: String
        let role: String
        let responsibility: String
        let isProjectManager: Bool

        enum CodingKeys: String, CodingKey {
            case agentReference = "agent_ref"
            case name, role, responsibility
            case isProjectManager = "is_project_manager"
        }
    }

    struct BootstrapResponse: Encodable {
        let agent: MemberResponse
        let conversationReference: String
        let roomName: String
        let roomGoal: String
        let trigger: MessageResponse
        let unread: MessagePageResponse
        let members: [MemberResponse]

        enum CodingKeys: String, CodingKey {
            case agent
            case conversationReference = "conversation_ref"
            case roomName = "room_name"
            case roomGoal = "room_goal"
            case trigger, unread, members
        }
    }

    struct MarkReadResponse: Encodable {
        let throughMessageReference: String
        let hasUnread: Bool
        let nextUnreadMessageReference: String?

        enum CodingKeys: String, CodingKey {
            case throughMessageReference = "through_message_ref"
            case hasUnread = "has_unread"
            case nextUnreadMessageReference = "next_unread_message_ref"
        }
    }

    struct AttachmentResponse: Encodable {
        let attachmentReference: String
        let name: String
        let mimeType: String
        let size: Int
        let kind: String

        enum CodingKeys: String, CodingKey {
            case attachmentReference = "attachment_ref"
            case name
            case mimeType = "mime_type"
            case size, kind
        }
    }

    struct DocumentCreateResponse: Encodable {
        let documentReference: String
        let name: String
        let title: String
        let size: Int
        let mimeType: String
        let sha256: String
        let instruction: String

        enum CodingKeys: String, CodingKey {
            case documentReference = "document_ref"
            case name, title, size
            case mimeType = "mime_type"
            case sha256, instruction
        }
    }

    struct MessageResponse: Encodable {
        let messageReference: String
        let sender: String
        let senderAgentReference: String?
        let content: String
        let replyToMessageReference: String?
        let attachments: [AttachmentResponse]
        let createdAtUnixMs: Int64

        enum CodingKeys: String, CodingKey {
            case messageReference = "message_ref"
            case sender
            case senderAgentReference = "sender_agent_ref"
            case content
            case replyToMessageReference = "reply_to_message_ref"
            case attachments
            case createdAtUnixMs = "created_at_unix_ms"
        }
    }

    struct MessagePageResponse: Encodable {
        let messages: [MessageResponse]
        let nextCursorReference: String?
        let hasMore: Bool
        let readThroughReference: String?

        enum CodingKeys: String, CodingKey {
            case messages
            case nextCursorReference = "next_before_message_ref"
            case hasMore = "has_more"
            case readThroughReference = "read_through_message_ref"
        }
    }

    struct InboxResponse: Encodable {
        let conversations: [InboxConversationResponse]
        let messageCount: Int
        let markedRead: Bool

        enum CodingKeys: String, CodingKey {
            case conversations
            case messageCount = "message_count"
            case markedRead = "marked_read"
        }
    }

    struct InboxConversationResponse: Encodable {
        let conversationReference: String
        let name: String
        let kind: String
        let messages: [InboxMessageResponse]

        enum CodingKeys: String, CodingKey {
            case conversationReference = "conversation_ref"
            case name, kind, messages
        }
    }

    struct InboxMessageResponse: Encodable {
        let messageReference: String
        let sender: String
        let content: String
        let attachments: [AttachmentResponse]
        let createdAtUnixMs: Int64

        enum CodingKeys: String, CodingKey {
            case messageReference = "message_ref"
            case sender, content, attachments
            case createdAtUnixMs = "created_at_unix_ms"
        }
    }

    struct InboxSendResponse: Encodable {
        let sent: Bool
        let notifiedProjectManager: Bool
        let conversationReference: String
        let replyToMessageReference: String
        let spawnedDeliveryCount: Int
        let routingStopReason: String?

        enum CodingKeys: String, CodingKey {
            case sent
            case notifiedProjectManager = "notified_project_manager"
            case conversationReference = "conversation_ref"
            case replyToMessageReference = "reply_to_message_ref"
            case spawnedDeliveryCount = "spawned_delivery_count"
            case routingStopReason = "routing_stop_reason"
        }
    }

    struct WorkspaceSnapshotResponse: Encodable {
        let agents: [WorkspaceAgentResponse]
        let teams: [WorkspaceTeamResponse]
    }

    struct WorkspaceAgentResponse: Encodable {
        let agentReference: String
        let name: String
        let profession: String
        let isCurrentAgent: Bool
        let teams: [String]

        enum CodingKeys: String, CodingKey {
            case agentReference = "agent_ref"
            case name, profession
            case isCurrentAgent = "is_current_agent"
            case teams
        }
    }

    struct WorkspaceTeamResponse: Encodable {
        let teamReference: String
        let projectReference: String
        let name: String
        let goal: String
        let hasProjectManager: Bool
        let projectManager: String?
        let members: [WorkspaceTeamMemberResponse]

        enum CodingKeys: String, CodingKey {
            case teamReference = "team_ref"
            case projectReference = "project_ref"
            case name, goal
            case hasProjectManager = "has_project_manager"
            case projectManager = "project_manager"
            case members
        }
    }

    struct WorkspaceTeamMemberResponse: Encodable {
        let agentReference: String
        let name: String
        let profession: String
        let role: String
        let isProjectManager: Bool

        enum CodingKeys: String, CodingKey {
            case agentReference = "agent_ref"
            case name, profession, role
            case isProjectManager = "is_project_manager"
        }
    }

    struct TodoResponse: Encodable {
        let todoReference: String
        let team: String
        let assignee: String
        let assignedToCurrentAgent: Bool
        let title: String
        let detail: String
        let objective: String
        let scope: String
        let expectedOutputs: [String]
        let acceptanceCriteria: [String]
        let constraints: [String]
        let priority: Int
        let status: String
        let blockedReason: String
        let result: String
        let builtinCapabilities: [String]
        let plugins: [String]
        let sources: [TodoSourceReferenceResponse]
        let dependencies: [TodoDependencyResponse]
        let updatedAtUnixMs: Int64

        enum CodingKeys: String, CodingKey {
            case todoReference = "todo_ref"
            case team, assignee
            case assignedToCurrentAgent = "assigned_to_current_agent"
            case title, detail, objective, scope, constraints, priority, status
            case expectedOutputs = "expected_outputs"
            case acceptanceCriteria = "acceptance_criteria"
            case blockedReason = "blocked_reason"
            case result
            case builtinCapabilities = "builtin_capabilities"
            case plugins
            case sources, dependencies
            case updatedAtUnixMs = "updated_at_unix_ms"
        }
    }

    struct TodoScheduleStateResponse: Encodable {
        let state: String
        let runningTodo: TodoResponse?
        let readyTodo: TodoResponse?

        enum CodingKeys: String, CodingKey {
            case state
            case runningTodo = "running_todo"
            case readyTodo = "ready_todo"
        }
    }

    struct TodoStartNextResponse: Encodable {
        let status: String
        let todo: TodoResponse?
    }

    struct TeamAssetSummaryResponse: Encodable {
        let assetReference: String
        let category: String
        let title: String
        let revision: Int
        let updatedAtUnixMs: Int64

        enum CodingKeys: String, CodingKey {
            case assetReference = "asset_ref"
            case category, title, revision
            case updatedAtUnixMs = "updated_at_unix_ms"
        }
    }

    struct TeamAssetDetailResponse: Encodable {
        let assetReference: String
        let category: String
        let title: String
        let markdown: String
        let revision: Int
        let updatedAtUnixMs: Int64

        enum CodingKeys: String, CodingKey {
            case assetReference = "asset_ref"
            case category, title, markdown, revision
            case updatedAtUnixMs = "updated_at_unix_ms"
        }
    }

    struct RequirementSurveySummaryResponse: Encodable {
        let surveyReference: String
        let title: String
        let purpose: String
        let status: String
        let createdAtUnixMs: Int64
        let submittedAtUnixMs: Int64?

        enum CodingKeys: String, CodingKey {
            case surveyReference = "survey_ref"
            case title, purpose, status
            case createdAtUnixMs = "created_at_unix_ms"
            case submittedAtUnixMs = "submitted_at_unix_ms"
        }
    }

    struct RequirementSurveyDetailResponse: Encodable {
        struct Option: Encodable {
            let key: String
            let label: String
        }

        struct Question: Encodable {
            let key: String
            let prompt: String
            let kind: String
            let required: Bool
            let options: [Option]
            let selectedOptionKeys: [String]
            let selectedOptionLabels: [String]

            enum CodingKeys: String, CodingKey {
                case key, prompt, kind, required, options
                case selectedOptionKeys = "selected_option_keys"
                case selectedOptionLabels = "selected_option_labels"
            }
        }

        struct Resolution: Encodable {
            struct ExecutionStep: Encodable {
                let key: String
                let title: String
                let detail: String
                let owner: String
                let deliverable: String
                let acceptanceCriteria: String

                enum CodingKeys: String, CodingKey {
                    case key, title, detail, owner, deliverable
                    case acceptanceCriteria = "acceptance_criteria"
                }
            }

            let summary: String
            let solutionMarkdown: String
            let executionSteps: [ExecutionStep]
            let risksAndOpenQuestions: String
            let relatedMaterials: String

            enum CodingKeys: String, CodingKey {
                case summary
                case solutionMarkdown = "solution_markdown"
                case executionSteps = "execution_steps"
                case risksAndOpenQuestions = "risks_and_open_questions"
                case relatedMaterials = "related_materials"
            }
        }

        let surveyReference: String
        let title: String
        let purpose: String
        let status: String
        let questions: [Question]
        let notes: String?
        let resolution: Resolution?
        let createdAtUnixMs: Int64
        let submittedAtUnixMs: Int64?
        let resolvedAtUnixMs: Int64?

        enum CodingKeys: String, CodingKey {
            case surveyReference = "survey_ref"
            case title, purpose, status, questions, notes, resolution
            case createdAtUnixMs = "created_at_unix_ms"
            case submittedAtUnixMs = "submitted_at_unix_ms"
            case resolvedAtUnixMs = "resolved_at_unix_ms"
        }
    }

    struct TodoDependencyResponse: Encodable {
        let todoReference: String
        let title: String
        let assignee: String
        let status: String
        let blockedReason: String
        let result: String

        enum CodingKeys: String, CodingKey {
            case todoReference = "todo_ref"
            case title, assignee, status
            case blockedReason = "blocked_reason"
            case result
        }
    }

    struct TodoSourceReferenceResponse: Encodable {
        let conversationReference: String
        let messageReference: String
        let relation: String

        enum CodingKeys: String, CodingKey {
            case conversationReference = "conversation_ref"
            case messageReference = "message_ref"
            case relation
        }
    }

    struct TodoTeamOptionResponse: Encodable {
        let teamReference: String
        let name: String
        let goal: String
        let assignees: [TodoAssigneeOptionResponse]

        enum CodingKeys: String, CodingKey {
            case teamReference = "team_ref"
            case name, goal, assignees
        }
    }

    struct TodoAssigneeOptionResponse: Encodable {
        let assigneeReference: String
        let name: String
        let profession: String
        let role: String
        let isProjectManager: Bool

        enum CodingKeys: String, CodingKey {
            case assigneeReference = "assignee_ref"
            case name, profession, role
            case isProjectManager = "is_project_manager"
        }
    }

    struct TodoDependencyOptionResponse: Encodable {
        let todoReference: String
        let title: String
        let assignee: String
        let status: String
        let blockedReason: String
        let result: String

        enum CodingKeys: String, CodingKey {
            case todoReference = "todo_ref"
            case title, assignee, status
            case blockedReason = "blocked_reason"
            case result
        }
    }

    struct TodoPluginOptionResponse: Encodable {
        let pluginReference: String
        let name: String
        let description: String

        enum CodingKeys: String, CodingKey {
            case pluginReference = "plugin_ref"
            case name, description
        }
    }

    struct TodoExecutionOptionsResponse: Encodable {
        let teams: [TodoTeamOptionResponse]
        let builtinCapabilities: [String]
        let plugins: [TodoPluginOptionResponse]

        enum CodingKeys: String, CodingKey {
            case teams
            case builtinCapabilities = "builtin_capabilities"
            case plugins
        }
    }

    struct TodoSourceMessageResponse: Encodable {
        let relation: String
        let content: String
        let attachmentNames: [String]
        let createdAtUnixMs: Int64

        enum CodingKeys: String, CodingKey {
            case relation, content
            case attachmentNames = "attachment_names"
            case createdAtUnixMs = "created_at_unix_ms"
        }
    }

    struct TodoExecutionContextResponse: Encodable {
        let title: String
        let detail: String
        let objective: String
        let scope: String
        let expectedOutputs: [String]
        let acceptanceCriteria: [String]
        let constraints: [String]
        let priority: Int
        let builtinCapabilities: [String]
        let plugins: [String]
        let sourceMessages: [TodoSourceMessageResponse]
        let prerequisites: [TodoDependencyResponse]
        let teamAssets: [TeamAssetSummaryResponse]

        enum CodingKeys: String, CodingKey {
            case title, detail, objective, scope, constraints, priority
            case expectedOutputs = "expected_outputs"
            case acceptanceCriteria = "acceptance_criteria"
            case builtinCapabilities = "builtin_capabilities"
            case plugins, prerequisites
            case sourceMessages = "source_messages"
            case teamAssets = "team_assets"
        }
    }

    struct TodoProgressResponse: Encodable {
        let sequence: Int64
        let kind: String
        let stage: String
        let detail: String
        let assetUpdateSuggestions: [LocalAgentTeamAssetUpdateSuggestion]
        let createdAtUnixMs: Int64

        init(progress: LocalAgentTodoProgress) {
            sequence = progress.sequence
            kind = progress.kind.rawValue
            stage = progress.stage
            detail = progress.detail
            assetUpdateSuggestions = progress.assetUpdateSuggestions
            createdAtUnixMs = progress.createdAtUnixMs
        }

        enum CodingKeys: String, CodingKey {
            case sequence, kind, stage, detail
            case assetUpdateSuggestions = "asset_update_suggestions"
            case createdAtUnixMs = "created_at_unix_ms"
        }
    }

    struct StructuredToolFailure: Encodable {
        struct Detail: Encodable {
            let code: String
            let field: String?
            let message: String
            let retryable: Bool
            let nextTool: String?

            enum CodingKeys: String, CodingKey {
                case code, field, message, retryable
                case nextTool = "next_tool"
            }
        }

        let ok = false
        let error: Detail
    }

    struct ProposalAcknowledgement: Encodable {
        let type: String
        let status: String
        let subject: String
    }

    struct DirectOpenResponse: Encodable {
        let conversationReference: String
        let targetAgentReference: String

        enum CodingKeys: String, CodingKey {
            case conversationReference = "conversation_ref"
            case targetAgentReference = "target_agent_ref"
        }
    }

    struct DirectSendResponse: Encodable {
        let conversationReference: String
        let messageReference: String
        let spawnedDeliveryCount: Int
        let routingStopReason: String?

        enum CodingKeys: String, CodingKey {
            case conversationReference = "conversation_ref"
            case messageReference = "message_ref"
            case spawnedDeliveryCount = "spawned_delivery_count"
            case routingStopReason = "routing_stop_reason"
        }
    }

    struct SendResponse: Encodable {
        let messageReference: String
        let completed: Bool
        let spawnedDeliveryCount: Int
        let routingStopReason: String?

        enum CodingKeys: String, CodingKey {
            case messageReference = "message_ref"
            case completed
            case spawnedDeliveryCount = "spawned_delivery_count"
            case routingStopReason = "routing_stop_reason"
        }
    }

    enum DocumentDraftResolution {
        case ready(references: [String], drafts: [ProjectAgentMessageAttachmentDraft])
        case failure(AgentToolOutcome)
    }

}
