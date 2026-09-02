import ChatOSCore
import Foundation

actor NativePluginRuntimeStore {
    struct Identity: Sendable {
        var runID: String
        var pluginID: String
        var releaseID: String
        var version: String
        var artifactSHA256: String
        var componentKey: String
        var adapterSessionID: String
        var projectID: String? = nil
        var requiresExclusiveExecution = false
    }

    struct VisualDescriptor: Sendable {
        var identity: Identity
        var displayName: String
        var visualSessionURL: URL
        var owner: PluginVisualSessionOwner
        var ownerBoundAt: Date
    }

    private struct Session {
        var identity: Identity
        var client: NativePluginStdioClient
        var tools: [NativeJSONValue]
        var toolNames: Set<String>
        var permissionSnapshot: Set<String>
        var displayName: String
        var visualSessionURL: URL
        var artifactURL: URL
        var projectRootURL: URL?
        var workspaceID: String?
        var owner: PluginVisualSessionOwner?
        var ownerBoundAt: Date?
        var activeInvocationIDs: Set<String> = []
        var visualFrameSequence: UInt64 = 0
    }

    private struct ComputerUseLeaseWaiter {
        var id: UUID
        var adapterSessionID: String
        var continuation: CheckedContinuation<Bool, Never>
    }

    private var sessions: [String: Session] = [:]
    private var computerUseLeaseOwner: String?
    private var computerUseLeaseWaiters: [ComputerUseLeaseWaiter] = []
    private let browserVisualRefreshTimeout: Duration
    private static let browserVisualRefreshTools: Set<String> = [
        "browser_screenshot",
        "browser_navigate",
        "browser_tab_new",
        "browser_tab_switch",
        "browser_tab_close",
        "browser_click",
        "browser_type",
        "browser_fill",
        "browser_key",
        "browser_scroll",
        "browser_upload",
        "browser_handle_dialog",
    ]

    init(browserVisualRefreshTimeout: Duration = .seconds(12)) {
        self.browserVisualRefreshTimeout = browserVisualRefreshTimeout
    }

    func insert(
        identity: Identity,
        client: NativePluginStdioClient,
        tools: [NativeJSONValue],
        permissionSnapshot: Set<String>,
        displayName: String,
        visualSessionURL: URL,
        artifactURL: URL,
        projectRootURL: URL?,
        workspaceID: String?
    ) async {
        if let previous = sessions.removeValue(forKey: identity.adapterSessionID) {
            releaseComputerUseLease(adapterSessionID: identity.adapterSessionID)
            await previous.client.terminate()
        }
        sessions[identity.adapterSessionID] = Session(
            identity: identity,
            client: client,
            tools: tools,
            toolNames: Set(tools.compactMap { $0.jsonObject?["name"]?.jsonString }),
            permissionSnapshot: permissionSnapshot,
            displayName: displayName,
            visualSessionURL: visualSessionURL,
            artifactURL: artifactURL,
            projectRootURL: projectRootURL,
            workspaceID: workspaceID
        )
    }

    func validate(
        adapterSessionID: String,
        pluginID: String,
        releaseID: String,
        artifactSHA256: String,
        componentKey: String,
        workspaceID: String?,
        projectID: String? = nil
    ) throws -> Identity {
        guard let session = sessions[adapterSessionID] else {
            throw NativePluginRuntimeError.sessionNotFound
        }
        let identity = session.identity
        guard identity.pluginID == pluginID,
              identity.releaseID == releaseID,
              identity.artifactSHA256 == artifactSHA256,
              identity.componentKey == componentKey,
              session.workspaceID == workspaceID,
              identity.projectID == projectID else {
            throw NativePluginRuntimeError.invalidRequest("Plugin 请求与已准备会话不匹配")
        }
        return identity
    }

    func validateScopeIfPresent(
        adapterSessionID: String,
        workspaceID: String?,
        projectID: String? = nil
    ) throws {
        guard let session = sessions[adapterSessionID] else { return }
        guard session.workspaceID == workspaceID,
              session.identity.projectID == projectID else {
            throw NativePluginRuntimeError.invalidRequest("Plugin 请求与已准备会话不匹配")
        }
    }

    func projectRootURL(adapterSessionID: String) -> URL? {
        sessions[adapterSessionID]?.projectRootURL
    }

    func bindOwner(_ owner: PluginVisualSessionOwner, adapterSessionID: String) {
        guard var session = sessions[adapterSessionID] else { return }
        session.owner = owner
        session.ownerBoundAt = Date()
        sessions[adapterSessionID] = session
    }

    func toolDefinition(name: String, adapterSessionID: String) -> NativeJSONValue? {
        sessions[adapterSessionID]?.tools.first {
            $0.jsonObject?["name"]?.jsonString == name
        }
    }

    func grantedPermissions(adapterSessionID: String) -> Set<String> {
        sessions[adapterSessionID]?.permissionSnapshot ?? []
    }

    func registerArtifacts(
        adapterSessionID: String,
        result: NativeJSONValue,
        ownerUserID: String,
        deviceID: String,
        workspaceID: String?,
        toolName: String
    ) throws -> NativeJSONValue {
        guard let session = sessions[adapterSessionID] else {
            throw NativePluginRuntimeError.sessionNotFound
        }
        return try NativePluginArtifactRegistrar.register(
            result: result,
            identity: session.identity,
            ownerUserID: ownerUserID,
            deviceID: deviceID,
            workspaceID: workspaceID,
            workspaceRootURL: session.projectRootURL,
            artifactRootURL: session.artifactURL,
            permissionSnapshot: session.permissionSnapshot,
            toolName: toolName
        )
    }

    func call(
        adapterSessionID: String,
        invocationID: String,
        toolName: String,
        arguments: NativeJSONValue,
        timeout: Duration
    ) async throws -> NativeJSONValue {
        guard let session = sessions[adapterSessionID], session.toolNames.contains(toolName) else {
            throw NativePluginRuntimeError.invalidRequest("Plugin MCP 没有发布这个工具")
        }
        let client = session.client
        let componentKey = session.identity.componentKey
        let isBrowser = componentKey.localizedCaseInsensitiveContains("browser")
        let isComputerUseVisual = componentKey.localizedCaseInsensitiveContains("computer")
        let requiresExclusiveExecution = session.identity.requiresExclusiveExecution
        sessions[adapterSessionID]?.activeInvocationIDs.insert(invocationID)
        defer {
            sessions[adapterSessionID]?.activeInvocationIDs.remove(invocationID)
        }
        if requiresExclusiveExecution {
            guard await acquireComputerUseLease(adapterSessionID: adapterSessionID) else {
                throw NativePluginRuntimeError.cancelled
            }
        }
        do {
            let result = try await client.callTool(
                name: toolName,
                arguments: arguments,
                timeout: timeout
            )
            if isBrowser {
                await refreshBrowserVisual(
                    adapterSessionID: adapterSessionID,
                    toolName: toolName,
                    arguments: arguments,
                    result: result
                )
            } else if isComputerUseVisual {
                refreshComputerUseVisual(
                    adapterSessionID: adapterSessionID,
                    arguments: arguments,
                    result: result
                )
            }
            return result
        } catch {
            if requiresExclusiveExecution,
               sessions[adapterSessionID]?.activeInvocationIDs == [invocationID] {
                releaseComputerUseLease(adapterSessionID: adapterSessionID)
            }
            throw error
        }
    }

    func cancel(adapterSessionID: String, invocationID: String?) async -> String {
        guard let session = sessions[adapterSessionID] else { return "invocation_not_found" }
        if let invocationID {
            guard session.activeInvocationIDs.contains(invocationID) else {
                return "invocation_not_found"
            }
            sessions.removeValue(forKey: adapterSessionID)
            releaseComputerUseLease(adapterSessionID: adapterSessionID)
            await session.client.terminate()
            try? FileManager.default.removeItem(at: session.visualSessionURL)
            return "cancelled"
        }
        sessions.removeValue(forKey: adapterSessionID)
        releaseComputerUseLease(adapterSessionID: adapterSessionID)
        await session.client.terminate()
        try? FileManager.default.removeItem(at: session.visualSessionURL)
        return "cancelled"
    }

    func visualDescriptors() -> [VisualDescriptor] {
        sessions.values.compactMap { session in
            guard let owner = session.owner, let ownerBoundAt = session.ownerBoundAt else { return nil }
            return VisualDescriptor(
                identity: session.identity,
                displayName: session.displayName,
                visualSessionURL: session.visualSessionURL,
                owner: owner,
                ownerBoundAt: ownerBoundAt
            )
        }
    }

    func terminateAll() async {
        let active = sessions.values
        sessions.removeAll()
        computerUseLeaseOwner = nil
        let waiters = computerUseLeaseWaiters
        computerUseLeaseWaiters.removeAll()
        for waiter in waiters {
            waiter.continuation.resume(returning: false)
        }
        for session in active {
            await session.client.terminate()
        }
    }

    private func refreshBrowserVisual(
        adapterSessionID: String,
        toolName: String,
        arguments: NativeJSONValue,
        result: NativeJSONValue
    ) async {
        guard let session = sessions[adapterSessionID] else { return }
        guard session.permissionSnapshot.contains("browser.file.transfer"),
              session.toolNames.contains("browser_screenshot"),
              Self.browserVisualRefreshTools.contains(toolName),
              toolName != "browser_session_close" else {
            return
        }
        let screenshotResult: NativeJSONValue
        if toolName == "browser_screenshot" {
            screenshotResult = result
        } else {
            guard let value = try? await session.client.callToolBestEffort(
                name: "browser_screenshot",
                arguments: .object([
                    "full_page": .bool(false),
                ]),
                timeout: browserVisualRefreshTimeout
            ) else { return }
            screenshotResult = value
        }
        guard let frame = NativeBrowserVisualBridge.captureFrame(
            from: screenshotResult,
            artifactRootURL: session.artifactURL
        ) else { return }
        guard var currentSession = sessions[adapterSessionID] else { return }
        currentSession.visualFrameSequence += 1
        let sequence = currentSession.visualFrameSequence
        sessions[adapterSessionID] = currentSession
        try? NativeBrowserVisualBridge.publish(
            frame: frame,
            adapterSessionID: currentSession.identity.adapterSessionID,
            visualSessionURL: currentSession.visualSessionURL,
            sequence: sequence,
            target: NativeBrowserVisualBridge.targetDescription(
                arguments: arguments,
                result: result
            )
        )
    }

    private func refreshComputerUseVisual(
        adapterSessionID: String,
        arguments: NativeJSONValue,
        result: NativeJSONValue
    ) {
        guard let frame = NativeComputerUseVisualBridge.captureFrame(from: result) else { return }
        guard var session = sessions[adapterSessionID] else { return }
        session.visualFrameSequence += 1
        let sequence = session.visualFrameSequence
        sessions[adapterSessionID] = session
        try? NativeComputerUseVisualBridge.publish(
            frame: frame,
            adapterSessionID: session.identity.adapterSessionID,
            visualSessionURL: session.visualSessionURL,
            sequence: sequence,
            targetApplication: NativeComputerUseVisualBridge.targetApplication(arguments: arguments)
        )
    }

    private func acquireComputerUseLease(adapterSessionID: String) async -> Bool {
        guard sessions[adapterSessionID] != nil else { return false }
        if computerUseLeaseOwner == nil {
            computerUseLeaseOwner = adapterSessionID
            return true
        }
        if computerUseLeaseOwner == adapterSessionID {
            return true
        }

        let waiterID = UUID()
        return await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                computerUseLeaseWaiters.append(.init(
                    id: waiterID,
                    adapterSessionID: adapterSessionID,
                    continuation: continuation
                ))
            }
        } onCancel: {
            Task { [weak self] in
                await self?.cancelComputerUseLeaseWaiter(waiterID)
            }
        }
    }

    private func cancelComputerUseLeaseWaiter(_ waiterID: UUID) {
        guard let index = computerUseLeaseWaiters.firstIndex(where: { $0.id == waiterID }) else {
            return
        }
        let waiter = computerUseLeaseWaiters.remove(at: index)
        waiter.continuation.resume(returning: false)
    }

    private func releaseComputerUseLease(adapterSessionID: String) {
        let cancelledWaiters = computerUseLeaseWaiters.filter {
            $0.adapterSessionID == adapterSessionID
        }
        computerUseLeaseWaiters.removeAll { $0.adapterSessionID == adapterSessionID }
        for waiter in cancelledWaiters {
            waiter.continuation.resume(returning: false)
        }

        guard computerUseLeaseOwner == adapterSessionID else { return }
        computerUseLeaseOwner = nil

        while let first = computerUseLeaseWaiters.first {
            let nextAdapterSessionID = first.adapterSessionID
            let matchingWaiters = computerUseLeaseWaiters.filter {
                $0.adapterSessionID == nextAdapterSessionID
            }
            computerUseLeaseWaiters.removeAll {
                $0.adapterSessionID == nextAdapterSessionID
            }
            guard sessions[nextAdapterSessionID] != nil else {
                for waiter in matchingWaiters {
                    waiter.continuation.resume(returning: false)
                }
                continue
            }
            computerUseLeaseOwner = nextAdapterSessionID
            for waiter in matchingWaiters {
                waiter.continuation.resume(returning: true)
            }
            break
        }
    }
}
