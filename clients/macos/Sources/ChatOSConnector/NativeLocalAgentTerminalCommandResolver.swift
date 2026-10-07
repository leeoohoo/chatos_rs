import Foundation

enum NativeLocalAgentTerminalCommandResolver {
    static func resolve(_ arguments: [String: LocalAgentJSONValue]) -> String? {
        firstNonEmpty(
            string(arguments["common"]),
            string(arguments["command"])
        )
    }

    static func resolve(_ arguments: [String: NativeJSONValue]) -> String? {
        firstNonEmpty(
            arguments["common"]?.jsonString,
            arguments["command"]?.jsonString
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
