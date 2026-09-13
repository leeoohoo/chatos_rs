// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

import ChatOSCore
import Foundation

public actor NativeLocalNotepadService: NotepadServicing {
    private let accountSession: any NativeLocalAgentAccountSessionAccess

    public init(accountSession: any NativeLocalAgentAccountSessionAccess) {
        self.accountSession = accountSession
    }

    public func initialize() async throws {
        _ = try await records()
    }

    public func listFolders() async throws -> [String] {
        let snapshots = try await records()
        var folders = Set(snapshots.compactMap { snapshot in
            snapshot.draft.kind == .folder ? snapshot.draft.folder : nil
        })
        for snapshot in snapshots where snapshot.draft.kind == .note {
            var components = snapshot.draft.folder.split(separator: "/")
            while !components.isEmpty {
                folders.insert(components.joined(separator: "/"))
                components.removeLast()
            }
        }
        return folders.sorted { $0.localizedStandardCompare($1) == .orderedAscending }
    }

    public func createFolder(_ folder: String) async throws {
        let client = try await accountSession.activeClient()
        _ = try await client.putNotepadRecord(
            id: "folder:\(UUID().uuidString.lowercased())",
            expectedRevision: nil,
            draft: LocalAgentNotepadDraft(
                kind: .folder,
                folder: folder,
                title: "",
                content: "",
                tags: []
            )
        )
    }

    public func renameFolder(from: String, to: String) async throws {
        let client = try await accountSession.activeClient()
        try await client.renameNotepadFolder(from, replacement: to)
    }

    public func deleteFolder(_ folder: String, recursive: Bool) async throws {
        let client = try await accountSession.activeClient()
        try await client.deleteNotepadFolder(folder, recursive: recursive)
    }

    public func listNotes(query: String?, limit: Int) async throws -> [NotepadNote] {
        let normalizedQuery = query?.trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        return Array(try await records()
            .filter { snapshot in
                guard snapshot.draft.kind == .note else { return false }
                guard let normalizedQuery, !normalizedQuery.isEmpty else { return true }
                return snapshot.draft.title.lowercased().contains(normalizedQuery)
                    || snapshot.draft.folder.lowercased().contains(normalizedQuery)
                    || snapshot.draft.tags.contains { $0.lowercased().contains(normalizedQuery) }
                    || snapshot.draft.content.lowercased().contains(normalizedQuery)
            }
            .prefix(min(max(limit, 1), 500))
            .map(Self.note))
    }

    public func createNote(_ draft: NotepadNoteDraft) async throws -> NotepadNoteDetail {
        let client = try await accountSession.activeClient()
        let snapshot = try await client.putNotepadRecord(
            id: "note:\(UUID().uuidString.lowercased())",
            expectedRevision: nil,
            draft: LocalAgentNotepadDraft(
                kind: .note,
                folder: draft.folder,
                title: draft.title,
                content: draft.content,
                tags: draft.tags
            )
        )
        return Self.detail(snapshot)
    }

    public func fetchNote(id: String) async throws -> NotepadNoteDetail {
        let client = try await accountSession.activeClient()
        return Self.detail(try await client.notepadRecord(id: id))
    }

    public func updateNote(
        id: String,
        update: NotepadNoteUpdate
    ) async throws -> NotepadNoteDetail {
        let client = try await accountSession.activeClient()
        let current = try await client.notepadRecord(id: id)
        guard current.draft.kind == .note else {
            throw NativeLocalAgentIPCError.invalidResponse
        }
        let snapshot = try await client.putNotepadRecord(
            id: id,
            expectedRevision: current.revision,
            draft: LocalAgentNotepadDraft(
                kind: .note,
                folder: update.folder ?? current.draft.folder,
                title: update.title ?? current.draft.title,
                content: update.content ?? current.draft.content,
                tags: update.tags ?? current.draft.tags
            )
        )
        return Self.detail(snapshot)
    }

    public func deleteNote(id: String) async throws {
        let client = try await accountSession.activeClient()
        let current = try await client.notepadRecord(id: id)
        guard current.draft.kind == .note else {
            throw NativeLocalAgentIPCError.invalidResponse
        }
        try await client.deleteNotepadRecord(id: id, expectedRevision: current.revision)
    }

    private func records() async throws -> [LocalAgentNotepadSnapshot] {
        let client = try await accountSession.activeClient()
        return try await client.notepadRecords()
    }

    private static func detail(_ snapshot: LocalAgentNotepadSnapshot) -> NotepadNoteDetail {
        NotepadNoteDetail(note: note(snapshot), content: snapshot.draft.content)
    }

    private static func note(_ snapshot: LocalAgentNotepadSnapshot) -> NotepadNote {
        NotepadNote(
            id: snapshot.recordID,
            title: snapshot.draft.title,
            folder: snapshot.draft.folder,
            tags: snapshot.draft.tags,
            createdAt: parseDate(snapshot.createdAt),
            updatedAt: parseDate(snapshot.updatedAt),
            file: snapshot.recordID
        )
    }

    private static func parseDate(_ value: String) -> Date? {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter.date(from: value) ?? ISO8601DateFormatter().date(from: value)
    }
}
