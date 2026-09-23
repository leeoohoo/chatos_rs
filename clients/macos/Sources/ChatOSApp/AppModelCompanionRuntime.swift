import ChatOSConnector
import ChatOSCore
import Foundation

extension AppModel {
    func companionResources() -> [LocalConnectorCompanionResource] {
        let contacts = contacts.map { resource in
            companionResource(resource, kind: .contact)
        }
        let projects = projects.map { resource in
            companionResource(resource, kind: .project)
        }
        return contacts + projects
    }

    func resolveCompanionResource(id: String) async throws -> LocalConnectorCompanionResource {
        let parts = id.split(separator: ":", maxSplits: 1, omittingEmptySubsequences: false)
        guard parts.count == 2, !parts[1].isEmpty else {
            throw LocalConnectorCompanionResourceError.notFound
        }
        let sourceID = String(parts[1])
        switch parts[0] {
        case "contact":
            guard let resource = contacts.first(where: { $0.id == sourceID }),
                  resource.conversationID != nil else {
                throw LocalConnectorCompanionResourceError.unavailable
            }
            return companionResource(resource, kind: .contact)
        case "project":
            _ = try await ensureProjectConversation(projectID: sourceID)
            guard let resource = projects.first(where: { $0.id == sourceID }) else {
                throw LocalConnectorCompanionResourceError.notFound
            }
            return companionResource(resource, kind: .project)
        default:
            throw LocalConnectorCompanionResourceError.notFound
        }
    }

    private func companionResource(
        _ resource: ResourceItem,
        kind: LocalConnectorCompanionResourceKind
    ) -> LocalConnectorCompanionResource {
        let conversation = resource.conversationID.flatMap { id in
            workspaceConversations.first(where: { $0.id == id })
        }
        return LocalConnectorCompanionResource(
            id: "\(kind.rawValue):\(resource.id)",
            kind: kind,
            title: resource.title,
            subtitle: kind == .contact ? resource.subtitle : resource.contactName,
            conversationID: resource.conversationID,
            messageCount: conversation?.messageCount ?? 0,
            updatedAt: conversation.map { ISO8601DateFormatter().string(from: $0.updatedAt) }
        )
    }
}

enum LocalConnectorCompanionResourceError: LocalizedError {
    case notFound
    case unavailable

    var errorDescription: String? {
        switch self {
        case .notFound: "桌面客户端中没有这个会话入口。"
        case .unavailable: "这个会话入口暂时还不能开始对话。"
        }
    }
}

enum PetActivityActionError: LocalizedError {
    case retryUnavailable
    case cancelUnavailable
    case promptUnavailable
    case promptResolved
    case taskDetailUnavailable

    var errorDescription: String? {
        switch self {
        case .retryUnavailable:
            "当前事件缺少重试所需的任务运行信息，请打开详情处理。"
        case .cancelUnavailable:
            "当前事件缺少取消任务所需的信息，请打开详情处理。"
        case .promptUnavailable:
            "当前提问缺少直接处理所需的信息，请打开详情处理。"
        case .promptResolved:
            "这个提问已经处理或失效。"
        case .taskDetailUnavailable:
            "当前事件缺少读取任务执行过程所需的信息。"
        }
    }
}
