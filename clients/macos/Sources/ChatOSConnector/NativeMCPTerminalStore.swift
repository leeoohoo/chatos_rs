import Foundation

actor NativeMCPTerminalStore {
    private static let maximumLogs = 4_000
    private static let retainedLogTarget = 3_500
    private static let maximumLogBytesPerProcess = 2 * 1_024 * 1_024
    private static let retainedLogBytesTarget = 1_536 * 1_024
    private static let maximumLogEntryBytes = 2 * 1_024 * 1_024
    fileprivate static let maximumPendingOutputBytes = 512 * 1_024
    private static let defaultMaximumRetainedExitedProcesses = 100
    private static let maximumOutputCharacters = 512 * 1_024
    private static let maximumWaitMilliseconds = 2 * 60 * 60 * 1_000
    private static let defaultForegroundTimeoutMilliseconds = 5 * 60 * 1_000
    private static let maximumForegroundTimeoutMilliseconds = 15 * 60 * 1_000
    private static let minimumForegroundTimeoutMilliseconds = 1_000

    private var processes: [String: ManagedTerminalProcess] = [:]
    private var nextProcessSequence = 0
    private let maximumRetainedExitedProcesses: Int

    init(
        maximumRetainedExitedProcesses: Int = NativeMCPTerminalStore
            .defaultMaximumRetainedExitedProcesses
    ) {
        self.maximumRetainedExitedProcesses = max(1, maximumRetainedExitedProcesses)
    }

    static var toolNames: Set<String> {
        [
            "execute_command", "get_recent_logs", "process_list", "process_poll",
            "process_log", "process_wait", "process_write", "process_kill", "process",
        ]
    }

    static var toolDefinitions: [NativeJSONValue] {
        [
            definition(
                name: "execute_command",
                description: "在当前项目中执行命令。前台默认最多等待 300 秒；长时间运行的命令应使用 background=true，再通过 process_poll 或 process_wait 跟踪。",
                properties: [
                    "path": .object(["type": .string("string")]),
                    "common": .object(["type": .string("string")]),
                    "command": .object(["type": .string("string")]),
                    "background": .object(["type": .string("boolean"), "default": .bool(false)]),
                    "timeout_ms": integerSchema(
                        minimum: Self.minimumForegroundTimeoutMilliseconds,
                        maximum: Self.maximumForegroundTimeoutMilliseconds
                    ),
                    "timeout": integerSchema(
                        minimum: Self.minimumForegroundTimeoutMilliseconds / 1_000,
                        maximum: Self.maximumForegroundTimeoutMilliseconds / 1_000
                    ),
                ],
                required: []
            ),
            definition(
                name: "get_recent_logs",
                description: "读取当前项目最近执行的命令日志。",
                properties: [
                    "per_terminal_limit": integerSchema(minimum: 1, maximum: 50),
                    "terminal_limit": integerSchema(minimum: 1, maximum: 20),
                ],
                required: []
            ),
            definition(
                name: "process_list",
                description: "列出当前项目中的命令进程。",
                properties: [
                    "include_exited": .object(["type": .string("boolean"), "default": .bool(false)]),
                    "limit": integerSchema(minimum: 1, maximum: 100),
                ],
                required: []
            ),
            definition(
                name: "process_poll",
                description: "轮询命令进程的状态和增量日志。",
                properties: processReadProperties(defaultLimit: 80),
                required: ["terminal_id"]
            ),
            definition(
                name: "process_log",
                description: "分页读取命令进程日志。",
                properties: processReadProperties(defaultLimit: 200),
                required: ["terminal_id"]
            ),
            definition(
                name: "process_wait",
                description: "等待命令进程结束或等待超时。",
                properties: [
                    "terminal_id": .object(["type": .string("string")]),
                    "timeout_ms": integerSchema(minimum: 1_000, maximum: Self.maximumWaitMilliseconds),
                    "timeout": integerSchema(minimum: 1, maximum: Self.maximumWaitMilliseconds / 1_000),
                ],
                required: ["terminal_id"]
            ),
            definition(
                name: "process_write",
                description: "向命令进程写入标准输入。",
                properties: [
                    "terminal_id": .object(["type": .string("string")]),
                    "data": .object(["type": .string("string")]),
                    "submit": .object(["type": .string("boolean"), "default": .bool(false)]),
                ],
                required: ["terminal_id", "data"]
            ),
            definition(
                name: "process_kill",
                description: "终止命令进程。",
                properties: ["terminal_id": .object(["type": .string("string")])],
                required: ["terminal_id"]
            ),
            definition(
                name: "process",
                description: "兼容的进程管理入口。支持 list、poll、log、wait、kill、write、submit、close。",
                properties: [
                    "action": .object([
                        "type": .string("string"),
                        "enum": .array(["list", "poll", "log", "wait", "kill", "write", "submit", "close"].map(NativeJSONValue.string)),
                    ]),
                    "terminal_id": .object(["type": .string("string")]),
                    "include_exited": .object(["type": .string("boolean")]),
                    "offset": integerSchema(minimum: 0, maximum: nil),
                    "limit": integerSchema(minimum: 1, maximum: 200),
                    "timeout_ms": integerSchema(minimum: 1_000, maximum: Self.maximumWaitMilliseconds),
                    "timeout": integerSchema(minimum: 1, maximum: Self.maximumWaitMilliseconds / 1_000),
                    "data": .object(["type": .string("string")]),
                ],
                required: ["action"]
            ),
        ]
    }

    func execute(
        command: String,
        cwd: URL,
        projectRoot: URL,
        background: Bool,
        timeoutMilliseconds: Int? = nil,
        ownerRunID: String? = nil
    ) async throws -> NativeJSONValue {
        let id = UUID().uuidString
        let process = Process()
        let input = Pipe()
        let output = Pipe()
        let error = Pipe()
        process.executableURL = URL(fileURLWithPath: "/bin/zsh")
        process.arguments = ["-lc", command]
        process.currentDirectoryURL = cwd
        process.environment = ProcessInfo.processInfo.environment
        process.standardInput = input
        process.standardOutput = output
        process.standardError = error

        let now = Self.timestamp()
        let processSequence = nextProcessSequence
        nextProcessSequence += 1
        let managed = ManagedTerminalProcess(
            id: id,
            sequence: processSequence,
            command: command,
            cwd: cwd,
            projectRoot: projectRoot.standardizedFileURL.resolvingSymlinksInPath(),
            ownerRunID: ownerRunID,
            process: process,
            input: input.fileHandleForWriting,
            output: output,
            error: error,
            status: "running",
            exitCode: nil,
            startedAt: now,
            lastActiveAt: now,
            logs: [],
            nextLogOffset: 0,
            retainedLogBytes: 0,
            logsWereTruncated: false,
            outputBytesWereTruncated: false
        )
        processes[id] = managed
        append(kind: "command", content: command + "\n", to: id)

        NativeProcessPipeReader.install(
            on: output.fileHandleForReading,
            onData: { [weak self] data in
                guard managed.stdoutPending.append(data) else { return }
                Task { await self?.flushPendingOutput(kind: "stdout", id: id) }
            },
            onEOF: { managed.stdoutDrain.complete() }
        )
        NativeProcessPipeReader.install(
            on: error.fileHandleForReading,
            onData: { [weak self] data in
                guard managed.stderrPending.append(data) else { return }
                Task { await self?.flushPendingOutput(kind: "stderr", id: id) }
            },
            onEOF: { managed.stderrDrain.complete() }
        )
        process.terminationHandler = { [weak self] terminated in
            Task { await self?.finish(id: id, exitCode: Int(terminated.terminationStatus)) }
        }

        do {
            try process.run()
        } catch {
            processes.removeValue(forKey: id)
            throw NativeMCPTerminalError.launchFailed(error.localizedDescription)
        }

        if background {
            return .object([
                "project_root": .string("."),
                "terminal_id": .string(id),
                "process_id": .string(id),
                "terminal_reused": .bool(false),
                "path": .string(displayPath(cwd, relativeTo: projectRoot)),
                "common": .string(command),
                "background": .bool(true),
                "busy": .bool(true),
                "output": .string(""),
                "output_chars": .number(0),
                "truncated": .bool(false),
                "finished_by": .string("background"),
            ])
        }

        let foregroundTimeoutMilliseconds = clamp(
            timeoutMilliseconds ?? Self.defaultForegroundTimeoutMilliseconds,
            Self.minimumForegroundTimeoutMilliseconds,
            Self.maximumForegroundTimeoutMilliseconds
        )
        let timedOut: Bool
        do {
            timedOut = try await waitForExit(
                id: id,
                timeoutMilliseconds: foregroundTimeoutMilliseconds
            )
        } catch is CancellationError {
            cancelProcess(id: id, reason: "terminal cancelled with executor run")
            throw CancellationError()
        }
        if timedOut {
            cancelProcess(
                id: id,
                reason: "foreground terminal timed out after \(foregroundTimeoutMilliseconds) ms"
            )
            _ = try? await waitForExit(id: id, timeoutMilliseconds: 1_000)
        }
        guard let completed = processes[id] else { throw NativeMCPTerminalError.processNotFound }
        let stdout = combinedOutput(completed, kinds: ["stdout"])
        let stderr = combinedOutput(completed, kinds: ["stderr"])
        let combined = combinedOutput(completed, kinds: ["stdout", "stderr"])
        let timeoutDescription = timedOut
            ? "前台命令在 \(foregroundTimeoutMilliseconds / 1_000) 秒内未结束，已终止。预计运行更久的命令请使用 background=true，并通过 process_poll 或 process_wait 跟踪。"
            : nil
        return .object([
            "project_root": .string("."),
            "terminal_id": .string(id),
            "process_id": .string(id),
            "terminal_reused": .bool(false),
            "path": .string(displayPath(cwd, relativeTo: projectRoot)),
            "common": .string(command),
            "background": .bool(false),
            "busy": .bool(completed.status != "exited"),
            "success": .bool(!timedOut && completed.exitCode == 0),
            "timed_out": .bool(timedOut),
            "timeout_ms": .number(Double(foregroundTimeoutMilliseconds)),
            "stdout": .string(stdout.text),
            "stderr": .string(stderr.text),
            "output": .string(combined.text),
            "output_chars": .number(Double(combined.characters)),
            "truncated": .bool(combined.truncated),
            "finished_by": .string(timedOut ? "timeout" : "exit"),
            "error": timeoutDescription.map(NativeJSONValue.string) ?? .null,
            "exit_code": completed.exitCode.map { .number(Double($0)) } ?? .null,
        ])
    }

    func call(
        name: String,
        arguments: [String: NativeJSONValue],
        projectRoot: URL,
        ownerRunID: String? = nil
    ) async throws -> NativeJSONValue {
        try validateOwnership(
            name: name,
            arguments: arguments,
            ownerRunID: ownerRunID
        )
        switch name {
        case "get_recent_logs":
            return recentLogs(
                projectRoot: projectRoot,
                perTerminalLimit: clamp(arguments.integer("per_terminal_limit") ?? 10, 1, 50),
                terminalLimit: clamp(arguments.integer("terminal_limit") ?? 20, 1, 20),
                ownerRunID: ownerRunID
            )
        case "process_list":
            return processList(
                projectRoot: projectRoot,
                includeExited: arguments.bool("include_exited") ?? false,
                limit: clamp(arguments.integer("limit") ?? 20, 1, 100),
                ownerRunID: ownerRunID
            )
        case "process_poll":
            return try poll(
                id: requiredID(arguments),
                projectRoot: projectRoot,
                offset: arguments.integer("offset"),
                limit: clamp(arguments.integer("limit") ?? 80, 1, 200)
            )
        case "process_log":
            let offset = arguments.integer("offset")
            let limit = clamp(arguments.integer("limit") ?? 200, 1, 200)
            let polled = try poll(
                id: requiredID(arguments),
                projectRoot: projectRoot,
                offset: offset,
                limit: limit
            )
            guard case let .object(values) = polled else { return polled }
            let logs = values.array("logs")
            return .object([
                "terminal_id": values["terminal_id"] ?? .null,
                "status": values["status"] ?? .string("unknown"),
                "output": .string(logs.compactMap(\.content).joined()),
                "offset": offset.map { .number(Double($0)) } ?? .null,
                "limit": .number(Double(limit)),
                "has_more": values["has_more"] ?? .bool(false),
                "next_offset": values["next_offset"] ?? .null,
                "truncated": values["truncated"] ?? .bool(false),
                "truncation": values["truncation"] ?? .object([
                    "truncated": .bool(false),
                ]),
            ])
        case "process_wait":
            return try await processWait(
                id: requiredID(arguments),
                projectRoot: projectRoot,
                timeoutMilliseconds: timeoutMilliseconds(arguments)
            )
        case "process_write":
            guard let data = arguments.string("data") else {
                throw NativeMCPTerminalError.invalidArguments("缺少参数：data")
            }
            return try write(
                id: requiredID(arguments),
                projectRoot: projectRoot,
                data: data,
                submit: arguments.bool("submit") ?? false
            )
        case "process_kill":
            return try kill(id: requiredID(arguments), projectRoot: projectRoot)
        case "process":
            return try await compatibilityCall(
                arguments: arguments,
                projectRoot: projectRoot,
                ownerRunID: ownerRunID
            )
        default:
            throw NativeMCPTerminalError.unsupportedTool(name)
        }
    }

    @discardableResult
    func cancel(ownerRunID: String) -> Int {
        let matchingIDs = processes.values.compactMap { process in
            process.ownerRunID == ownerRunID && process.status != "exited" ? process.id : nil
        }
        for id in matchingIDs {
            cancelProcess(id: id, reason: "terminal cancelled with executor run")
        }
        return matchingIDs.count
    }

    @discardableResult
    func cancelAllOwnedProcesses() -> Int {
        let matchingIDs = processes.values.compactMap { process in
            process.ownerRunID != nil && process.status != "exited" ? process.id : nil
        }
        for id in matchingIDs {
            cancelProcess(id: id, reason: "terminal cancelled with executor reset")
        }
        return matchingIDs.count
    }

    private func compatibilityCall(
        arguments: [String: NativeJSONValue],
        projectRoot: URL,
        ownerRunID: String?
    ) async throws -> NativeJSONValue {
        guard let action = arguments.string("action")?.lowercased() else {
            throw NativeMCPTerminalError.invalidArguments("缺少参数：action")
        }
        var result: NativeJSONValue
        switch action {
        case "list":
            result = processList(
                projectRoot: projectRoot,
                includeExited: arguments.bool("include_exited") ?? false,
                limit: clamp(arguments.integer("limit") ?? 20, 1, 100),
                ownerRunID: ownerRunID
            )
        case "poll":
            result = try poll(id: requiredID(arguments), projectRoot: projectRoot, offset: arguments.integer("offset"), limit: clamp(arguments.integer("limit") ?? 80, 1, 200))
        case "log":
            result = try await call(name: "process_log", arguments: arguments, projectRoot: projectRoot)
        case "wait":
            result = try await processWait(id: requiredID(arguments), projectRoot: projectRoot, timeoutMilliseconds: timeoutMilliseconds(arguments))
        case "kill":
            result = try kill(id: requiredID(arguments), projectRoot: projectRoot)
        case "write", "submit", "close":
            let data = action == "close" ? "\u{4}" : (arguments.string("data") ?? "")
            result = try write(id: requiredID(arguments), projectRoot: projectRoot, data: data, submit: action == "submit")
        default:
            throw NativeMCPTerminalError.invalidArguments("不支持的进程操作：\(action)")
        }
        guard case var .object(values) = result else { return result }
        values["action"] = .string(action)
        return .object(values)
    }

    private func recentLogs(
        projectRoot: URL,
        perTerminalLimit: Int,
        terminalLimit: Int,
        ownerRunID: String? = nil
    ) -> NativeJSONValue {
        let matching = matchingProcesses(
            projectRoot: projectRoot,
            includeExited: true,
            ownerRunID: ownerRunID
        )
        let terminals = matching.prefix(terminalLimit).map { process in
            let logs = Array(process.logs.suffix(perTerminalLimit))
            let truncated = process.logsWereTruncated
            return NativeJSONValue.object([
                "terminal_id": .string(process.id),
                "terminal_name": .string(terminalName(process)),
                "status": .string(process.status),
                "cwd": .string(displayPath(process.cwd, relativeTo: process.projectRoot)),
                "project_id": .null,
                "last_active_at": .string(process.lastActiveAt),
                "log_count": .number(Double(process.logs.count)),
                "returned_log_count": .number(Double(logs.count)),
                "truncated": .bool(truncated),
                "truncation": .object([
                    "truncated": .bool(truncated),
                    "first_available_offset": process.logs.first.map {
                        .number(Double($0.offset))
                    } ?? .null,
                ]),
                "logs": .array(logs.map(\.jsonValue)),
            ])
        }
        return .object([
            "result_scope": .string(resultScope(terminals.count)),
            "is_multiple_terminals": .bool(terminals.count > 1),
            "terminal_count": .number(Double(terminals.count)),
            "total_terminals": .number(Double(matching.count)),
            "per_terminal_limit": .number(Double(perTerminalLimit)),
            "terminal_limit": .number(Double(terminalLimit)),
            "terminals": .array(Array(terminals)),
        ])
    }

    private func processList(
        projectRoot: URL,
        includeExited: Bool,
        limit: Int,
        ownerRunID: String? = nil
    ) -> NativeJSONValue {
        let matching = Array(matchingProcesses(
            projectRoot: projectRoot,
            includeExited: includeExited,
            ownerRunID: ownerRunID
        ).prefix(limit))
        let entries = matching.map(snapshot)
        return .object([
            "status": .string("ok"),
            "result_scope": .string(resultScope(entries.count)),
            "is_multiple_terminals": .bool(entries.count > 1),
            "terminal_count": .number(Double(entries.count)),
            "process_count": .number(Double(entries.count)),
            "visible_total": .number(Double(entries.count)),
            "total_terminals": .number(Double(entries.count)),
            "include_exited": .bool(includeExited),
            "limit": .number(Double(limit)),
            "terminals": .array(entries),
            "processes": .array(entries),
        ])
    }

    private func poll(
        id: String,
        projectRoot: URL,
        offset: Int?,
        limit: Int
    ) throws -> NativeJSONValue {
        let process = try requireProcess(id: id, projectRoot: projectRoot)
        let selected: [TerminalLog]
        let requestedOffset = offset.map { max(0, $0) }
        if let offset {
            selected = Array(process.logs.filter { $0.offset >= max(0, offset) }.prefix(limit))
        } else {
            selected = Array(process.logs.suffix(limit))
        }
        let firstAvailableOffset = process.logs.first?.offset ?? process.nextLogOffset
        let truncated = process.logsWereTruncated
            && (process.outputBytesWereTruncated
                || requestedOffset.map { $0 < firstAvailableOffset } ?? true)
        guard case var .object(values) = snapshot(process) else { return .null }
        values["mode"] = .string(offset == nil ? "recent" : "offset")
        values["requested_offset"] = offset.map { .number(Double($0)) } ?? .null
        values["next_offset"] = selected.last.map { .number(Double($0.offset + 1)) } ?? .null
        values["limit"] = .number(Double(limit))
        values["fetched_log_count"] = .number(Double(selected.count))
        values["returned_log_count"] = .number(Double(selected.count))
        values["has_more"] = .bool(offset != nil && process.logs.contains { $0.offset >= (selected.last?.offset ?? -1) + 1 })
        values["truncated"] = .bool(truncated)
        values["truncation"] = .object([
            "truncated": .bool(truncated),
            "first_available_offset": .number(Double(firstAvailableOffset)),
        ])
        values["logs"] = .array(selected.map(\.jsonValue))
        return .object(values)
    }

    private func processWait(
        id: String,
        projectRoot: URL,
        timeoutMilliseconds: Int
    ) async throws -> NativeJSONValue {
        _ = try requireProcess(id: id, projectRoot: projectRoot)
        let started = Date()
        let timedOut = try await waitForExit(id: id, timeoutMilliseconds: timeoutMilliseconds)
        let process = try requireProcess(id: id, projectRoot: projectRoot)
        let output = combinedOutput(process, kinds: ["stdout", "stderr"])
        let waited = Int(Date().timeIntervalSince(started) * 1_000)
        return .object([
            "terminal_id": .string(id),
            "process_id": .string(id),
            "terminal_name": .string(terminalName(process)),
            "status": .string(process.status),
            "wait_status": .string(timedOut ? "timeout" : "exited"),
            "busy": .bool(process.status != "exited"),
            "exited": .bool(process.status == "exited"),
            "completed": .bool(!timedOut),
            "timed_out": .bool(timedOut),
            "finished_by": .string(timedOut ? "timeout" : "exit"),
            "exit_code": process.exitCode.map { .number(Double($0)) } ?? .null,
            "timeout_ms": .number(Double(timeoutMilliseconds)),
            "waited_ms": .number(Double(waited)),
            "output": .string(output.text),
            "output_preview": .string(output.text),
            "output_chars": .number(Double(output.characters)),
            "truncated": .bool(output.truncated),
        ])
    }

    private func write(
        id: String,
        projectRoot: URL,
        data: String,
        submit: Bool
    ) throws -> NativeJSONValue {
        let process = try requireProcess(id: id, projectRoot: projectRoot)
        guard process.status != "exited" else { throw NativeMCPTerminalError.processExited }
        let content = data + (submit ? "\n" : "")
        do {
            try process.input.write(contentsOf: Data(content.utf8))
        } catch {
            throw NativeMCPTerminalError.writeFailed(error.localizedDescription)
        }
        append(kind: "input", content: content, to: id)
        return .object([
            "ok": .bool(true),
            "terminal_id": .string(id),
            "bytes_written": .number(Double(content.utf8.count)),
            "submit": .bool(submit),
        ])
    }

    private func kill(id: String, projectRoot: URL) throws -> NativeJSONValue {
        _ = try requireProcess(id: id, projectRoot: projectRoot)
        cancelProcess(id: id, reason: "terminal killed")
        return .object(["ok": .bool(true), "terminal_id": .string(id), "killed": .bool(true)])
    }

    private func cancelProcess(id: String, reason: String) {
        guard let process = processes[id], process.status != "exited" else { return }
        if process.process.isRunning { process.process.terminate() }
        append(kind: "system", content: "[\(reason)]\n", to: id)
    }

    private func waitForExit(id: String, timeoutMilliseconds: Int) async throws -> Bool {
        let changes = try exitChanges(id: id)
        let timedOut = await withTaskGroup(of: Bool.self) { group in
            group.addTask {
                for await _ in changes { return false }
                return false
            }
            group.addTask {
                do {
                    try await Task.sleep(for: .milliseconds(timeoutMilliseconds))
                    return true
                } catch {
                    return false
                }
            }
            let result = await group.next() ?? false
            group.cancelAll()
            return result
        }
        try Task.checkCancellation()
        return timedOut
    }

    private func exitChanges(id: String) throws -> AsyncStream<Void> {
        guard let process = processes[id] else {
            throw NativeMCPTerminalError.processNotFound
        }
        if process.status == "exited" {
            return AsyncStream { continuation in
                continuation.yield(())
                continuation.finish()
            }
        }

        let subscriberID = UUID()
        let pair = AsyncStream<Void>.makeStream(bufferingPolicy: .bufferingNewest(1))
        process.exitContinuations[subscriberID] = pair.continuation
        pair.continuation.onTermination = { [weak self] _ in
            Task { await self?.removeExitSubscriber(subscriberID, processID: id) }
        }
        return pair.stream
    }

    private func removeExitSubscriber(_ subscriberID: UUID, processID: String) {
        processes[processID]?.exitContinuations.removeValue(forKey: subscriberID)
    }

    private func finish(id: String, exitCode: Int) async {
        guard let process = processes[id] else { return }
        async let stdoutDrained = process.stdoutDrain.wait(timeoutMilliseconds: 1_000)
        async let stderrDrained = process.stderrDrain.wait(timeoutMilliseconds: 1_000)
        let drainsCompleted = await (stdoutDrained, stderrDrained)
        if !drainsCompleted.0 || !drainsCompleted.1 {
            // A descendant may have inherited the pipe and kept it open after
            // the shell exited. Do not block the terminal actor indefinitely;
            // preserve the bytes already drained and report that output may be
            // incomplete.
            process.logsWereTruncated = true
            process.outputBytesWereTruncated = true
            process.output.fileHandleForReading.readabilityHandler = nil
            process.error.fileHandleForReading.readabilityHandler = nil
        }
        flushPendingOutput(kind: "stdout", id: id)
        flushPendingOutput(kind: "stderr", id: id)
        process.status = "exited"
        process.exitCode = exitCode
        process.lastActiveAt = Self.timestamp()
        let exitContinuations = process.exitContinuations.values
        process.exitContinuations.removeAll()
        for continuation in exitContinuations {
            continuation.yield(())
            continuation.finish()
        }
        try? process.input.close()
        try? process.output.fileHandleForReading.close()
        try? process.error.fileHandleForReading.close()
        pruneExitedProcesses()
    }

    private func flushPendingOutput(kind: String, id: String) {
        guard let process = processes[id] else { return }
        let pending = kind == "stderr" ? process.stderrPending : process.stdoutPending
        guard let batch = pending.take() else { return }
        if batch.discarded {
            process.logsWereTruncated = true
            process.outputBytesWereTruncated = true
        }
        if !batch.data.isEmpty {
            append(kind: kind, data: batch.data, to: id)
        }
    }

    private func append(kind: String, data: Data, to id: String) {
        append(kind: kind, content: String(decoding: data, as: UTF8.self), to: id)
    }

    private func append(kind: String, content: String, to id: String) {
        guard let process = processes[id], !content.isEmpty else { return }
        let contentData = Data(content.utf8)
        let retainedData: Data
        if contentData.count > Self.maximumLogEntryBytes {
            retainedData = Data(contentData.suffix(Self.maximumLogEntryBytes))
            process.logsWereTruncated = true
            process.outputBytesWereTruncated = true
        } else {
            retainedData = contentData
        }
        let retainedContent = String(decoding: retainedData, as: UTF8.self)
        let offset = process.nextLogOffset
        process.nextLogOffset += 1
        process.logs.append(.init(
            offset: offset,
            kind: kind,
            content: retainedContent,
            byteCount: retainedData.count,
            createdAt: Self.timestamp()
        ))
        process.retainedLogBytes += retainedData.count

        if process.logs.count > Self.maximumLogs
            || process.retainedLogBytes > Self.maximumLogBytesPerProcess {
            var removeCount = 0
            while process.logs.count - removeCount > 1,
                  (process.logs.count - removeCount > Self.retainedLogTarget
                    || process.retainedLogBytes > Self.retainedLogBytesTarget) {
                process.retainedLogBytes -= process.logs[removeCount].byteCount
                removeCount += 1
            }
            if removeCount > 0 {
                process.logs.removeFirst(removeCount)
                process.logsWereTruncated = true
            }
        }
        process.lastActiveAt = Self.timestamp()
    }

    private func pruneExitedProcesses() {
        let exited = processes.values
            .filter { $0.status == "exited" }
            .sorted { $0.sequence > $1.sequence }
        for process in exited.dropFirst(maximumRetainedExitedProcesses) {
            processes.removeValue(forKey: process.id)
        }
    }

    private func requireProcess(id: String, projectRoot: URL) throws -> ManagedTerminalProcess {
        guard let process = processes[id], sameRoot(process.projectRoot, projectRoot) else {
            throw NativeMCPTerminalError.processNotFound
        }
        return process
    }

    private func matchingProcesses(
        projectRoot: URL,
        includeExited: Bool,
        ownerRunID: String? = nil
    ) -> [ManagedTerminalProcess] {
        processes.values.filter { process in
            sameRoot(process.projectRoot, projectRoot)
                && (includeExited || process.status != "exited")
                && (ownerRunID == nil || process.ownerRunID == ownerRunID)
        }.sorted { $0.sequence > $1.sequence }
    }

    private func validateOwnership(
        name: String,
        arguments: [String: NativeJSONValue],
        ownerRunID: String?
    ) throws {
        guard let ownerRunID else { return }
        let processID: String?
        if name == "process",
           arguments.string("action")?.lowercased() == "list" {
            processID = nil
        } else {
            processID = arguments.string("terminal_id")
        }
        guard let processID else {
            guard name == "get_recent_logs" || name == "process_list" || name == "process" else {
                throw NativeMCPTerminalError.processNotFound
            }
            return
        }
        guard processes[processID]?.ownerRunID == ownerRunID else {
            throw NativeMCPTerminalError.processNotFound
        }
    }

    private func sameRoot(_ lhs: URL, _ rhs: URL) -> Bool {
        lhs.standardizedFileURL.resolvingSymlinksInPath().path
            == rhs.standardizedFileURL.resolvingSymlinksInPath().path
    }

    private func snapshot(_ process: ManagedTerminalProcess) -> NativeJSONValue {
        let output = combinedOutput(process, kinds: ["stdout", "stderr"], maximum: 1_200)
        let busy = process.status != "exited"
        return .object([
            "terminal_id": .string(process.id),
            "process_id": .string(process.id),
            "terminal_name": .string(terminalName(process)),
            "status": .string(process.status),
            "process_status": .string(busy ? "running" : "exited"),
            "busy": .bool(busy),
            "has_session": .bool(true),
            "command": .string(process.command),
            "pid": process.process.processIdentifier > 0 ? .number(Double(process.process.processIdentifier)) : .null,
            "started_at": .string(process.startedAt),
            "uptime_seconds": .null,
            "cwd": .string(displayPath(process.cwd, relativeTo: process.projectRoot)),
            "project_id": .null,
            "last_active_at": .string(process.lastActiveAt),
            "output_preview": .string(output.text),
            "output_tail": .string(output.text),
            "output_tail_chars": .number(Double(output.characters)),
            "exit_code": process.exitCode.map { .number(Double($0)) } ?? .null,
        ])
    }

    private func combinedOutput(
        _ process: ManagedTerminalProcess,
        kinds: Set<String>,
        maximum: Int? = nil
    ) -> (text: String, characters: Int, truncated: Bool) {
        let full = process.logs.filter { kinds.contains($0.kind) }.map(\.content).joined()
        let characters = full.count
        let maximum = maximum ?? Self.maximumOutputCharacters
        guard characters > maximum else {
            return (full, characters, process.outputBytesWereTruncated)
        }
        return (String(full.suffix(maximum)), characters, true)
    }

    private func terminalName(_ process: ManagedTerminalProcess) -> String {
        let path = displayPath(process.cwd, relativeTo: process.projectRoot)
        return path == "." ? process.projectRoot.lastPathComponent : process.cwd.lastPathComponent
    }

    private func displayPath(_ url: URL, relativeTo root: URL) -> String {
        let resolvedURL = url.standardizedFileURL.resolvingSymlinksInPath()
        let resolvedRoot = root.standardizedFileURL.resolvingSymlinksInPath()
        guard resolvedURL.path != resolvedRoot.path else { return "." }
        let prefix = resolvedRoot.path.hasSuffix("/") ? resolvedRoot.path : resolvedRoot.path + "/"
        return resolvedURL.path.hasPrefix(prefix) ? String(resolvedURL.path.dropFirst(prefix.count)) : resolvedURL.path
    }

    private func requiredID(_ values: [String: NativeJSONValue]) throws -> String {
        guard let id = values.string("terminal_id")?.trimmingCharacters(in: .whitespacesAndNewlines), !id.isEmpty else {
            throw NativeMCPTerminalError.invalidArguments("缺少参数：terminal_id")
        }
        return id
    }

    private func timeoutMilliseconds(_ values: [String: NativeJSONValue]) -> Int {
        if let milliseconds = values.integer("timeout_ms") {
            return clamp(milliseconds, 1_000, Self.maximumWaitMilliseconds)
        }
        if let seconds = values.integer("timeout") {
            return clamp(seconds * 1_000, 1_000, Self.maximumWaitMilliseconds)
        }
        return 30_000
    }

    private func clamp(_ value: Int, _ minimum: Int, _ maximum: Int) -> Int {
        Swift.min(Swift.max(value, minimum), maximum)
    }

    private func resultScope(_ count: Int) -> String {
        count > 1 ? "multiple_terminals" : count == 1 ? "single_terminal" : "no_terminal"
    }

    private static func timestamp() -> String { ISO8601DateFormatter().string(from: Date()) }

    private static func processReadProperties(defaultLimit: Int) -> [String: NativeJSONValue] {
        [
            "terminal_id": .object(["type": .string("string")]),
            "offset": integerSchema(minimum: 0, maximum: nil),
            "limit": .object([
                "type": .string("integer"),
                "minimum": .number(1),
                "maximum": .number(200),
                "default": .number(Double(defaultLimit)),
            ]),
        ]
    }

    private static func integerSchema(minimum: Int, maximum: Int?) -> NativeJSONValue {
        var values: [String: NativeJSONValue] = [
            "type": .string("integer"),
            "minimum": .number(Double(minimum)),
        ]
        if let maximum { values["maximum"] = .number(Double(maximum)) }
        return .object(values)
    }

    private static func definition(
        name: String,
        description: String,
        properties: [String: NativeJSONValue],
        required: [String]
    ) -> NativeJSONValue {
        .object([
            "name": .string(name),
            "description": .string(description),
            "inputSchema": .object([
                "type": .string("object"),
                "properties": .object(properties),
                "required": .array(required.map(NativeJSONValue.string)),
                "additionalProperties": .bool(false),
            ]),
        ])
    }
}

