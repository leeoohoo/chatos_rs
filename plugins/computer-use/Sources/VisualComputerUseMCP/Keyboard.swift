@preconcurrency import CoreGraphics
import Foundation

enum Keyboard {
    struct ParsedShortcut: Sendable, Equatable {
        let keyCode: CGKeyCode
        let flags: CGEventFlags
    }

    private static let modifiers: [String: CGEventFlags] = [
        "command": .maskCommand,
        "cmd": .maskCommand,
        "⌘": .maskCommand,
        "shift": .maskShift,
        "⇧": .maskShift,
        "option": .maskAlternate,
        "alt": .maskAlternate,
        "opt": .maskAlternate,
        "⌥": .maskAlternate,
        "control": .maskControl,
        "ctrl": .maskControl,
        "⌃": .maskControl,
        "fn": .maskSecondaryFn,
        "function": .maskSecondaryFn
    ]

    // macOS virtual key codes use the physical ANSI/US keyboard layout. Text entry
    // is handled separately with Unicode keyboard events.
    private static let keyCodes: [String: CGKeyCode] = [
        "a": 0, "s": 1, "d": 2, "f": 3, "h": 4, "g": 5,
        "z": 6, "x": 7, "c": 8, "v": 9, "b": 11,
        "q": 12, "w": 13, "e": 14, "r": 15, "y": 16, "t": 17,
        "1": 18, "2": 19, "3": 20, "4": 21, "6": 22, "5": 23,
        "=": 24, "9": 25, "7": 26, "-": 27, "8": 28, "0": 29,
        "]": 30, "o": 31, "u": 32, "[": 33, "i": 34, "p": 35,
        "return": 36, "enter": 36,
        "l": 37, "j": 38, "'": 39, "k": 40, ";": 41, "\\": 42,
        ",": 43, "/": 44, "n": 45, "m": 46, ".": 47,
        "tab": 48, "space": 49, "`": 50,
        "delete": 51, "backspace": 51, "escape": 53, "esc": 53,
        "f17": 64, "f18": 79, "f19": 80, "f20": 90,
        "f5": 96, "f6": 97, "f7": 98, "f3": 99, "f8": 100,
        "f9": 101, "f11": 103, "f13": 105, "f16": 106,
        "f14": 107, "f10": 109, "f12": 111, "f15": 113,
        "help": 114, "home": 115, "pageup": 116,
        "forwarddelete": 117, "f4": 118, "end": 119,
        "f2": 120, "pagedown": 121, "f1": 122,
        "left": 123, "right": 124, "down": 125, "up": 126
    ]

    static func parseShortcut(_ keys: [String]) throws -> ParsedShortcut {
        guard !keys.isEmpty else {
            throw VisualComputerUseError.invalidShortcut("keys must contain at least one key.")
        }

        var flags: CGEventFlags = []
        var nonModifierKeys: [String] = []

        for rawKey in keys {
            let key = normalize(rawKey)
            if let modifier = modifiers[key] {
                flags.insert(modifier)
            } else {
                nonModifierKeys.append(key)
            }
        }

        guard nonModifierKeys.count == 1, let key = nonModifierKeys.first else {
            throw VisualComputerUseError.invalidShortcut(
                "A shortcut must contain exactly one non-modifier key."
            )
        }
        guard let keyCode = keyCodes[key] else {
            throw VisualComputerUseError.unsupportedKey(key)
        }
        return ParsedShortcut(keyCode: keyCode, flags: flags)
    }

    static func normalize(_ key: String) -> String {
        key.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    }
}
