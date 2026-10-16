import CoreFoundation
import Foundation

/// Bounded, offline JSON Schema preflight for plugin/MCP tools. No reference may
/// fetch a URL. Unsupported assertions reject the call instead of being ignored.
/// Keep the stricter first-party AgentSchemaValidator contract unchanged.
public enum AgentExternalToolSchemaValidator {
    public static func validate(arguments: String, schema: Data) throws {
        guard arguments.utf8.count <= AgentSchemaValidator.maximumArgumentBytes else {
            throw Failure.mismatch("$", "argument envelope is too large")
        }
        let root = try JSONSerialization.jsonObject(with: schema, options: [.fragmentsAllowed])
        let value = try JSONSerialization.jsonObject(with: Data(arguments.utf8), options: [.fragmentsAllowed])
        try check(value, schema: root, root: root, path: "$", depth: 0)
    }

    private static func check(_ value: Any, schema: Any, root: Any, path: String, depth: Int) throws {
        guard depth < 32 else { throw Failure.unsupported("schema nesting/reference limit") }
        if let boolean = schema as? NSNumber, CFGetTypeID(boolean) == CFBooleanGetTypeID() {
            if !boolean.boolValue { throw Failure.mismatch(path, "schema rejects this value") }
            return
        }
        guard let spec = schema as? [String: Any] else { throw Failure.unsupported("schema must be an object or boolean") }
        let supported: Set<String> = [
            "$schema", "$id", "$comment", "$defs", "definitions", "$ref", "title", "description",
            "default", "examples", "deprecated", "readOnly", "writeOnly", "format",
            "x-enum-labels", "x-chatos-external-mcp-id",
            "type", "properties", "required", "additionalProperties", "minProperties", "maxProperties",
            "items", "prefixItems", "minItems", "maxItems", "uniqueItems", "minimum", "maximum",
            "exclusiveMinimum", "exclusiveMaximum", "multipleOf", "minLength", "maxLength", "pattern",
            "enum", "const", "anyOf", "oneOf", "allOf", "not", "if", "then", "else",
        ]
        if let keyword = Set(spec.keys).subtracting(supported).sorted().first {
            throw Failure.unsupported(keyword)
        }
        if let reference = spec["$ref"] as? String {
            try check(value, schema: resolve(reference, root: root), root: root, path: path, depth: depth + 1)
        }
        for keyword in ["allOf", "anyOf", "oneOf"] {
            guard let branches = spec[keyword] as? [Any] else { continue }
            var matches = 0
            var reasons: [String] = []
            for branch in branches {
                do {
                    try check(value, schema: branch, root: root, path: path, depth: depth + 1)
                    matches += 1
                } catch let failure as Failure {
                    if case .unsupported = failure { throw failure }
                    reasons.append(failure.localizedDescription)
                }
            }
            let valid = keyword == "allOf" ? matches == branches.count
                : keyword == "oneOf" ? matches == 1 : matches > 0
            if !valid {
                throw Failure.mismatch(path, "does not match \(keyword); \(reasons.prefix(3).joined(separator: "; "))")
            }
        }
        if let negated = spec["not"], try matches(value, schema: negated, root: root, path: path, depth: depth + 1) {
            throw Failure.mismatch(path, "matches forbidden schema")
        }
        if let condition = spec["if"] {
            let branch = try matches(value, schema: condition, root: root, path: path, depth: depth + 1) ? "then" : "else"
            if let schema = spec[branch] { try check(value, schema: schema, root: root, path: path, depth: depth + 1) }
        }
        if let choices = spec["enum"] as? [Any], !choices.contains(where: { equal(value, $0) }) {
            throw Failure.mismatch(path, "must be one of the declared enum values")
        }
        if let constant = spec["const"], !equal(value, constant) { throw Failure.mismatch(path, "must match const") }
        let types = (spec["type"] as? String).map { [$0] } ?? (spec["type"] as? [String])
        if let types, !types.contains(where: { hasType(value, $0) }) {
            throw Failure.mismatch(path, "expected \(types.joined(separator: " or "))")
        }
        if let object = value as? [String: Any] {
            try bounds(Double(object.count), minimum: spec["minProperties"], maximum: spec["maxProperties"], path: path)
            for key in spec["required"] as? [String] ?? [] where object[key] == nil {
                throw Failure.mismatch(path + "." + key, "required field is missing")
            }
            let properties = spec["properties"] as? [String: Any] ?? [:]
            for key in object.keys.sorted() {
                if let schema = properties[key] {
                    try check(object[key]!, schema: schema, root: root, path: path + "." + key, depth: depth + 1)
                } else if let schema = spec["additionalProperties"] {
                    try check(object[key]!, schema: schema, root: root, path: path + "." + key, depth: depth + 1)
                }
            }
        }
        if let array = value as? [Any] {
            try bounds(Double(array.count), minimum: spec["minItems"], maximum: spec["maxItems"], path: path)
            if spec["uniqueItems"] as? Bool == true {
                var seen = Set<Data>()
                for item in array {
                    let bytes = try JSONSerialization.data(withJSONObject: item, options: [.fragmentsAllowed, .sortedKeys])
                    if !seen.insert(bytes).inserted { throw Failure.mismatch(path, "items must be unique") }
                }
            }
            let prefix = spec["prefixItems"] as? [Any] ?? (spec["items"] as? [Any] ?? [])
            for (index, item) in array.enumerated() {
                let schema: Any? = index < prefix.count ? prefix[index] : (spec["items"] is [Any] ? nil : spec["items"])
                if let schema { try check(item, schema: schema, root: root, path: "\(path)[\(index)]", depth: depth + 1) }
            }
        }
        if let string = value as? String {
            try bounds(Double(string.unicodeScalars.count), minimum: spec["minLength"], maximum: spec["maxLength"], path: path)
            if let pattern = spec["pattern"] as? String {
                guard let expression = try? NSRegularExpression(pattern: pattern) else { throw Failure.unsupported("pattern") }
                if expression.firstMatch(in: string, range: NSRange(string.startIndex..., in: string)) == nil {
                    throw Failure.mismatch(path, "must match the declared pattern")
                }
            }
        }
        if let number = value as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID() {
            let actual = number.doubleValue
            try bounds(actual, minimum: spec["minimum"], maximum: spec["maximum"], path: path)
            if let minimum = spec["exclusiveMinimum"] as? Double, actual <= minimum { throw Failure.mismatch(path, "must exceed \(minimum)") }
            if let maximum = spec["exclusiveMaximum"] as? Double, actual >= maximum { throw Failure.mismatch(path, "must be below \(maximum)") }
            if let multiple = spec["multipleOf"] as? Double {
                guard multiple > 0 else { throw Failure.unsupported("multipleOf") }
                let quotient = actual / multiple
                if abs(quotient - quotient.rounded()) > 1e-9 { throw Failure.mismatch(path, "must be a multiple of \(multiple)") }
            }
        }
    }

