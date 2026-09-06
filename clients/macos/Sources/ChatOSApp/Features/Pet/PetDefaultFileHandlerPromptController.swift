import AppKit
import CoreServices
import Foundation
import OSLog
import UniformTypeIdentifiers

@MainActor
final class PetDefaultFileHandlerPromptController: ObservableObject {
    struct Prompt: Identifiable, Equatable {
        let contentType: String
        let fileExtension: String

        var id: String { contentType }
    }

    typealias ContentTypeProvider = (URL) -> String?
    typealias DefaultApplicationProvider = (URL) -> String?
    typealias SupportedFileProvider = (URL, String) -> Bool
    typealias DefaultHandlerSetter = (String, String) -> OSStatus

    private static let logger = Logger(
        subsystem: "com.chatos.swift-client",
        category: "DefaultFileHandlerPrompt"
    )

    @Published private(set) var prompt: Prompt?

    private let bundleIdentifier: String
    private let contentTypeProvider: ContentTypeProvider
    private let defaultApplicationProvider: DefaultApplicationProvider
    private let supportedFileProvider: SupportedFileProvider
    private let defaultHandlerSetter: DefaultHandlerSetter

    convenience init(bundle: Bundle = .main) {
        let bundleIdentifier = bundle.bundleIdentifier ?? "com.chatos.swift-client"
        let declaredExtensions = Self.declaredExtensions(in: bundle)
        let declaredContentTypes = Self.declaredContentTypes(in: bundle)

        self.init(
            bundleIdentifier: bundleIdentifier,
            contentTypeProvider: { url in
                if let contentType = try? url.resourceValues(forKeys: [.contentTypeKey]).contentType {
                    return contentType.identifier
                }
                return UTType(filenameExtension: url.pathExtension)?.identifier
            },
            defaultApplicationProvider: { url in
                guard let appURL = NSWorkspace.shared.urlForApplication(toOpen: url) else { return nil }
                return Bundle(url: appURL)?.bundleIdentifier
            },
            supportedFileProvider: { url, identifier in
                let fileExtension = url.pathExtension.lowercased()
                if declaredExtensions.contains(fileExtension) {
                    return true
                }
                guard let fileType = UTType(identifier) else { return false }
                return declaredContentTypes.contains { declaredIdentifier in
                    guard let declaredType = UTType(declaredIdentifier) else { return false }
                    return fileType.conforms(to: declaredType)
                }
            },
            defaultHandlerSetter: { contentType, handlerBundleIdentifier in
                LSSetDefaultRoleHandlerForContentType(
                    contentType as CFString,
                    .all,
                    handlerBundleIdentifier as CFString
                )
            }
        )
    }

    init(
        bundleIdentifier: String,
        contentTypeProvider: @escaping ContentTypeProvider,
        defaultApplicationProvider: @escaping DefaultApplicationProvider,
        supportedFileProvider: @escaping SupportedFileProvider,
        defaultHandlerSetter: @escaping DefaultHandlerSetter
    ) {
        self.bundleIdentifier = bundleIdentifier
        self.contentTypeProvider = contentTypeProvider
        self.defaultApplicationProvider = defaultApplicationProvider
        self.supportedFileProvider = supportedFileProvider
        self.defaultHandlerSetter = defaultHandlerSetter
    }

    func offerAfterOpening(_ urls: [URL]) {
        guard let candidate = urls.lazy.compactMap(candidate(for:)).first else { return }
        prompt = Prompt(
            contentType: candidate.contentType,
            fileExtension: candidate.fileExtension
        )
    }

    func makeDefault() {
        guard let prompt else { return }
        self.prompt = nil

        let status = defaultHandlerSetter(prompt.contentType, bundleIdentifier)
        guard status == noErr else {
            Self.logger.error(
                "Failed to request default handler for \(prompt.contentType, privacy: .public): \(status)"
            )
            return
        }
        NSUpdateDynamicServices()
    }

    func dismiss() {
        prompt = nil
    }

    private func candidate(for url: URL) -> Candidate? {
        guard url.isFileURL,
              let contentType = contentTypeProvider(url),
              supportedFileProvider(url, contentType),
              defaultApplicationProvider(url) != bundleIdentifier else {
            return nil
        }
        return Candidate(
            contentType: contentType,
            fileExtension: url.pathExtension.lowercased()
        )
    }

    private static func declaredExtensions(in bundle: Bundle) -> Set<String> {
        documentTypes(in: bundle).reduce(into: Set<String>()) { result, item in
            let extensions = item["CFBundleTypeExtensions"] as? [String] ?? []
            result.formUnion(extensions.map { $0.lowercased() })
        }
    }

    private static func declaredContentTypes(in bundle: Bundle) -> Set<String> {
        documentTypes(in: bundle).reduce(into: Set<String>()) { result, item in
            result.formUnion(item["LSItemContentTypes"] as? [String] ?? [])
        }
    }

    private static func documentTypes(in bundle: Bundle) -> [[String: Any]] {
        bundle.object(forInfoDictionaryKey: "CFBundleDocumentTypes") as? [[String: Any]] ?? []
    }

    private struct Candidate {
        let contentType: String
        let fileExtension: String
    }
}
