import ChatOSCore
import CoreGraphics

enum SidebarSelection: Hashable {
    case contact(String)
    case project(String)
    case localConnector
    case applications
    case mediaStudio
    case agentGroupChat
    case requirementSurveys
    case pluginApplication(String, String)
    case terminal(String)
    case remote(String)
}

enum ProjectWorkspaceTab: String, CaseIterable, Identifiable {
    case directory = "项目目录"
    case messages = "用户消息"
    case settings = "项目设置"

    var id: Self { self }

    func title(language: ChatOSLanguage) -> String {
        guard language == .english else { return rawValue }
        return switch self {
        case .directory: "Project Files"
        case .messages: "Messages"
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
}

final class VisualSessionFrameImage: @unchecked Sendable, Equatable {
    let image: CGImage

    init(image: CGImage) {
        self.image = image
    }

    static func == (lhs: VisualSessionFrameImage, rhs: VisualSessionFrameImage) -> Bool {
        lhs === rhs
    }
}

struct VisualSessionPresentation: Equatable, Sendable {
    var session: PluginVisualSession
    var isExpanded: Bool
    var frameImage: VisualSessionFrameImage?

    var ownerSessionID: String { session.owner.conversationID }
}
