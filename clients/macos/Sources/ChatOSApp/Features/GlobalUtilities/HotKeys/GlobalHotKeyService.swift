@preconcurrency import Carbon
import ChatOSCore
import Combine
import Foundation

private extension Notification.Name {
    static let chatOSGlobalHotKeyPressed = Notification.Name(
        "com.chatos.global-utility-hot-key-pressed"
    )
}

private let chatOSGlobalHotKeySignature: OSType = 0x4348_4F53

private let chatOSGlobalHotKeyHandler: EventHandlerUPP = { _, eventRef, _ in
    guard let eventRef else { return OSStatus(eventNotHandledErr) }
    var hotKeyID = EventHotKeyID()
    let status = GetEventParameter(
        eventRef,
        EventParamName(kEventParamDirectObject),
        EventParamType(typeEventHotKeyID),
        nil,
        MemoryLayout<EventHotKeyID>.size,
        nil,
        &hotKeyID
    )
    guard status == noErr else { return status }
    guard hotKeyID.signature == chatOSGlobalHotKeySignature else {
        return OSStatus(eventNotHandledErr)
    }
    let identifier = hotKeyID.id
    let eventKind = GetEventKind(eventRef)
    DispatchQueue.main.async {
        NotificationCenter.default.post(
            name: .chatOSGlobalHotKeyPressed,
            object: nil,
            userInfo: [
                "identifier": identifier,
                "isPressed": eventKind == UInt32(kEventHotKeyPressed),
            ]
        )
    }
    return noErr
}

@MainActor
final class GlobalHotKeyService: ObservableObject {
    @Published private(set) var states: [GlobalUtilityAction: GlobalHotKeyRegistrationState] =
        Dictionary(uniqueKeysWithValues: GlobalUtilityAction.allCases.map { ($0, .disabled) })

    var onAction: ((GlobalUtilityAction) -> Void)?

    private var handlerRef: EventHandlerRef?
    private var registrations: [GlobalUtilityAction: EventHotKeyRef] = [:]
    private var observer: AnyCancellable?
    private var pressGate = GlobalHotKeyPressGate()

    init() {
        installHandler()
        observer = NotificationCenter.default.publisher(for: .chatOSGlobalHotKeyPressed)
            .receive(on: RunLoop.main)
            .sink { [weak self] notification in
                guard let identifier = notification.userInfo?["identifier"] as? UInt32,
                      let isPressed = notification.userInfo?["isPressed"] as? Bool,
                      let action = GlobalUtilityAction(hotKeyIdentifier: identifier) else { return }
                self?.handle(action, isPressed: isPressed)
            }
    }

    func reconfigure(preferences: GlobalUtilityPreferencesStore) {
        unregisterAll()
        guard preferences.isEnabled else {
            states = Dictionary(
                uniqueKeysWithValues: GlobalUtilityAction.allCases.map { ($0, .disabled) }
            )
            return
        }

        var nextStates: [GlobalUtilityAction: GlobalHotKeyRegistrationState] = [:]
        for action in GlobalUtilityAction.allCases {
            guard preferences.isActionEnabled(action) else {
                nextStates[action] = .disabled
                continue
            }
            let requested = preferences.hotKey(for: action)
            let requestedStatus = register(requested, for: action)
            if requestedStatus == noErr {
                nextStates[action] = .registered(activeHotKey: requested, usesFallback: false)
                continue
            }

            if let fallback = action.fallbackHotKey,
               fallback != requested,
               register(fallback, for: action) == noErr {
                nextStates[action] = .registered(activeHotKey: fallback, usesFallback: true)
            } else if requestedStatus == OSStatus(eventHotKeyInvalidErr) {
                nextStates[action] = .unsupported(message: "Unsupported shortcut")
            } else {
                nextStates[action] = .conflict(
                    requestedHotKey: requested,
                    fallbackHotKey: action.fallbackHotKey
                )
            }
        }
        states = nextStates
    }

    func stop() {
        unregisterAll()
        if let handlerRef {
            RemoveEventHandler(handlerRef)
            self.handlerRef = nil
        }
        states = Dictionary(
            uniqueKeysWithValues: GlobalUtilityAction.allCases.map { ($0, .disabled) }
        )
    }

    private func installHandler() {
        guard handlerRef == nil else { return }
        var eventTypes = [
            EventTypeSpec(
                eventClass: OSType(kEventClassKeyboard),
                eventKind: UInt32(kEventHotKeyPressed)
            ),
            EventTypeSpec(
                eventClass: OSType(kEventClassKeyboard),
                eventKind: UInt32(kEventHotKeyReleased)
            ),
        ]
        _ = eventTypes.withUnsafeMutableBufferPointer { buffer in
            InstallEventHandler(
                GetApplicationEventTarget(),
                chatOSGlobalHotKeyHandler,
                buffer.count,
                buffer.baseAddress,
                nil,
                &handlerRef
            )
        }
    }

    private func register(
        _ hotKey: GlobalHotKey,
        for action: GlobalUtilityAction
    ) -> OSStatus {
        guard hotKey.isValid else { return OSStatus(eventHotKeyInvalidErr) }
        var registration: EventHotKeyRef?
        let identifier = EventHotKeyID(
            signature: chatOSGlobalHotKeySignature,
            id: action.hotKeyIdentifier
        )
        let status = RegisterEventHotKey(
            hotKey.keyCode,
            hotKey.modifiers.carbonModifiers,
            identifier,
            GetApplicationEventTarget(),
            0,
            &registration
        )
        if status == noErr, let registration {
            registrations[action] = registration
        }
        return status
    }

    private func unregisterAll() {
        for registration in registrations.values {
            UnregisterEventHotKey(registration)
        }
        registrations.removeAll()
        pressGate.reset()
    }

    private func handle(_ action: GlobalUtilityAction, isPressed: Bool) {
        guard pressGate.shouldTrigger(action, isPressed: isPressed) else { return }
        onAction?(action)
    }
}

struct GlobalHotKeyPressGate {
    private static let debounceInterval: TimeInterval = 0.35
    private var lastTriggerTimes: [GlobalUtilityAction: TimeInterval] = [:]

    mutating func shouldTrigger(
        _ action: GlobalUtilityAction,
        isPressed: Bool,
        now: TimeInterval = ProcessInfo.processInfo.systemUptime
    ) -> Bool {
        guard isPressed else { return false }
        if let previous = lastTriggerTimes[action],
           now - previous < Self.debounceInterval {
            return false
        }
        lastTriggerTimes[action] = now
        return true
    }

    mutating func reset() {
        lastTriggerTimes.removeAll()
    }
}

private extension GlobalUtilityAction {
    var hotKeyIdentifier: UInt32 {
        switch self {
        case .screenshot: 1
        case .screenRecording: 2
        case .clipboardHistory: 3
        case .quickSearch: 4
        }
    }

    init?(hotKeyIdentifier: UInt32) {
        switch hotKeyIdentifier {
        case 1: self = .screenshot
        case 2: self = .screenRecording
        case 3: self = .clipboardHistory
        case 4: self = .quickSearch
        default: return nil
        }
    }
}

private extension GlobalHotKeyModifiers {
    var carbonModifiers: UInt32 {
        var value: UInt32 = 0
        if contains(.command) { value |= UInt32(cmdKey) }
        if contains(.option) { value |= UInt32(optionKey) }
        if contains(.control) { value |= UInt32(controlKey) }
        if contains(.shift) { value |= UInt32(shiftKey) }
        return value
    }
}
