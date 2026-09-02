import ChatOSCore

enum SidebarSelection: Hashable {
    case contact(String)
    case project(String)
    case localConnector
    case applications
    case pluginApplication(String, String)
    case terminal(String)
    case remote(String)
}

enum ProjectWorkspaceTab: String, CaseIterable, Identifiable {
    case directory = "项目目录"
    case messages = "用户消息"
    case plan = "Plan"
    case settings = "项目设置"

    var id: Self { self }

    func title(language: ChatOSLanguage) -> String {
        guard language == .english else { return rawValue }
        return switch self {
        case .directory: "Project Files"
        case .messages: "Messages"
        case .plan: "Plan"
        case .settings: "Project Settings"
        }
    }
}

struct ResourceItem: Identifiable, Hashable {
    let id: String
    let title: String
    let subtitle: String?
    let conversationID: String?
    let contactName: String?
}

struct PetQuickChatResource: Identifiable, Hashable {
    enum Kind: Hashable {
        case contact
        case project
    }

    let id: String
    let sourceID: String
    let kind: Kind
    let title: String
    let subtitle: String?
    let conversationID: String?

    var allowsPlanMode: Bool { kind == .project }
}

struct VisualSessionPresentation: Equatable {
    var session: PluginVisualSession
    var isExpanded: Bool

    var ownerSessionID: String { session.owner.conversationID }
}
