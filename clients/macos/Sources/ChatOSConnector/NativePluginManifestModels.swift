import Foundation

struct NativePluginManifest: Decodable, Sendable {
    struct PathReference: Decodable, Sendable {
        var path: String

        init(from decoder: Decoder) throws {
            if let value = try? decoder.singleValueContainer().decode(String.self) {
                path = value
                return
            }
            let container = try decoder.container(keyedBy: CodingKeys.self)
            path = try container.decode(String.self, forKey: .path)
        }

        private enum CodingKeys: String, CodingKey { case path }
    }

    struct MCPServer: Decodable, Sendable {
        var type: String
        var bin: String
        var args: [String]
        var env: [String: String]
        var requiresExclusiveExecution: Bool

        init(from decoder: Decoder) throws {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            type = try container.decode(String.self, forKey: .type)
            bin = try container.decode(String.self, forKey: .bin)
            args = try container.decodeIfPresent([String].self, forKey: .args) ?? []
            env = try container.decodeIfPresent([String: String].self, forKey: .env) ?? [:]
            requiresExclusiveExecution = try container.decodeIfPresent(
                Bool.self,
                forKey: .requiresExclusiveExecution
            ) ?? false
        }

        private enum CodingKeys: String, CodingKey {
            case type, bin, args, env, requiresExclusiveExecution
        }
    }

    struct Interface: Decodable, Sendable {
        var displayName: String?
        var brandColor: String?
        var logo: PathReference?
        var logoDark: PathReference?
    }

    struct UIContribution: Decodable, Sendable {
        struct Runtime: Decodable, Sendable {
            var type: String
            var bin: String
            var args: [String]
            var healthPath: String?
            var launchTimeoutMs: UInt64?

            init(from decoder: Decoder) throws {
                let container = try decoder.container(keyedBy: CodingKeys.self)
                type = try container.decode(String.self, forKey: .type)
                bin = try container.decode(String.self, forKey: .bin)
                args = try container.decodeIfPresent([String].self, forKey: .args) ?? []
                healthPath = try container.decodeIfPresent(String.self, forKey: .healthPath)
                launchTimeoutMs = try container.decodeIfPresent(UInt64.self, forKey: .launchTimeoutMs)
            }

            private enum CodingKeys: String, CodingKey {
                case type, bin, args, healthPath, launchTimeoutMs
            }
        }

        var componentKey: String
        var source: PathReference
        var title: String?
        var surface: String?
        var runtime: Runtime?
    }

    struct Permission: Decodable, Sendable {
        var permission: String
        var required: Bool
        var reason: String?
        var components: [String]

        init(from decoder: Decoder) throws {
            let container = try decoder.container(keyedBy: CodingKeys.self)
            permission = try container.decode(String.self, forKey: .permission)
            required = try container.decodeIfPresent(Bool.self, forKey: .required) ?? false
            reason = try container.decodeIfPresent(String.self, forKey: .reason)
            components = try container.decodeIfPresent([String].self, forKey: .components) ?? []
        }

        private enum CodingKeys: String, CodingKey { case permission, required, reason, components }
    }

    var schemaVersion: Int
    var name: String
    var version: String
    var description: String
    var skills: [PathReference]
    var mcpServers: [String: MCPServer]
    var ui: [UIContribution]
    var interface: Interface?
    var permissions: [Permission]

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        schemaVersion = try container.decode(Int.self, forKey: .schemaVersion)
        name = try container.decode(String.self, forKey: .name)
        version = try container.decode(String.self, forKey: .version)
        description = try container.decodeIfPresent(String.self, forKey: .description) ?? ""
        skills = try container.decodeIfPresent([PathReference].self, forKey: .skills) ?? []
        mcpServers = try container.decodeIfPresent([String: MCPServer].self, forKey: .mcpServers) ?? [:]
        ui = try container.decodeIfPresent([UIContribution].self, forKey: .ui) ?? []
        interface = try container.decodeIfPresent(Interface.self, forKey: .interface)
        permissions = try container.decodeIfPresent([Permission].self, forKey: .permissions) ?? []
    }

    private enum CodingKeys: String, CodingKey {
        case schemaVersion, name, version, description, skills, mcpServers, ui, interface, permissions
    }
}
