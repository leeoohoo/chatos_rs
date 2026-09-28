import Testing
@testable import ChatOSApp

struct CodeSyntaxHighlighterTests {
    @Test
    func discoversSyntaxSpansWithoutBuildingAppKitTextOffMain() async {
        let spans = await Task.detached {
            CodeSyntaxHighlighter.spans(
                in: "let value = 42 // comment",
                fileName: "Example.swift"
            )
        }.value

        #expect(spans.contains { $0.style == .number })
        #expect(spans.contains { $0.style == .keyword })
        #expect(spans.contains { $0.style == .comment })
    }

    @Test
    func largeFilesStayInResponsivePlainTextMode() async {
        let content = String(
            repeating: "a",
            count: CodeSyntaxHighlighter.maximumHighlightedUTF16Count + 1
        )
        let spans = await Task.detached {
            CodeSyntaxHighlighter.spans(in: content, fileName: "Large.swift")
        }.value

        #expect(spans.isEmpty)
    }

    @Test
    func highlightSpanCountIsBounded() async {
        let content = String(repeating: "1 ", count: 25_000)
        let spans = await Task.detached {
            CodeSyntaxHighlighter.spans(in: content, fileName: "Values.swift")
        }.value

        #expect(spans.count == CodeSyntaxHighlighter.maximumSpanCount)
    }
}