private final class ManagedTerminalProcess: @unchecked Sendable {
    let id: String
    let sequence: Int
    let command: String
    let cwd: URL
    let projectRoot: URL
    let ownerRunID: String?
    let process: Process
    let input: FileHandle
    let output: Pipe
    let error: Pipe
    var status: String
    var exitCode: Int?
    let startedAt: String
    var lastActiveAt: String
    var logs: [TerminalLog]
    var nextLogOffset: Int
    var retainedLogBytes: Int
    var logsWereTruncated: Bool
    var outputBytesWereTruncated: Bool
    var exitContinuations: [UUID: AsyncStream<Void>.Continuation]
    let stdoutPending = NativeCoalescingProcessOutput(
        maximumBytes: NativeMCPTerminalStore.maximumPendingOutputBytes
    )
    let stderrPending = NativeCoalescingProcessOutput(
        maximumBytes: NativeMCPTerminalStore.maximumPendingOutputBytes
    )
    let stdoutDrain = NativeProcessPipeDrainSignal()
    let stderrDrain = NativeProcessPipeDrainSignal()

    init(
        id: String,
        sequence: Int,
        command: String,
        cwd: URL,
        projectRoot: URL,
        ownerRunID: String?,
        process: Process,
        input: FileHandle,
        output: Pipe,
        error: Pipe,
        status: String,
        exitCode: Int?,
        startedAt: String,
        lastActiveAt: String,
        logs: [TerminalLog],
        nextLogOffset: Int,
        retainedLogBytes: Int,
        logsWereTruncated: Bool,
        outputBytesWereTruncated: Bool,
        exitContinuations: [UUID: AsyncStream<Void>.Continuation] = [:]
    ) {
        self.id = id
        self.sequence = sequence
        self.command = command
        self.cwd = cwd
        self.projectRoot = projectRoot
        self.ownerRunID = ownerRunID
        self.process = process
        self.input = input
        self.output = output
        self.error = error
        self.status = status
        self.exitCode = exitCode
        self.startedAt = startedAt
        self.lastActiveAt = lastActiveAt
        self.logs = logs
        self.nextLogOffset = nextLogOffset
        self.retainedLogBytes = retainedLogBytes
        self.logsWereTruncated = logsWereTruncated
        self.outputBytesWereTruncated = outputBytesWereTruncated
        self.exitContinuations = exitContinuations
    }
}

