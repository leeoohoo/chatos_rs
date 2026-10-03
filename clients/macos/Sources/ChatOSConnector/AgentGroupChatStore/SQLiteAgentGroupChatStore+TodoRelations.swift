import ChatOSAgentRuntime
import ChatOSCore
import CryptoKit
import Foundation
import SQLite3

extension SQLiteAgentGroupChatStore {
    public func listAgentTodoSources(
        ownerUserID: String,
        agentID: String,
        todoID: String
    ) throws -> [LocalAgentTodoSourceLink] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        guard try readTodo(ownerUserID: ownerUserID, agentID: agentID, todoID: todoID) != nil else {
            throw AgentGroupChatError.notFound
        }
        return try AgentTodoRepository.listSources(
            database,
            ownerUserID: ownerUserID,
            todoID: todoID,
            preparedStatement: recordPreparedStatement
        )
    }

    public func listTodoSources(
        ownerUserID: String,
        todoIDs: [String]
    ) throws -> [LocalAgentTodoSourceLink] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        let ids = Array(Set(todoIDs)).sorted()
        guard ids.count <= 500 else { throw AgentGroupChatError.invalidField("todoIDs") }
        for id in ids {
            try AgentGroupChatValidation.identifier(id, field: "todoID")
        }
        return try AgentTodoRepository.listSources(
            database,
            ownerUserID: ownerUserID,
            todoIDs: ids,
            preparedStatement: recordPreparedStatement
        )
    }

    public func listAgentTodoDependencies(
        ownerUserID: String,
        agentID: String,
        todoID: String
    ) throws -> [LocalAgentTodoDependency] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        guard try readTodo(ownerUserID: ownerUserID, agentID: agentID, todoID: todoID) != nil else {
            throw AgentGroupChatError.notFound
        }
        return try AgentTodoRepository.listDependencies(
            database,
            ownerUserID: ownerUserID,
            todoID: todoID,
            preparedStatement: recordPreparedStatement
        )
    }

    public func listTodoDependencies(
        ownerUserID: String,
        todoIDs: [String]
    ) throws -> [LocalAgentTodoDependency] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        let ids = Array(Set(todoIDs)).sorted()
        guard ids.count <= 500 else { throw AgentGroupChatError.invalidField("todoIDs") }
        for id in ids {
            try AgentGroupChatValidation.identifier(id, field: "todoID")
        }
        return try AgentTodoRepository.listDependencies(
            database,
            ownerUserID: ownerUserID,
            todoIDs: ids,
            preparedStatement: recordPreparedStatement
        )
    }

    public func setAgentTodoDependencies(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        dependencies: [LocalAgentTodoDependencyDraft],
        nowUnixMs: Int64
    ) throws -> [LocalAgentTodoDependency] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        guard nowUnixMs >= 0, dependencies.count <= 64,
              Set(dependencies.map(\.prerequisiteTodoID)).count == dependencies.count else {
            throw AgentGroupChatError.invalidField("todoDependencies")
        }
        for dependency in dependencies {
            try AgentGroupChatValidation.identifier(
                dependency.prerequisiteTodoID,
                field: "todoDependency"
            )
            try AgentGroupChatValidation.identifier(
                dependency.prerequisiteAgentID,
                field: "todoDependencyAgent"
            )
        }
        return try transaction {
            try replaceAgentTodoDependencies(
                ownerUserID: ownerUserID,
                agentID: agentID,
                todoID: todoID,
                dependencies: dependencies,
                nowUnixMs: nowUnixMs
            )
        }
    }

    func replaceAgentTodoDependencies(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        dependencies: [LocalAgentTodoDependencyDraft],
        nowUnixMs: Int64
    ) throws -> [LocalAgentTodoDependency] {
        guard let todo = try readTodo(
            ownerUserID: ownerUserID,
            agentID: agentID,
            todoID: todoID
        ) else { throw AgentGroupChatError.notFound }
        try execute(
            "DELETE FROM local_agent_todo_dependencies WHERE owner_user_id = ? AND todo_id = ?",
            [.text(ownerUserID), .text(todoID)]
        )
        let prerequisiteIDs = dependencies.map(\.prerequisiteTodoID)
        guard !prerequisiteIDs.contains(todoID) else {
            throw AgentGroupChatError.invalidField("todoDependencies")
        }
        let prerequisiteRecords = try AgentTodoRepository.findMany(
            database,
            ownerUserID: ownerUserID,
            todoIDs: prerequisiteIDs,
            preparedStatement: recordPreparedStatement
        )
        let prerequisitesByID = Dictionary(
            uniqueKeysWithValues: prerequisiteRecords.map { ($0.id, $0) }
        )
        guard dependencies.allSatisfy({ dependency in
            guard let prerequisite = prerequisitesByID[dependency.prerequisiteTodoID] else {
                return false
            }
            return prerequisite.agentID == dependency.prerequisiteAgentID
                && prerequisite.teamRoomID == todo.teamRoomID
        }) else {
            throw AgentGroupChatError.invalidField("todoDependencies")
        }
        let createsCycle = try AgentTodoRepository.dependenciesCreateCycle(
            database,
            ownerUserID: ownerUserID,
            prerequisiteTodoIDs: prerequisiteIDs,
            todoID: todoID,
            preparedStatement: recordPreparedStatement
        )
        guard !createsCycle else {
            throw AgentGroupChatError.invalidField("todoDependencyCycle")
        }
        try AgentTodoRepository.insertDependencies(
            database,
            ownerUserID: ownerUserID,
            todoID: todoID,
            prerequisiteTodoIDs: prerequisiteIDs,
            nowUnixMs: nowUnixMs,
            preparedStatement: recordPreparedStatement
        )
        return try AgentTodoRepository.listDependencies(
            database,
            ownerUserID: ownerUserID,
            todoID: todoID,
            preparedStatement: recordPreparedStatement
        )
    }

    public func linkAgentTodoSources(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        sources: [LocalAgentTodoSourceDraft],
        nowUnixMs: Int64
    ) throws -> [LocalAgentTodoSourceLink] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        guard !sources.isEmpty, sources.count <= 64, nowUnixMs >= 0 else {
            throw AgentGroupChatError.invalidField("sourceMessageRefs")
        }
        for source in sources {
            try AgentGroupChatValidation.identifier(
                source.roomID,
                field: "sourceConversation"
            )
            try AgentGroupChatValidation.identifier(source.messageID, field: "sourceMessage")
        }
        return try transaction {
            guard try readTodo(ownerUserID: ownerUserID, agentID: agentID, todoID: todoID) != nil else {
                throw AgentGroupChatError.notFound
            }
            let messageRoomIDs = try AgentMessageRepository.roomIDs(
                database,
                ownerUserID: ownerUserID,
                messageIDs: sources.map(\.messageID),
                preparedStatement: recordPreparedStatement
            )
            guard sources.allSatisfy({ messageRoomIDs[$0.messageID] == $0.roomID }) else {
                throw AgentGroupChatError.invalidField("sourceMessageRefs")
            }
            try AgentTodoRepository.insertSources(
                database,
                ownerUserID: ownerUserID,
                todoID: todoID,
                sources: sources,
                nowUnixMs: nowUnixMs,
                preparedStatement: recordPreparedStatement
            )
            return try AgentTodoRepository.listSources(
                database,
                ownerUserID: ownerUserID,
                todoID: todoID,
                preparedStatement: recordPreparedStatement
            )
        }
    }

    public func listAgentTodoProgress(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        limit: Int
    ) throws -> [LocalAgentTodoProgress] {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        guard (1...500).contains(limit),
              try readTodo(ownerUserID: ownerUserID, agentID: agentID, todoID: todoID) != nil else {
            throw AgentGroupChatError.invalidField("todoProgressLimit")
        }
        return try AgentTodoRepository.listProgress(
            database,
            ownerUserID: ownerUserID,
            agentID: agentID,
            todoID: todoID,
            limit: limit,
            preparedStatement: recordPreparedStatement
        )
    }

    public func appendAgentTodoProgress(
        ownerUserID: String,
        agentID: String,
        todoID: String,
        kind: LocalAgentTodoProgressKind,
        runID: String?,
        stage: String,
        detail: String,
        assetUpdateSuggestions: [LocalAgentTeamAssetUpdateSuggestion],
        nowUnixMs: Int64
    ) throws -> LocalAgentTodoProgress {
        try AgentGroupChatValidation.identifier(ownerUserID, field: "ownerUserID")
        try AgentGroupChatValidation.identifier(agentID, field: "agentID")
        try AgentGroupChatValidation.identifier(todoID, field: "todoID")
        if let runID { try AgentGroupChatValidation.identifier(runID, field: "runID") }
        try AgentGroupChatValidation.optionalText(stage, field: "todoProgressStage", maximumLength: 240)
        try AgentGroupChatValidation.text(detail, field: "todoProgressDetail", maximumLength: 16_000)
        guard assetUpdateSuggestions.count <= 8 else {
            throw AgentGroupChatError.invalidField("assetUpdateSuggestions")
        }
        try assetUpdateSuggestions.forEach { try $0.validate() }
        let encodedSuggestions: String
        do {
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys, .withoutEscapingSlashes]
            encodedSuggestions = String(
                decoding: try encoder.encode(assetUpdateSuggestions),
                as: UTF8.self
            )
        } catch {
            throw AgentGroupChatError.invalidField("assetUpdateSuggestions")
        }
        guard nowUnixMs >= 0 else { throw AgentGroupChatError.invalidField("nowUnixMs") }
        return try transaction {
            guard try readTodo(ownerUserID: ownerUserID, agentID: agentID, todoID: todoID) != nil else {
                throw AgentGroupChatError.notFound
            }
            let sequence = try AgentTodoRepository.nextProgressSequence(
                database,
                ownerUserID: ownerUserID,
                todoID: todoID,
                preparedStatement: recordPreparedStatement
            )
            let progress = LocalAgentTodoProgress(
                id: UUID().uuidString.lowercased(),
                ownerUserID: ownerUserID,
                agentID: agentID,
                todoID: todoID,
                sequence: sequence,
                kind: kind,
                runID: runID,
                stage: stage,
                detail: detail,
                assetUpdateSuggestions: assetUpdateSuggestions,
                createdAtUnixMs: nowUnixMs
            )
            try execute(
                """
                INSERT INTO local_agent_todo_events (
                    owner_user_id, id, todo_id, sequence, run_id, kind, stage, detail,
                    asset_update_suggestions_json, created_at_unix_ms
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                """,
                [
                    .text(ownerUserID), .text(progress.id), .text(todoID), .integer(sequence),
                    runID.map(Value.text) ?? .null, .text(kind.rawValue), .text(stage),
                    .text(detail), .text(encodedSuggestions), .integer(nowUnixMs),
                ]
            )
            return progress
        }
    }

}
