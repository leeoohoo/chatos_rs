import ChatOSCore
import Foundation
import Testing
@testable import ChatOSApp

@Suite("Notepad selection")
@MainActor
struct NotepadViewModelTests {
    @Test("the latest clicked note wins even when an earlier fetch finishes later")
    func latestSelectionWins() async {
        let service = NotepadSelectionTestService()
        let viewModel = NotepadViewModel(service: service)
        await viewModel.load()
        #expect(viewModel.selectedNoteID == "a")

        let slowSelection = Task { await viewModel.selectNote("b") }
        try? await Task.sleep(for: .milliseconds(15))
        await viewModel.selectNote("c")
        await slowSelection.value

        #expect(viewModel.selectedNoteID == "c")
        #expect(viewModel.selectedTreeNodeID == "note:c")
        #expect(viewModel.content == "content-c")
    }

    @Test("clicking the current note cancels a pending selection")
    func currentSelectionCancelsPendingFetch() async {
        let service = NotepadSelectionTestService()
        let viewModel = NotepadViewModel(service: service)
        await viewModel.load()

        let slowSelection = Task { await viewModel.selectNote("b") }
        try? await Task.sleep(for: .milliseconds(15))
        await viewModel.selectNote("a")
        await slowSelection.value

        #expect(viewModel.selectedNoteID == "a")
        #expect(viewModel.selectedTreeNodeID == "note:a")
        #expect(viewModel.content == "content-a")
    }

    @Test("saving a metadata-only response keeps the editor content intact")
    func saveKeepsEditorContent() async {
        let service = NotepadSelectionTestService()
        let viewModel = NotepadViewModel(service: service)
        await viewModel.load()
        viewModel.content = "edited locally"

        #expect(await viewModel.save())

        #expect(viewModel.content == "edited locally")
        #expect(!viewModel.isDirty)
    }

    @Test("a pasted image placeholder becomes standard Markdown after upload")
    func pastedImageBecomesMarkdown() async {
        let service = NotepadSelectionTestService()
        let viewModel = NotepadViewModel(service: service)
        await viewModel.load()
        let placeholder = "![正在上传图片…](chatos-uploading://placeholder)"
        viewModel.content = "before\n\n\(placeholder)\n\nafter"

        await viewModel.uploadPastedImage(
            .init(data: Data([0x89, 0x50, 0x4e, 0x47]), mimeType: "image/png", name: "screen [1].png"),
            placeholder: placeholder
        )

        #expect(viewModel.content == "before\n\n![screen \\[1\\]](<https://example.test/image.png>)\n\nafter")
        #expect(!viewModel.isUploadingImage)
        #expect(viewModel.errorMessage == nil)
    }
}

private actor NotepadSelectionTestService: NotepadServicing {
    private let notes: [NotepadNote] = [
        .init(
            id: "a",
            title: "A",
            folder: "",
            tags: [],
            createdAt: Date(timeIntervalSince1970: 3),
            updatedAt: Date(timeIntervalSince1970: 3),
            file: "a.md"
        ),
        .init(
            id: "b",
            title: "B",
            folder: "",
            tags: [],
            createdAt: Date(timeIntervalSince1970: 2),
            updatedAt: Date(timeIntervalSince1970: 2),
            file: "b.md"
        ),
        .init(
            id: "c",
            title: "C",
            folder: "",
            tags: [],
            createdAt: Date(timeIntervalSince1970: 1),
            updatedAt: Date(timeIntervalSince1970: 1),
            file: "c.md"
        ),
    ]

    func initialize() async throws {}
    func listFolders() async throws -> [String] { [] }
    func createFolder(_ folder: String) async throws {}
    func renameFolder(from: String, to: String) async throws {}
    func deleteFolder(_ folder: String, recursive: Bool) async throws {}
    func listNotes(query: String?, limit: Int) async throws -> [NotepadNote] { notes }

    func createNote(_ draft: NotepadNoteDraft) async throws -> NotepadNoteDetail {
        detail(id: "a")
    }

    func fetchNote(id: String) async throws -> NotepadNoteDetail {
        if id == "b" {
            try await Task.sleep(for: .milliseconds(120))
        }
        return detail(id: id)
    }

    func updateNote(id: String, update: NotepadNoteUpdate) async throws -> NotepadNoteDetail {
        let note = notes.first(where: { $0.id == id }) ?? notes[0]
        return .init(note: note, content: "")
    }

    func uploadImage(
        _ image: NotepadImageUpload,
        noteID: String
    ) async throws -> NotepadImageAsset {
        .init(
            url: URL(string: "https://example.test/image.png")!,
            mimeType: image.mimeType,
            name: image.name,
            size: image.data.count
        )
    }

    func deleteNote(id: String) async throws {}

    private func detail(id: String) -> NotepadNoteDetail {
        let note = notes.first(where: { $0.id == id }) ?? notes[0]
        return .init(note: note, content: "content-\(note.id)")
    }
}