/// Coordinates process termination with the final pipe-reader callback. A
/// process exit can race the readability handler; waiting for EOF prevents a
/// foreground result from being returned before its last output batch is
/// accounted for.
private final class NativeProcessPipeDrainSignal: @unchecked Sendable {
    private let lock = NSLock()
    private var completed = false
    private var continuations: [UUID: AsyncStream<Void>.Continuation] = [:]

    func complete() {
        lock.lock()
        guard !completed else {
            lock.unlock()
            return
        }
        completed = true
        let pending = Array(continuations.values)
        continuations.removeAll()
        lock.unlock()
        for continuation in pending {
            continuation.yield(())
            continuation.finish()
        }
    }

    func wait(timeoutMilliseconds: Int) async -> Bool {
        let changes = stream()
        return await withTaskGroup(of: Bool.self) { group in
            group.addTask {
                for await _ in changes { return true }
                return true
            }
            group.addTask {
                try? await Task.sleep(for: .milliseconds(timeoutMilliseconds))
                return false
            }
            let result = await group.next() ?? false
            group.cancelAll()
            return result
        }
    }

    private func stream() -> AsyncStream<Void> {
        lock.lock()
        if completed {
            lock.unlock()
            return AsyncStream { continuation in
                continuation.yield(())
                continuation.finish()
            }
        }
        let subscriberID = UUID()
        let pair = AsyncStream<Void>.makeStream(bufferingPolicy: .bufferingNewest(1))
        continuations[subscriberID] = pair.continuation
        pair.continuation.onTermination = { [weak self] _ in
            self?.remove(subscriberID)
        }
        lock.unlock()
        return pair.stream
    }

