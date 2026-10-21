import AppKit
import ChatOSCore
import Combine
import Darwin
import Foundation

private enum GlobalUtilityIPC {
    static let companionArgument = "--global-utilities-companion"
    static let sessionArgument = "--global-utilities-session"
    static let parentPIDArgument = "--global-utilities-parent-pid"

    static let reloadContext = Notification.Name("com.chatos.global-utilities.reload-context")
    static let reloadPreferences = Notification.Name(
        "com.chatos.global-utilities.reload-preferences"
    )
    static let trigger = Notification.Name("com.chatos.global-utilities.trigger")
    static let status = Notification.Name("com.chatos.global-utilities.status")
    static let hostCommand = Notification.Name("com.chatos.global-utilities.host-command")
    static let shutdown = Notification.Name("com.chatos.global-utilities.shutdown")

    static let actionKey = "action"
    static let payloadKey = "payload"

    static var snapshotURL: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("ChatOS", isDirectory: true)
            .appendingPathComponent("GlobalUtilities", isDirectory: true)
            .appendingPathComponent("context.json")
    }

    static func post(
        _ name: Notification.Name,
        sessionID: String,
        userInfo: [String: Any]? = nil
    ) {
        DistributedNotificationCenter.default().postNotificationName(
            name,
            object: sessionID,
            userInfo: userInfo,
            deliverImmediately: true
        )
    }

    static func encoded<Value: Encodable>(_ value: Value) -> String? {
        try? JSONEncoder().encode(value).base64EncodedString()
    }

    static func decoded<Value: Decodable>(_ type: Value.Type, from value: Any?) -> Value? {
        guard let encoded = value as? String,
              let data = Data(base64Encoded: encoded) else { return nil }
        return try? JSONDecoder().decode(type, from: data)
    }
}

private struct GlobalUtilityContextSnapshot: Codable, Sendable {
    let isEnglish: Bool
    let projects: [GlobalUtilitySearchItem]
    let contacts: [GlobalUtilitySearchItem]
}

private enum GlobalUtilityHostCommand: Codable, Sendable {
    case openProject(String)
    case openContact(String)
    case openSettings(runtimePermissions: Bool)
    case openTranslationImage(path: String, suggestedName: String)
}

/// Main-process facade. It never creates a capture, clipboard, search, or hot-key
/// service. Its only job is to launch the isolated companion and exchange small
/// snapshots and commands with it.
@MainActor
final class GlobalUtilityCompanionClient: NSObject {
    let preferences: GlobalUtilityPreferencesStore
    let hotKeys = GlobalHotKeyService(installEventHandler: false)

    private weak var model: AppModel?
    private let sessionID = UUID().uuidString
    private var process: Process?
    private var cancellables = Set<AnyCancellable>()
    private var hasStarted = false
    private var isCompanionReady = false
    private var pendingAction: GlobalUtilityAction?

    init(model: AppModel, preferences: GlobalUtilityPreferencesStore) {
        self.model = model
        self.preferences = preferences
        super.init()
    }

    func start() {
        guard !hasStarted else { return }
        hasStarted = true

        let center = DistributedNotificationCenter.default()
        center.addObserver(
            self,
            selector: #selector(receiveStatus(_:)),
            name: GlobalUtilityIPC.status,
            object: sessionID
        )
        center.addObserver(
            self,
            selector: #selector(receiveHostCommand(_:)),
            name: GlobalUtilityIPC.hostCommand,
            object: sessionID
        )

        guard let model else { return }
        Publishers.CombineLatest3(model.$projects, model.$contacts, model.$interfaceLanguage)
            .receive(on: RunLoop.main)
            .sink { [weak self] _, _, _ in
                self?.publishContextSnapshot()
            }
            .store(in: &cancellables)
        preferences.$configurationRevision
            .dropFirst()
            .receive(on: RunLoop.main)
            .sink { [weak self] _ in
                guard let self else { return }
                GlobalUtilityIPC.post(
                    GlobalUtilityIPC.reloadPreferences,
                    sessionID: self.sessionID
                )
            }
            .store(in: &cancellables)

        publishContextSnapshot()
        launchCompanionIfNeeded()
    }

    func stop() {
        guard hasStarted else { return }
        GlobalUtilityIPC.post(GlobalUtilityIPC.shutdown, sessionID: sessionID)
        process?.terminate()
        process = nil
        cancellables.removeAll()
        DistributedNotificationCenter.default().removeObserver(self)
        hotKeys.stop()
        isCompanionReady = false
        pendingAction = nil
        hasStarted = false
    }

    func trigger(_ action: GlobalUtilityAction) {
        launchCompanionIfNeeded()
        guard isCompanionReady else {
            pendingAction = action
            return
        }
        postTrigger(action)
    }

    private func postTrigger(_ action: GlobalUtilityAction) {
        GlobalUtilityIPC.post(
            GlobalUtilityIPC.trigger,
            sessionID: sessionID,
            userInfo: [GlobalUtilityIPC.actionKey: action.rawValue]
        )
    }

