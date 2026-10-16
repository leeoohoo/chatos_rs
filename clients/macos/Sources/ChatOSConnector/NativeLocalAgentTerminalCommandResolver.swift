import Foundation

enum NativeLocalAgentTerminalCommandResolver {
    static let maximumPurposeCharacters = 500

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

    static func purpose(_ arguments: [String: LocalAgentJSONValue]) -> String? {
        boundedPurpose(string(arguments["purpose"]))
    }

    static func purpose(_ arguments: [String: NativeJSONValue]) -> String? {
        boundedPurpose(arguments["purpose"]?.jsonString)
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

    private static func boundedPurpose(_ value: String?) -> String? {
        guard let purpose = firstNonEmpty(value),
              purpose.count <= maximumPurposeCharacters else { return nil }
        return purpose
    }
}
