import AppKit
import Foundation
import Testing
@testable import ChatOSApp

struct ConversationFileLinkTests {
    @Test func relativeAndEncodedChineseNames() throws {
        let root = URL(fileURLWithPath: "/tmp/work")
        for target in ["李磊简历_排版优化版.docx", "./李磊 简历.docx", "./100%25.docx"] {
            let url = try #require(URL(string: target))
            let relative = try ConversationFileLinkReader.relativePath(url, root: root)
            #expect(!relative.hasPrefix("./"))
            #expect(!relative.contains("%25"))
        }
        #expect(try ConversationFileLinkReader.relativePath(
            URL(fileURLWithPath: "/tmp/work/李磊 简历.docx"), root: root
        ) == "李磊 简历.docx")
    }

    @Test func rejectsTraversalAndOtherConversations() throws {
        let root = URL(fileURLWithPath: "/tmp/work")
        for target in ["../other/file.docx", "%2e%2e/file", "sub/../../secret", "/tmp/work-other/file",
                       "file:///tmp/other/file", "file://server/tmp/work/file", "https://example.com/file"] {
            #expect(throws: (any Error).self) {
                try ConversationFileLinkReader.relativePath(try #require(URL(string: target)), root: root)
            }
        }
    }

    @Test func readsFullFileAndRejectsMissingDirectoriesAndSymlinks() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }
        let data = Data(repeating: 0x52, count: 150_123)
        try data.write(to: root.appendingPathComponent("李磊 简历.docx"))
        let file = try ConversationFileLinkReader.read(try #require(URL(string: "./李磊 简历.docx")), root: root)
        #expect(file.name == "李磊 简历.docx")
        #expect(file.data == data)
        try FileManager.default.createSymbolicLink(
            atPath: root.appendingPathComponent("link.docx").path,
            withDestinationPath: root.appendingPathComponent("李磊 简历.docx").path
        )
        try FileManager.default.createSymbolicLink(atPath: root.appendingPathComponent("dir").path,
                                                  withDestinationPath: root.path)
        for target in ["missing.docx", "link.docx", "dir/李磊 简历.docx", "."] {
            #expect(throws: (any Error).self) {
                try ConversationFileLinkReader.read(try #require(URL(string: target)), root: root)
            }
        }
    }

    @Test @MainActor func nativeLinksAreForwardedInsteadOfLaunchingAppKit() throws {
        let view = MarkdownLayoutTextView()
        var links: [URL] = []
        view.linkHandler = { links.append($0) }
        for target in ["./李磊 简历.docx", "https://example.com/file"] {
            let url = try #require(URL(string: target))
            #expect(view.textView(view, clickedOnLink: url, at: 0))
            #expect(links.last == url)
        }
        #expect(view.delegate === view)
        #expect(view.textView(view, clickedOnLink: 42, at: 0))
        #expect(links.count == 2)
    }
}