    private func launchCompanionIfNeeded() {
        guard process?.isRunning != true,
              let executableURL = Bundle.main.executableURL else { return }
        let child = Process()
        child.executableURL = executableURL
        child.arguments = [
            GlobalUtilityIPC.companionArgument,
            GlobalUtilityIPC.sessionArgument,
            sessionID,
            GlobalUtilityIPC.parentPIDArgument,
            String(ProcessInfo.processInfo.processIdentifier),
        ]
        child.standardInput = FileHandle.nullDevice
        child.standardOutput = FileHandle.nullDevice
        child.standardError = FileHandle.nullDevice
        child.terminationHandler = { [weak self, weak child] _ in
            Task { @MainActor in
                guard let self, self.process === child else { return }
                self.process = nil
                self.isCompanionReady = false
                self.hotKeys.replaceStatesFromCompanion(Dictionary(
                    uniqueKeysWithValues: GlobalUtilityAction.allCases.map { ($0, .disabled) }
                ))
                guard self.hasStarted else { return }
                DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [weak self] in
                    self?.launchCompanionIfNeeded()
                }
            }
        }
        do {
            try child.run()
            process = child
            isCompanionReady = false
        } catch {
            hotKeys.replaceStatesFromCompanion(Dictionary(
                uniqueKeysWithValues: GlobalUtilityAction.allCases.map {
                    ($0, .unsupported(message: error.localizedDescription))
                }
            ))
        }
    }

    private func publishContextSnapshot() {
        guard let model else { return }
        let snapshot = GlobalUtilityContextSnapshot(
            isEnglish: model.globalUtilityIsEnglish,
            projects: model.globalUtilityProjects,
            contacts: model.globalUtilityContacts
        )
        guard let data = try? JSONEncoder().encode(snapshot) else { return }
        do {
            let url = GlobalUtilityIPC.snapshotURL
            try FileManager.default.createDirectory(
                at: url.deletingLastPathComponent(),
                withIntermediateDirectories: true
            )
            try data.write(to: url, options: .atomic)
        } catch {
            return
        }
        GlobalUtilityIPC.post(GlobalUtilityIPC.reloadContext, sessionID: sessionID)
    }

    @objc private func receiveStatus(_ notification: Notification) {
        guard let states = GlobalUtilityIPC.decoded(
            [GlobalUtilityAction: GlobalHotKeyRegistrationState].self,
            from: notification.userInfo?[GlobalUtilityIPC.payloadKey]
        ) else { return }
        isCompanionReady = true
        hotKeys.replaceStatesFromCompanion(states)
        if let pendingAction {
            self.pendingAction = nil
            postTrigger(pendingAction)
        }
    }

    @objc private func receiveHostCommand(_ notification: Notification) {
        guard let command = GlobalUtilityIPC.decoded(
            GlobalUtilityHostCommand.self,
            from: notification.userInfo?[GlobalUtilityIPC.payloadKey]
        ), let model else { return }
        switch command {
        case let .openProject(projectID):
            model.globalUtilityOpenProject(projectID)
        case let .openContact(contactID):
            model.globalUtilityOpenContact(contactID)
        case let .openSettings(runtimePermissions):
            model.globalUtilityOpenSettings(runtimePermissions: runtimePermissions)
        case let .openTranslationImage(path, suggestedName):
            let url = URL(fileURLWithPath: path)
            Task { [weak model] in
                let data = try? await AppCancellableDetachedWork.run {
                    try AppBoundedFileReader.read(
                        url,
                        maximumBytes: ScreenshotPersistencePolicy.maximumTranslationBytes
                    )
                }
                try? FileManager.default.removeItem(at: url)
                guard let data else { return }
                model?.globalUtilityOpenTranslationImage(
                    data: data,
                    suggestedName: suggestedName
                )
            }
        }
    }
}

@MainActor
private final class GlobalUtilityCompanionContext: GlobalUtilityContext {
    private(set) var globalUtilityIsEnglish = false
    private(set) var globalUtilityProjects: [GlobalUtilitySearchItem] = []
    private(set) var globalUtilityContacts: [GlobalUtilitySearchItem] = []

    private let sessionID: String

    init(sessionID: String) {
        self.sessionID = sessionID
    }

    func reload() {
        guard let data = try? Data(contentsOf: GlobalUtilityIPC.snapshotURL),
              let snapshot = try? JSONDecoder().decode(
                GlobalUtilityContextSnapshot.self,
                from: data
              ) else { return }
        globalUtilityIsEnglish = snapshot.isEnglish
        globalUtilityProjects = snapshot.projects
        globalUtilityContacts = snapshot.contacts
    }

    func globalUtilityOpenProject(_ projectID: String) {
        send(.openProject(projectID))
    }

    func globalUtilityOpenContact(_ contactID: String) {
        send(.openContact(contactID))
    }

    func globalUtilityOpenSettings(runtimePermissions: Bool) {
        send(.openSettings(runtimePermissions: runtimePermissions))
    }

