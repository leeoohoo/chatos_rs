import Foundation

public enum GlobalUtilityAction: String, CaseIterable, Codable, Hashable, Sendable {
    case screenshot
    case screenRecording
    case clipboardHistory
    case quickSearch

    public var defaultHotKey: GlobalHotKey {
        switch self {
        case .screenshot:
            GlobalHotKey(keyCode: 0, keyEquivalent: "A", modifiers: [.control])
        case .screenRecording:
            GlobalHotKey(keyCode: 12, keyEquivalent: "Q", modifiers: [.control])
        case .clipboardHistory:
            GlobalHotKey(keyCode: 14, keyEquivalent: "E", modifiers: [.command])
        case .quickSearch:
            GlobalHotKey(keyCode: 49, keyEquivalent: "Space", modifiers: [.command])
        }
    }

    public var fallbackHotKey: GlobalHotKey? {
        switch self {
        case .quickSearch:
            GlobalHotKey(keyCode: 49, keyEquivalent: "Space", modifiers: [.option])
        case .screenshot, .screenRecording, .clipboardHistory:
            nil
        }
    }
}

public struct GlobalHotKeyModifiers: OptionSet, Codable, Hashable, Sendable {
    public let rawValue: UInt32

    public init(rawValue: UInt32) {
        self.rawValue = rawValue
    }

    public static let command = Self(rawValue: 1 << 0)
    public static let option = Self(rawValue: 1 << 1)
    public static let control = Self(rawValue: 1 << 2)
    public static let shift = Self(rawValue: 1 << 3)

    public var displayPrefix: String {
        var value = ""
        if contains(.control) { value += "⌃" }
        if contains(.option) { value += "⌥" }
        if contains(.shift) { value += "⇧" }
        if contains(.command) { value += "⌘" }
        return value
    }
}

public struct GlobalHotKey: Codable, Equatable, Hashable, Sendable {
    public var keyCode: UInt32
    public var keyEquivalent: String
    public var modifiers: GlobalHotKeyModifiers

    public init(
        keyCode: UInt32,
        keyEquivalent: String,
        modifiers: GlobalHotKeyModifiers
    ) {
        self.keyCode = keyCode
        self.keyEquivalent = keyEquivalent
        self.modifiers = modifiers
    }

    public var displayName: String {
        "\(modifiers.displayPrefix)\(keyEquivalent)"
    }

    public var isValid: Bool {
        !keyEquivalent.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            && !modifiers.isEmpty
    }
}

public enum GlobalHotKeyRegistrationState: Equatable, Sendable {
    case registered(activeHotKey: GlobalHotKey, usesFallback: Bool)
    case conflict(requestedHotKey: GlobalHotKey, fallbackHotKey: GlobalHotKey?)
    case unsupported(message: String)
    case disabled
}
