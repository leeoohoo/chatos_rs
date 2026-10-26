import Foundation
import os.signpost

/// Stable Instruments interval names for the macOS performance acceptance paths.
///
/// Keep these names short and unchanged so traces from different releases can be compared.
enum ChatOSPerformanceOperation: String, CaseIterable, Sendable {
    case conversationTimelineBuild = "ConversationTimelineBuild"
    case markdownParse = "MarkdownParse"
    case markdownLayout = "MarkdownLayout"
    case todoPresentationBuild = "TodoPresentationBuild"
    case petFrameUpdate = "PetFrameUpdate"
    case activationRecoveryDispatch = "ActivationRecoveryDispatch"
    case codeHighlightScan = "CodeHighlightScan"
    case codeHighlightApply = "CodeHighlightApply"

    nonisolated var signpostName: StaticString {
        switch self {
        case .conversationTimelineBuild: "ConversationTimelineBuild"
        case .markdownParse: "MarkdownParse"
        case .markdownLayout: "MarkdownLayout"
        case .todoPresentationBuild: "TodoPresentationBuild"
        case .petFrameUpdate: "PetFrameUpdate"
        case .activationRecoveryDispatch: "ActivationRecoveryDispatch"
        case .codeHighlightScan: "CodeHighlightScan"
        case .codeHighlightApply: "CodeHighlightApply"
        }
    }
}

/// A tiny wrapper around signposts that is safe to call from both MainActor and detached work.
/// Signposts are compiled into Release builds and have negligible cost when no trace is active.
enum ChatOSPerformanceSignpost {
    private static let log = OSLog(
        subsystem: "com.chatos.native",
        category: "Performance"
    )

    @inline(__always)
    nonisolated static func measure<T>(
        _ operation: ChatOSPerformanceOperation,
        _ body: () throws -> T
    ) rethrows -> T {
        let identifier = OSSignpostID(log: log)
        os_signpost(
            .begin,
            log: log,
            name: operation.signpostName,
            signpostID: identifier
        )
        defer {
            os_signpost(
                .end,
                log: log,
                name: operation.signpostName,
                signpostID: identifier
            )
        }
        return try body()
    }
}
