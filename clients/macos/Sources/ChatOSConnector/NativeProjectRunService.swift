import ChatOSCore
import Foundation

public actor NativeProjectRunService: ProjectRunServicing {
    private static let maximumPreferencesBytes = 4 * 1_024 * 1_024
    private let connector: NativeLocalConnectorService
    private let preferencesURL: URL
    private let maximumRetainedExitedInstances: Int
    private var rootsByProjectID: [String: String] = [:]
    private var analyses: [String: NativeProjectRunAnalysis] = [:]
    private var preferences: NativeProjectRunPreferences
    private var processes: [String: NativeProjectProcess] = [:]
    private var stateChangeContinuations: [
        String: [UUID: AsyncStream<Void>.Continuation]
    ] = [:]

    public init(
        connector: NativeLocalConnectorService,
        preferencesURL: URL,
        maximumRetainedExitedInstances: Int = 100
    ) {
        self.connector = connector
        self.preferencesURL = preferencesURL
        self.maximumRetainedExitedInstances = max(1, maximumRetainedExitedInstances)
        self.preferences = (try? Self.loadPreferences(from: preferencesURL)) ?? .init()
    }

    public func updateProjects(_ projects: [WorkspaceProject]) {
        rootsByProjectID = Dictionary(
            uniqueKeysWithValues: projects.compactMap { project in
                project.rootPath.map { (project.id, $0) }
            }
        )
        analyses = analyses.filter { rootsByProjectID[$0.key] != nil }
    }

    public func fetchCatalog(projectID: String) async throws -> ProjectRunCatalog {
        try await catalog(projectID: projectID, force: false)
    }

    public func analyze(projectID: String) async throws -> ProjectRunCatalog {
        try await catalog(projectID: projectID, force: true)
    }

    public func changes(projectID: String) async -> AsyncStream<Void> {
        let subscriberID = UUID()
        let pair = AsyncStream<Void>.makeStream(bufferingPolicy: .bufferingNewest(1))
        var projectContinuations = stateChangeContinuations[projectID] ?? [:]
        projectContinuations[subscriberID] = pair.continuation
        stateChangeContinuations[projectID] = projectContinuations
        pair.continuation.onTermination = { [weak self] _ in
            Task {
                await self?.removeStateChangeSubscriber(
                    subscriberID,
                    projectID: projectID
                )
            }
        }
        return pair.stream
    }

    public func fetchState(projectID: String) async throws -> ProjectRunState {
        let projectInstances = processes.values
            .filter { $0.projectID == projectID }
            .map(\.domainModel)
            .sorted { ($0.startedAt ?? .distantPast) > ($1.startedAt ?? .distantPast) }
        return .init(
            projectID: projectID,
            status: projectInstances.contains(where: \.isRunning) ? "running" : "idle",
            isBusy: false,
            isRunning: projectInstances.contains(where: \.isRunning),
            instances: projectInstances
        )
    }

    public func fetchEnvironment(projectID: String) async throws -> ProjectRunEnvironment {
        let catalog = try await catalog(projectID: projectID, force: false)
        guard let analysis = analyses[projectID] else { throw NativeProjectRunError.projectDirectoryUnavailable }
        let selection = preferences.projects[projectID] ?? .init()
        let target = catalog.targets.first(where: { $0.id == catalog.defaultTargetID }) ?? catalog.targets.first
        let missing = target?.requiredToolchains.filter { kind in
            let selected = selection.selectedToolchains[kind]
            if let selected, !selected.isEmpty { return !FileManager.default.isExecutableFile(atPath: selected) }
            return analysis.toolchainOptions[kind]?.isEmpty != false
        } ?? []
        let issues = missing.map { kind in
            ProjectRunValidationIssue(
                kind: "error",
                message: "没有发现可用的 \(kind) 工具链",
                targetID: target?.id,
                targetLabel: target?.label,
                path: nil,
                hint: "请安装 \(kind)，或在工具链设置中选择可执行文件。"
            )
        }
        return .init(
            toolchainOptions: analysis.toolchainOptions,
            configurationFiles: analysis.configurationFiles,
            validationIssues: issues,
            selectedToolchains: selection.selectedToolchains,
            customToolchains: selection.customToolchains,
            environmentVariables: selection.environmentVariables,
            terminalUIEnabled: false
        )
    }

    public func updateEnvironment(
        projectID: String,
        selectedToolchains: [String: String],
        customToolchains: [String: ProjectRunCustomToolchain],
        environmentVariables: [String: String]
    ) async throws -> ProjectRunEnvironment {
        var selection = preferences.projects[projectID] ?? .init()
        selection.selectedToolchains = selectedToolchains.filter { !$0.value.isEmpty }
        selection.customToolchains = customToolchains
        selection.environmentVariables = environmentVariables
        preferences.projects[projectID] = selection
        try persistPreferences()
        return try await fetchEnvironment(projectID: projectID)
    }

    public func setDefaultTarget(projectID: String, targetID: String) async throws -> ProjectRunCatalog {
        var catalog = try await catalog(projectID: projectID, force: false)
        guard catalog.targets.contains(where: { $0.id == targetID }) else {
            throw NativeProjectRunError.targetNotFound
        }
        var selection = preferences.projects[projectID] ?? .init()
        selection.defaultTargetID = targetID
        preferences.projects[projectID] = selection
        try persistPreferences()
        catalog.defaultTargetID = targetID
        catalog.targets = catalog.targets.map { target in
            var target = target
            target.isDefault = target.id == targetID
            return target
        }
        return catalog
    }

    public func start(projectID: String, targetID: String) async throws {
        let catalog = try await catalog(projectID: projectID, force: false)
        guard let target = catalog.targets.first(where: { $0.id == targetID }) else {
            throw NativeProjectRunError.targetNotFound
        }
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/zsh")
        process.arguments = ["-lc", target.command]
        process.currentDirectoryURL = URL(fileURLWithPath: target.cwd, isDirectory: true)
        let outputPipe = Pipe()
        let logBuffer = NativeProjectLogBuffer(initialText: "$ \(target.command)\n")
        NativeProcessPipeReader.install(
            on: outputPipe.fileHandleForReading,
            onData: logBuffer.append
        )
        process.standardOutput = outputPipe
        process.standardError = outputPipe
        process.standardInput = FileHandle.nullDevice
        process.environment = launchEnvironment(projectID: projectID)
        let instanceID = UUID().uuidString
        let instance = NativeProjectProcess(
            id: instanceID,
            projectID: projectID,
            targetID: targetID,
            name: target.label,
            cwd: target.cwd,
            process: process,
            outputPipe: outputPipe,
            logBuffer: logBuffer
        )
        process.terminationHandler = { [weak self] process in
            let status = process.terminationStatus
            Task { await self?.processTerminated(id: instanceID, exitCode: status) }
        }
        do {
            try process.run()
            instance.status = "running"
            instance.isRunning = true
            processes[instanceID] = instance
            notifyStateChanged(projectID: projectID)
        } catch {
            throw NativeProjectRunError.processLaunchFailed(error.localizedDescription)
        }
    }

    public func stop(instanceID: String) async throws {
        guard let instance = processes[instanceID] else { throw NativeProjectRunError.instanceNotFound }
        if instance.process.isRunning {
            instance.process.interrupt()
            instance.status = "stopping"
        } else {
            instance.isRunning = false
            instance.status = "exited"
        }
        notifyStateChanged(projectID: instance.projectID)
    }

    public func delete(instanceID: String) async throws {
        guard let instance = processes.removeValue(forKey: instanceID) else {
            throw NativeProjectRunError.instanceNotFound
        }
        if instance.process.isRunning { instance.process.terminate() }
        notifyStateChanged(projectID: instance.projectID)
    }

    private func catalog(projectID: String, force: Bool) async throws -> ProjectRunCatalog {
        if !force, let analysis = analyses[projectID] {
            return catalog(projectID: projectID, analysis: analysis)
        }
        guard let rootPath = rootsByProjectID[projectID] else {
            throw NativeProjectRunError.projectNotRegistered
        }
        let resolved = try await connector.resolveProjectPath(rootPath)
        let analysis = try await Task.detached {
            try NativeProjectRunAnalyzer().analyze(root: resolved.absoluteURL)
        }.value
        analyses[projectID] = analysis
        return catalog(projectID: projectID, analysis: analysis)
    }

    private func catalog(projectID: String, analysis: NativeProjectRunAnalysis) -> ProjectRunCatalog {
        let selected = preferences.projects[projectID]?.defaultTargetID
        let defaultID = selected.flatMap { id in analysis.targets.contains(where: { $0.id == id }) ? id : nil }
            ?? analysis.targets.first?.id
        let targets = analysis.targets.map { target in
            var target = target
            target.isDefault = target.id == defaultID
            return target
        }
        return .init(
            projectID: projectID,
            status: targets.isEmpty ? "empty" : "ready",
            defaultTargetID: defaultID,
            targets: targets,
            errorMessage: nil
        )
    }

    private func launchEnvironment(projectID: String) -> [String: String] {
        var environment = ProcessInfo.processInfo.environment
        let selection = preferences.projects[projectID] ?? .init()
        for (key, value) in selection.environmentVariables { environment[key] = value }
        let directories = selection.selectedToolchains.values
            .map { URL(fileURLWithPath: $0).deletingLastPathComponent().path }
        if !directories.isEmpty {
            environment["PATH"] = (directories + [environment["PATH"] ?? "/usr/bin:/bin"]).joined(separator: ":")
        }
        return environment
    }

    private func processTerminated(id: String, exitCode: Int32) {
        guard let instance = processes[id] else { return }
        instance.outputPipe.fileHandleForReading.readabilityHandler = nil
        let remainingOutput = instance.outputPipe.fileHandleForReading.readDataToEndOfFile()
        if !remainingOutput.isEmpty { instance.logBuffer.append(remainingOutput) }
        try? instance.outputPipe.fileHandleForReading.close()
        instance.logBuffer.append("\n[进程已退出，代码 \(exitCode)]\n")
        instance.exitCode = exitCode
        instance.isRunning = false
        instance.status = exitCode == 0 ? "exited" : "failed"
        notifyStateChanged(projectID: instance.projectID)
        pruneExitedInstances(projectID: instance.projectID)
    }

    private func removeStateChangeSubscriber(_ subscriberID: UUID, projectID: String) {
        stateChangeContinuations[projectID]?.removeValue(forKey: subscriberID)
        if stateChangeContinuations[projectID]?.isEmpty == true {
            stateChangeContinuations.removeValue(forKey: projectID)
        }
    }

    private func notifyStateChanged(projectID: String) {
        guard let continuations = stateChangeContinuations[projectID] else { return }
        for continuation in continuations.values {
            continuation.yield(())
        }
    }

    private func pruneExitedInstances(projectID: String) {
        let exited = processes.values
            .filter { $0.projectID == projectID && !$0.isRunning }
            .sorted { $0.startedAt > $1.startedAt }
        for instance in exited.dropFirst(maximumRetainedExitedInstances) {
            processes.removeValue(forKey: instance.id)
        }
    }

    private func persistPreferences() throws {
        let data = try JSONEncoder().encode(preferences)
        guard data.count <= Self.maximumPreferencesBytes else {
            throw NativeBoundedFileReadError.fileTooLarge(
                maximumBytes: Self.maximumPreferencesBytes
            )
        }
        try FileManager.default.createDirectory(
            at: preferencesURL.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try data.write(to: preferencesURL, options: [.atomic])
    }

    private static func loadPreferences(from url: URL) throws -> NativeProjectRunPreferences {
        try JSONDecoder().decode(
            NativeProjectRunPreferences.self,
            from: NativeBoundedFileReader.read(
                url,
                maximumBytes: maximumPreferencesBytes
            )
        )
    }
}

private struct NativeProjectRunPreferences: Codable, Sendable {
    var projects: [String: NativeProjectRunSelection] = [:]
}

private struct NativeProjectRunSelection: Codable, Sendable {
    var defaultTargetID: String?
    var selectedToolchains: [String: String] = [:]
    var customToolchains: [String: ProjectRunCustomToolchain] = [:]
    var environmentVariables: [String: String] = [:]
}

private final class NativeProjectProcess: @unchecked Sendable {
    let id: String
    let projectID: String
    let targetID: String
    let name: String
    let cwd: String
    let process: Process
    let outputPipe: Pipe
    let logBuffer: NativeProjectLogBuffer
    let startedAt: Date
    var status = "starting"
    var isRunning = false
    var exitCode: Int32?

    init(
        id: String,
        projectID: String,
        targetID: String,
        name: String,
        cwd: String,
        process: Process,
        outputPipe: Pipe,
        logBuffer: NativeProjectLogBuffer
    ) {
        self.id = id
        self.projectID = projectID
        self.targetID = targetID
        self.name = name
        self.cwd = cwd
        self.process = process
        self.outputPipe = outputPipe
        self.logBuffer = logBuffer
        self.startedAt = Date()
    }

    var domainModel: ProjectRunInstance {
        .init(
            id: id,
            name: name,
            cwd: cwd,
            status: status,
            isBusy: false,
            isRunning: isRunning && process.isRunning,
            log: logBuffer.snapshot(),
            startedAt: startedAt,
            exitCode: exitCode
        )
    }
}

private final class NativeProjectLogBuffer: @unchecked Sendable {
    private let lock = NSLock()
    private var data: Data
    private let maximumBytes = 512 * 1_024

    init(initialText: String) {
        self.data = Data(initialText.utf8)
    }

    func append(_ value: Data) {
        lock.lock()
        data.append(value)
        if data.count > maximumBytes {
            data = Data(data.suffix(maximumBytes))
        }
        lock.unlock()
    }

    func append(_ value: String) {
        append(Data(value.utf8))
    }

    func snapshot() -> String {
        lock.lock()
        let value = String(decoding: data, as: UTF8.self)
        lock.unlock()
        return value
    }
}