    private func remove(_ subscriberID: UUID) {
        lock.lock()
        continuations.removeValue(forKey: subscriberID)
        lock.unlock()
    }
}

/// Keeps pipe readers non-blocking without creating one actor task per output
/// fragment. Only one delivery task may be outstanding and queued bytes are
/// bounded, so a command that writes faster than the actor can consume cannot
/// create an unbounded task/data backlog.
private final class NativeCoalescingProcessOutput: @unchecked Sendable {
    private let lock = NSLock()
    private let maximumBytes: Int
    private var storage = Data()
    private var deliveryScheduled = false
    private var discarded = false

    init(maximumBytes: Int) {
        self.maximumBytes = max(1, maximumBytes)
    }

    /// Returns true only when the caller must schedule a delivery task.
    func append(_ data: Data) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        if !data.isEmpty {
            storage.append(data)
            if storage.count > maximumBytes {
                storage = Data(storage.suffix(maximumBytes))
                discarded = true
            }
        }
        guard !deliveryScheduled, !storage.isEmpty else { return false }
        deliveryScheduled = true
        return true
    }

    func take() -> (data: Data, discarded: Bool)? {
        lock.lock()
        defer { lock.unlock() }
        guard deliveryScheduled else { return nil }
        let result = (storage, discarded)
        storage.removeAll(keepingCapacity: true)
        discarded = false
        deliveryScheduled = false
        return result
    }
}