    private static func matches(_ value: Any, schema: Any, root: Any, path: String, depth: Int) throws -> Bool {
        do { try check(value, schema: schema, root: root, path: path, depth: depth); return true }
        catch let failure as Failure {
            if case .unsupported = failure { throw failure }
            return false
        }
    }

    private static func resolve(_ reference: String, root: Any) throws -> Any {
        if reference == "#" { return root }
        guard reference.hasPrefix("#/") else { throw Failure.unsupported("only local JSON-pointer $ref is supported") }
        var value = root
        for segment in reference.dropFirst(2).split(separator: "/", omittingEmptySubsequences: false) {
            let key = String(segment).replacingOccurrences(of: "~1", with: "/").replacingOccurrences(of: "~0", with: "~")
            guard let next = (value as? [String: Any])?[key] else { throw Failure.unsupported("unresolved $ref") }
            value = next
        }
        return value
    }

    private static func equal(_ lhs: Any, _ rhs: Any) -> Bool {
        if (lhs is NSNull) != (rhs is NSNull) { return false }
        if let l = lhs as? NSNumber, let r = rhs as? NSNumber {
            return (CFGetTypeID(l) == CFBooleanGetTypeID()) == (CFGetTypeID(r) == CFBooleanGetTypeID()) && l == r
        }
        return (lhs as? NSObject)?.isEqual(rhs) == true
    }

    private static func hasType(_ value: Any, _ type: String) -> Bool {
        switch type {
        case "null": return value is NSNull
        case "object": return value is [String: Any]
        case "array": return value is [Any]
        case "string": return value is String
        case "boolean": return (value as? NSNumber).map { CFGetTypeID($0) == CFBooleanGetTypeID() } == true
        case "number", "integer":
            guard let number = value as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID(), number.doubleValue.isFinite else { return false }
            return type == "number" || number.doubleValue.rounded() == number.doubleValue
        default: return false
        }
    }

    private static func bounds(_ actual: Double, minimum: Any?, maximum: Any?, path: String) throws {
        if let minimum = minimum as? Double, actual < minimum { throw Failure.mismatch(path, "minimum is \(minimum)") }
        if let maximum = maximum as? Double, actual > maximum { throw Failure.mismatch(path, "maximum is \(maximum)") }
    }

    private enum Failure: LocalizedError {
        case mismatch(String, String)
        case unsupported(String)
        var errorDescription: String? {
            switch self {
            case .mismatch(let path, let reason): "工具参数不符合 Schema：\(path)：\(reason)"
            case .unsupported(let reason): "工具 Schema 暂不支持：\(reason)；调用未执行。"
            }
        }
    }
}