    func globalUtilityOpenTranslationImage(data: Data, suggestedName: String) {
        let sessionID = sessionID
        Task.detached(priority: .userInitiated) {
            let directory = GlobalUtilityIPC.snapshotURL.deletingLastPathComponent()
                .appendingPathComponent("TranslationTransfer", isDirectory: true)
            let url = directory.appendingPathComponent(UUID().uuidString).appendingPathExtension("png")
            do {
                try FileManager.default.createDirectory(
                    at: directory,
                    withIntermediateDirectories: true
                )
                try data.write(to: url, options: .atomic)
                guard let payload = GlobalUtilityIPC.encoded(
                    GlobalUtilityHostCommand.openTranslationImage(
                        path: url.path,
                        suggestedName: suggestedName
                    )
                ) else { return }
                GlobalUtilityIPC.post(
                    GlobalUtilityIPC.hostCommand,
                    sessionID: sessionID,
                    userInfo: [GlobalUtilityIPC.payloadKey: payload]
                )
            } catch {
                try? FileManager.default.removeItem(at: url)
            }
        }
    }

    private func send(_ command: GlobalUtilityHostCommand) {
        guard let payload = GlobalUtilityIPC.encoded(command) else { return }
        GlobalUtilityIPC.post(
            GlobalUtilityIPC.hostCommand,
            sessionID: sessionID,
            userInfo: [GlobalUtilityIPC.payloadKey: payload]
        )
    }
}

@MainActor
private final class GlobalUtilityCompanionDelegate: NSObject, NSApplicationDelegate {
    private let sessionID: String
    private let parentPID: pid_t
    private let context: GlobalUtilityCompanionContext
    private let preferences = GlobalUtilityPreferencesStore()
    private lazy var coordinator = GlobalUtilityCoordinator(
        context: context,
        preferences: preferences
    )
    private var cancellables = Set<AnyCancellable>()
    private var parentMonitor: Timer?

    init(sessionID: String, parentPID: pid_t) {
        self.sessionID = sessionID
        self.parentPID = parentPID
        self.context = GlobalUtilityCompanionContext(sessionID: sessionID)
        super.init()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        context.reload()
        preferences.reloadFromDefaults()
        coordinator.start()

        let center = DistributedNotificationCenter.default()
        center.addObserver(
            self,
            selector: #selector(reloadContext(_:)),
            name: GlobalUtilityIPC.reloadContext,
            object: sessionID
        )
        center.addObserver(
            self,
            selector: #selector(reloadPreferences(_:)),
            name: GlobalUtilityIPC.reloadPreferences,
            object: sessionID
        )
        center.addObserver(
            self,
            selector: #selector(trigger(_:)),
            name: GlobalUtilityIPC.trigger,
            object: sessionID
        )
        center.addObserver(
            self,
            selector: #selector(shutdown(_:)),
            name: GlobalUtilityIPC.shutdown,
            object: sessionID
        )
        coordinator.hotKeys.$states
            .sink { [sessionID] states in
                guard let payload = GlobalUtilityIPC.encoded(states) else { return }
                GlobalUtilityIPC.post(
                    GlobalUtilityIPC.status,
                    sessionID: sessionID,
                    userInfo: [GlobalUtilityIPC.payloadKey: payload]
                )
            }
            .store(in: &cancellables)
        startParentMonitor()
    }

    func applicationWillTerminate(_ notification: Notification) {
        parentMonitor?.invalidate()
        coordinator.stop()
        DistributedNotificationCenter.default().removeObserver(self)
    }

    @objc private func reloadContext(_ notification: Notification) {
        context.reload()
    }

    @objc private func reloadPreferences(_ notification: Notification) {
        preferences.reloadFromDefaults()
    }

    @objc private func trigger(_ notification: Notification) {
        guard let rawValue = notification.userInfo?[GlobalUtilityIPC.actionKey] as? String,
              let action = GlobalUtilityAction(rawValue: rawValue) else { return }
        coordinator.trigger(action)
    }

    @objc private func shutdown(_ notification: Notification) {
        NSApp.terminate(nil)
    }

    private func startParentMonitor() {
        let monitoredPID = parentPID
        parentMonitor = Timer.scheduledTimer(withTimeInterval: 2, repeats: true) {
            _ in
            if kill(monitoredPID, 0) == -1, errno == ESRCH {
                Task { @MainActor in
                    NSApp.terminate(nil)
                }
            }
        }
    }
}

enum GlobalUtilitiesCompanionApplication {
    static var shouldRun: Bool {
        CommandLine.arguments.contains(GlobalUtilityIPC.companionArgument)
    }

    @MainActor
    static func run() {
        guard let sessionID = argument(after: GlobalUtilityIPC.sessionArgument),
              let parentValue = argument(after: GlobalUtilityIPC.parentPIDArgument),
              let parentPID = pid_t(parentValue) else { return }
        let application = NSApplication.shared
        application.setActivationPolicy(.accessory)
        let delegate = GlobalUtilityCompanionDelegate(
            sessionID: sessionID,
            parentPID: parentPID
        )
        application.delegate = delegate
        application.run()
        withExtendedLifetime(delegate) {}
    }

    private static func argument(after marker: String) -> String? {
        guard let index = CommandLine.arguments.firstIndex(of: marker),
              CommandLine.arguments.indices.contains(index + 1) else { return nil }
        return CommandLine.arguments[index + 1]
    }
}
