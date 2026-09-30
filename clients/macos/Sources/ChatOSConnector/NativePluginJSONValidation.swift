import ChatOSCore
import Foundation

extension NativeJSONValue {
    func requireObject() throws -> [String: NativeJSONValue] {
        guard let jsonObject else {
            throw NativePluginRuntimeError.invalidRequest("Plugin body 必须是对象")
        }
        return jsonObject
    }

    func value(atJSONPointer pointer: String) -> NativeJSONValue? {
        guard pointer.hasPrefix("/") else { return nil }
        return pointer.dropFirst().split(separator: "/").reduce(Optional(self)) { current, token in
            guard let current else { return nil }
            let key = token.replacingOccurrences(of: "~1", with: "/")
                .replacingOccurrences(of: "~0", with: "~")
            return current.jsonObject?[key]
        }
    }
}

extension Dictionary where Key == String, Value == NativeJSONValue {
    func requireString(_ key: String) throws -> String {
        guard let value = self[key]?.jsonString?.nonEmptyPluginValue else {
            throw NativePluginRuntimeError.invalidRequest("Plugin 缺少 \(key)")
        }
        return value
    }

    func requireStringArray(_ key: String) throws -> [String] {
        guard let values = self[key]?.jsonArray else {
            throw NativePluginRuntimeError.invalidRequest("Plugin 缺少 \(key)")
        }
        return try values.map {
            guard let value = $0.jsonString?.nonEmptyPluginValue else {
                throw NativePluginRuntimeError.invalidRequest("Plugin 的 \(key) 无效")
            }
            return value
        }
    }
}

private extension String {
    var nonEmptyPluginValue: String? {
        let result = trimmingCharacters(in: .whitespacesAndNewlines)
        return result.isEmpty ? nil : result
    }
}
