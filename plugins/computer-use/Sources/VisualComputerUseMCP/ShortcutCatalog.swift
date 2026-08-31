import Foundation

struct ShortcutCatalog: Sendable {
    private struct ShortcutFile: Codable {
        var global: [ShortcutDefinition]?
        var apps: [String: [ShortcutDefinition]]?
    }

    private let global: [ShortcutDefinition]
    private let apps: [String: [ShortcutDefinition]]
    let sourceDescription: String

    init(environment: [String: String] = ProcessInfo.processInfo.environment) {
        let builtIns = Self.builtIns
        var global = builtIns.global ?? []
        var apps = builtIns.apps ?? [:]
        var source = "built-in macOS shortcuts"

        if let path = environment["VISUAL_COMPUTER_USE_SHORTCUTS"], !path.isEmpty,
           let data = FileManager.default.contents(atPath: path),
           let custom = try? JSONDecoder().decode(ShortcutFile.self, from: data) {
            global = Self.merged(base: global, overrides: custom.global ?? [])
            for (app, definitions) in custom.apps ?? [:] {
                apps[app] = Self.merged(base: apps[app] ?? [], overrides: definitions)
            }
            source += " + \(path)"
        }

        self.global = global
        self.apps = apps
        self.sourceDescription = source
    }

    func shortcuts(for application: ActiveApplicationDTO, query: String?) -> [ShortcutDefinition] {
        let appSpecific = application.bundleIdentifier.flatMap { apps[$0] } ?? []
        let all = Self.merged(base: global, overrides: appSpecific)
        guard let query, !query.isEmpty else { return all }
        let needle = query.lowercased()
        return all.filter {
            $0.id.lowercased().contains(needle)
                || $0.title.lowercased().contains(needle)
                || ($0.description?.lowercased().contains(needle) ?? false)
                || $0.keys.joined(separator: "+").lowercased().contains(needle)
        }
    }

    private static func merged(
        base: [ShortcutDefinition],
        overrides: [ShortcutDefinition]
    ) -> [ShortcutDefinition] {
        var result = base
        for item in overrides {
            if let index = result.firstIndex(where: { $0.id == item.id }) {
                result[index] = item
            } else {
                result.append(item)
            }
        }
        return result.sorted { $0.id < $1.id }
    }

    private static let builtIns = ShortcutFile(
        global: [
            .init(id: "copy", title: "Copy", keys: ["command", "c"], description: nil),
            .init(id: "cut", title: "Cut", keys: ["command", "x"], description: nil),
            .init(id: "find", title: "Find", keys: ["command", "f"], description: nil),
            .init(id: "new", title: "New", keys: ["command", "n"], description: nil),
            .init(id: "open", title: "Open", keys: ["command", "o"], description: nil),
            .init(id: "paste", title: "Paste", keys: ["command", "v"], description: nil),
            .init(id: "redo", title: "Redo", keys: ["command", "shift", "z"], description: nil),
            .init(id: "save", title: "Save", keys: ["command", "s"], description: nil),
            .init(id: "select_all", title: "Select All", keys: ["command", "a"], description: nil),
            .init(id: "undo", title: "Undo", keys: ["command", "z"], description: nil),
            .init(id: "close_window", title: "Close Window", keys: ["command", "w"], description: nil),
            .init(id: "preferences", title: "Settings", keys: ["command", ","], description: nil)
        ],
        apps: [
            "com.apple.finder": [
                .init(id: "go_to_folder", title: "Go to Folder", keys: ["command", "shift", "g"], description: nil),
                .init(id: "new_folder", title: "New Folder", keys: ["command", "shift", "n"], description: nil),
                .init(id: "move_to_trash", title: "Move to Trash", keys: ["command", "delete"], description: "Recoverable while the item remains in Trash."),
                .init(id: "quick_look", title: "Quick Look", keys: ["space"], description: nil)
            ],
            "com.apple.Safari": [
                .init(id: "focus_address_bar", title: "Focus Address Bar", keys: ["command", "l"], description: nil),
                .init(id: "new_tab", title: "New Tab", keys: ["command", "t"], description: nil),
                .init(id: "reopen_closed_tab", title: "Reopen Last Closed Tab", keys: ["command", "shift", "t"], description: nil)
            ],
            "com.google.Chrome": [
                .init(id: "focus_address_bar", title: "Focus Address Bar", keys: ["command", "l"], description: nil),
                .init(id: "new_tab", title: "New Tab", keys: ["command", "t"], description: nil),
                .init(id: "reopen_closed_tab", title: "Reopen Last Closed Tab", keys: ["command", "shift", "t"], description: nil)
            ]
        ]
    )
}
