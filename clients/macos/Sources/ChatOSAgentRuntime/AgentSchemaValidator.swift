import Foundation
import CoreFoundation

/// Supported JSON Schema subset used by first-party tools. Unknown schema keywords fail closed.
public enum AgentSchemaValidator {
    public static func validate(arguments: String, schema: Data) throws {
        guard let data = arguments.data(using: .utf8), data.count <= 2 * 1024 * 1024,
              let specification = try JSONSerialization.jsonObject(with: schema) as? [String: Any] else { throw ValidationError.invalid("schema") }
        try check(try JSONSerialization.jsonObject(with: data, options: [.fragmentsAllowed]), schema: specification, path: "$", depth: 0)
    }
    private static func check(_ value: Any, schema: [String: Any], path: String, depth: Int) throws {
        guard depth < 24 else { throw ValidationError.invalid(path) }
        let supported: Set<String> = ["type", "properties", "required", "additionalProperties", "items", "minItems", "maxItems", "minimum", "maximum", "minLength", "maxLength", "enum", "description"]
        guard Set(schema.keys).isSubset(of: supported) else { throw ValidationError.invalid("unsupported schema") }
        switch schema["type"] as? String {
        case "object":
            guard let object = value as? [String: Any] else { throw ValidationError.invalid(path) }
            let properties = schema["properties"] as? [String: [String: Any]] ?? [:]
            for key in schema["required"] as? [String] ?? [] where object[key] == nil { throw ValidationError.invalid(path + "." + key) }
            for (key, child) in object {
                if let spec = properties[key] { try check(child, schema: spec, path: path + "." + key, depth: depth + 1) }
                else if schema["additionalProperties"] as? Bool == false { throw ValidationError.invalid(path + "." + key) }
            }
        case "array":
            guard let array = value as? [Any], array.count >= (schema["minItems"] as? Int ?? 0),
                  array.count <= (schema["maxItems"] as? Int ?? 10_000), let item = schema["items"] as? [String: Any] else { throw ValidationError.invalid(path) }
            for child in array { try check(child, schema: item, path: path + "[]", depth: depth + 1) }
        case "string":
            guard let string = value as? String, string.count >= (schema["minLength"] as? Int ?? 0),
                  string.count <= (schema["maxLength"] as? Int ?? 1_000_000) else { throw ValidationError.invalid(path) }
            if let options = schema["enum"] as? [String], !options.contains(string) { throw ValidationError.invalid(path) }
        case "integer", "number":
            guard let number = value as? NSNumber, CFGetTypeID(number) != CFBooleanGetTypeID(), number.doubleValue.isFinite,
                  number.doubleValue >= (schema["minimum"] as? Double ?? -.greatestFiniteMagnitude),
                  number.doubleValue <= (schema["maximum"] as? Double ?? .greatestFiniteMagnitude),
                  schema["type"] as? String != "integer" || number.doubleValue.rounded() == number.doubleValue else { throw ValidationError.invalid(path) }
        case "boolean":
            guard let number = value as? NSNumber, CFGetTypeID(number) == CFBooleanGetTypeID() else { throw ValidationError.invalid(path) }
        default: throw ValidationError.invalid(path)
        }
    }
    private enum ValidationError: LocalizedError {
        case invalid(String)
        var errorDescription: String? { if case .invalid(let path) = self { return "工具参数不符合 Schema：\(path)" }; return nil }
    }
}