private struct TerminalLog: Sendable {
    let offset: Int
    let kind: String
    let content: String
    let byteCount: Int
    let createdAt: String

    var jsonValue: NativeJSONValue {
        .object([
            "offset": .number(Double(offset)),
            "kind": .string(kind),
            "content": .string(content),
            "created_at": .string(createdAt),
        ])
    }
}

private enum NativeMCPTerminalError: LocalizedError {
    case unsupportedTool(String)
    case invalidArguments(String)
    case launchFailed(String)
    case processNotFound
    case processExited
    case writeFailed(String)

    var errorDescription: String? {
        switch self {
        case let .unsupportedTool(name): "不支持的终端工具：\(name)"
        case let .invalidArguments(message): message
        case let .launchFailed(message): "命令启动失败：\(message)"
        case .processNotFound: "当前项目中没有找到该命令进程"
        case .processExited: "命令进程已经结束"
        case let .writeFailed(message): "写入命令进程失败：\(message)"
        }
    }
}

private extension Dictionary where Key == String, Value == NativeJSONValue {
    func string(_ key: String) -> String? {
        guard case let .string(value)? = self[key] else { return nil }
        return value
    }

    func bool(_ key: String) -> Bool? {
        guard case let .bool(value)? = self[key] else { return nil }
        return value
    }

    func integer(_ key: String) -> Int? {
        guard case let .number(value)? = self[key] else { return nil }
        return Int(value)
    }

    func array(_ key: String) -> [NativeJSONValue] {
        guard case let .array(value)? = self[key] else { return [] }
        return value
    }
}

private extension NativeJSONValue {
    var content: String? {
        guard case let .object(values) = self, case let .string(content)? = values["content"] else { return nil }
        return content
    }
}
