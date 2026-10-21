import Foundation
import Testing

@Suite("Story presentation architecture")
struct StoryPresentationBoundaryTests {
    @Test("presentation files do not regain native storage, media-byte, or confirmation writes")
    func enforcesPresentationBoundary() throws {
        let macOSRoot = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
        let storyRoot = macOSRoot
            .appendingPathComponent("Sources/ChatOSApp/Features/MediaStudio/Story")
        let fixedPresentationFiles: Set<String> = [
            "StoryAgentViews.swift",
            "StoryEditors.swift",
            "StoryMediaBatchViews.swift",
            "StoryPromptInspectorView.swift",
            "StoryRelationGraphView.swift",
        ]
        let filenames = try FileManager.default.contentsOfDirectory(atPath: storyRoot.path)
            .filter { name in
                name.hasSuffix(".swift")
                    && name != "StoryStudioAssembly.swift"
                    && (name.hasPrefix("StoryStudio")
                        || name.hasPrefix("StoryWorkbench")
                        || fixedPresentationFiles.contains(name))
            }
            .sorted()
        #expect(!filenames.isEmpty)

        let forbiddenLiterals = [
            "StoryProjectStore",
            "Data(contentsOf:",
            ".base64EncodedString()",
            "startAccessingSecurityScopedResource",
            "store.fileURL(",
            "store.saveImage(",
            "store.saveVideo(",
            "store.applyActualVideoLastFrame(",
        ]
        let confirmationAssignment = try NSRegularExpression(
            pattern: #"\b(confirmedImageID|confirmedFrameID|confirmedLastFrameID|userSelectedFirstFrameID|inheritedFirstFrameSourceSegmentID|useLastFrameForVideo)\s*=(?!=)"#
        )

        for filename in filenames {
            let source = try String(
                contentsOf: storyRoot.appendingPathComponent(filename),
                encoding: .utf8
            )
            for literal in forbiddenLiterals {
                #expect(!source.contains(literal), "\(filename) contains forbidden presentation dependency: \(literal)")
            }
            let fullRange = NSRange(source.startIndex..., in: source)
            #expect(
                confirmationAssignment.firstMatch(in: source, range: fullRange) == nil,
                "\(filename) writes Story confirmation state directly"
            )
        }
    }
}
