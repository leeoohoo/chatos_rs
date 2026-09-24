import Foundation

public struct BundledAgentSkillDescriptor: Sendable, Equatable, Identifiable {
    public enum Role: String, Sendable {
        case router
        case specialist
        case policy
    }

    public let name: String
    public let role: Role
    public let relativeDirectory: String
    public let requiresFrontmatter: Bool

    public var id: String { name }
}

/// Single inventory for product-owned, on-disk Skills. Plugin Skills stay with their release and
/// enter the runtime through the plugin adapter instead of being copied into this bundle.
public enum BundledAgentSkillCatalog {
    public static let skills: [BundledAgentSkillDescriptor] = [
        .init(
            name: "chatos-compact-communication",
            role: .policy,
            relativeDirectory: "chatos-compact-communication",
            requiresFrontmatter: false
        ),
        .init(
            name: "chatos-skill-runtime",
            role: .policy,
            relativeDirectory: "chatos-skill-runtime",
            requiresFrontmatter: true
        ),
        .init(
            name: "chatos-relay-context",
            role: .specialist,
            relativeDirectory: "chatos-relay-context",
            requiresFrontmatter: true
        ),
        .init(
            name: "chatos-collaboration-messaging",
            role: .specialist,
            relativeDirectory: "chatos-collaboration-messaging",
            requiresFrontmatter: true
        ),
        .init(
            name: "chatos-agent-staffing",
            role: .specialist,
            relativeDirectory: "chatos-agent-staffing",
            requiresFrontmatter: true
        ),
        .init(
            name: "chatos-todo-planning",
            role: .specialist,
            relativeDirectory: "chatos-todo-planning",
            requiresFrontmatter: true
        ),
        .init(
            name: "chatos-todo-execution",
            role: .specialist,
            relativeDirectory: "chatos-todo-execution",
            requiresFrontmatter: true
        ),
        .init(
            name: "chatos-team-knowledge",
            role: .specialist,
            relativeDirectory: "chatos-team-knowledge",
            requiresFrontmatter: true
        ),
        .init(
            name: "chatos-project-dashboard",
            role: .specialist,
            relativeDirectory: "chatos-project-dashboard",
            requiresFrontmatter: true
        ),
        .init(
            name: "chatos-agent-builder",
            role: .specialist,
            relativeDirectory: "chatos-agent-builder",
            requiresFrontmatter: true
        ),
        .init(
            name: "chatos-command-approval",
            role: .specialist,
            relativeDirectory: "chatos-command-approval",
            requiresFrontmatter: true
        ),
        .init(
            name: "chatos-remote-connection",
            role: .specialist,
            relativeDirectory: "chatos-remote-connection",
            requiresFrontmatter: true
        ),
        .init(
            name: "requirement-survey",
            role: .router,
            relativeDirectory: "requirement-survey",
            requiresFrontmatter: true
        ),
        .init(
            name: "requirement-survey-create",
            role: .specialist,
            relativeDirectory: "requirement-survey-create",
            requiresFrontmatter: true
        ),
        .init(
            name: "requirement-survey-read-results",
            role: .specialist,
            relativeDirectory: "requirement-survey-read-results",
            requiresFrontmatter: true
        ),
        .init(
            name: "requirement-survey-resolve",
            role: .specialist,
            relativeDirectory: "requirement-survey-resolve",
            requiresFrontmatter: true
        ),
        .init(
            name: "requirement-survey-review-execution",
            role: .specialist,
            relativeDirectory: "requirement-survey-review-execution",
            requiresFrontmatter: true
        ),
        .init(
            name: "chatos-terminal",
            role: .router,
            relativeDirectory: "chatos-terminal",
            requiresFrontmatter: true
        ),
        .init(
            name: "chatos-project-files",
            role: .router,
            relativeDirectory: "chatos-project-files",
            requiresFrontmatter: true
        ),
        .init(
            name: "chatos-project-read",
            role: .specialist,
            relativeDirectory: "chatos-project-read",
            requiresFrontmatter: true
        ),
        .init(
            name: "chatos-project-write",
            role: .specialist,
            relativeDirectory: "chatos-project-write",
            requiresFrontmatter: true
        ),
        .init(
            name: "chatos-project-team-setup",
            role: .specialist,
            relativeDirectory: "chatos-project-team-setup",
            requiresFrontmatter: true
        ),
        .init(
            name: "chatos-terminal-command-execution",
            role: .specialist,
            relativeDirectory: "chatos-terminal-command-execution",
            requiresFrontmatter: true
        ),
        .init(
            name: "chatos-terminal-process-observation",
            role: .specialist,
            relativeDirectory: "chatos-terminal-process-observation",
            requiresFrontmatter: true
        ),
        .init(
            name: "chatos-terminal-process-control",
            role: .specialist,
            relativeDirectory: "chatos-terminal-process-control",
            requiresFrontmatter: true
        ),
    ]

    public static func skill(named name: String) -> BundledAgentSkillDescriptor? {
        skills.first { $0.name == name }
    }
}
