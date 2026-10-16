import Foundation

enum NativeLocalAgentTerminalCommandResolver {
    // `common` is a legacy command alias, not a human-readable summary. Keep it for
    // frozen old Runs, but use the same canonical command for approval and execution.
    static func resolve(_ arguments: [String: LocalAgentJSONValue]) -> String? {
        firstNonEmpty(
            string(arguments["command"]),
            string(arguments["common"])
        )
    }

    static func resolve(_ arguments: [String: NativeJSONValue]) -> String? {
        firstNonEmpty(
            arguments["command"]?.jsonString,
            arguments["common"]?.jsonString
        )
    }

    private static func string(_ value: LocalAgentJSONValue?) -> String? {
        guard case let .string(value)? = value else { return nil }
        return value
    }

    private static func firstNonEmpty(_ values: String?...) -> String? {
        values.lazy.compactMap { value in
            value.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
        }.first { !$0.isEmpty }
    }
}
