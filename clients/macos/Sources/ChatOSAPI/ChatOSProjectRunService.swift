import ChatOSCore
import Foundation

public struct ChatOSProjectRunService: ProjectRunServicing {
    private let client: ChatOSAPIClient

    public init(client: ChatOSAPIClient) {
        self.client = client
    }

    public func fetchCatalog(projectID: String) async throws -> ProjectRunCatalog {
        let response: CatalogDTO = try await client.request("/projects/\(projectID.encodedPathComponent)/run/catalog")
        return response.domainModel(fallbackProjectID: projectID)
    }

    public func analyze(projectID: String) async throws -> ProjectRunCatalog {
        let response: CatalogDTO = try await client.request(
            "/projects/\(projectID.encodedPathComponent)/run/analyze",
            method: "POST"
        )
        return response.domainModel(fallbackProjectID: projectID)
    }

    public func fetchState(projectID: String) async throws -> ProjectRunState {
        let response: StateDTO = try await client.request("/projects/\(projectID.encodedPathComponent)/run/state")
        return response.domainModel(fallbackProjectID: projectID)
    }

    public func fetchEnvironment(projectID: String) async throws -> ProjectRunEnvironment {
        let response: EnvironmentDTO = try await client.request("/projects/\(projectID.encodedPathComponent)/run/environment")
        return response.domainModel
    }

    public func updateEnvironment(
        projectID: String,
        selectedToolchains: [String: String],
        customToolchains: [String: ProjectRunCustomToolchain],
        environmentVariables: [String: String]
    ) async throws -> ProjectRunEnvironment {
        let custom = customToolchains.mapValues {
            CustomToolchainRequest(kind: $0.kind, label: $0.label, path: $0.path)
        }
        let body = try JSONEncoder().encode(
            EnvironmentUpdateRequest(
                selectedToolchains: selectedToolchains,
                customToolchains: custom,
                environmentVariables: environmentVariables
            )
        )
        let response: EnvironmentDTO = try await client.request(
            "/projects/\(projectID.encodedPathComponent)/run/environment",
            method: "PUT",
            body: body
        )
        return response.domainModel
    }

    public func setDefaultTarget(projectID: String, targetID: String) async throws -> ProjectRunCatalog {
        let body = try JSONEncoder().encode(DefaultTargetRequest(targetID: targetID))
        let response: CatalogDTO = try await client.request(
            "/projects/\(projectID.encodedPathComponent)/run/default",
            method: "POST",
            body: body
        )
        return response.domainModel(fallbackProjectID: projectID)
    }

    public func start(projectID: String, targetID: String) async throws {
        let body = try JSONEncoder().encode(StartRequest(targetID: targetID, createIfMissing: true))
        let _: RunMutationDTO = try await client.request(
            "/projects/\(projectID.encodedPathComponent)/run/execute",
            method: "POST",
            body: body
        )
    }

    public func stop(instanceID: String) async throws {
        let _: RunMutationDTO = try await client.request(
            "/terminals/\(instanceID.encodedPathComponent)/interrupt",
            method: "POST"
        )
    }

    public func delete(instanceID: String) async throws {
        let _: RunMutationDTO = try await client.request(
            "/terminals/\(instanceID.encodedPathComponent)",
            method: "DELETE"
        )
    }
}

private struct CatalogDTO: Decodable, Sendable {
    var projectID: String?
    var status: String?
    var defaultTargetID: String?
    var targets: [TargetDTO]?
    var errorMessage: String?
    enum CodingKeys: String, CodingKey {
        case status, targets
        case projectID = "project_id"
        case defaultTargetID = "default_target_id"
        case errorMessage = "error_message"
    }
    func domainModel(fallbackProjectID: String) -> ProjectRunCatalog {
        ProjectRunCatalog(
            projectID: projectID ?? fallbackProjectID,
            status: status ?? "unknown",
            defaultTargetID: defaultTargetID,
            targets: (targets ?? []).map(\.domainModel),
            errorMessage: errorMessage
        )
    }
}

private struct TargetDTO: Decodable, Sendable {
    var id: String
    var label: String?
    var kind: String?
    var language: String?
    var cwd: String?
    var command: String?
    var source: String?
    var isDefault: Bool?
    var entrypoint: String?
    var manifestPath: String?
    var requiredToolchains: [String]?
    enum CodingKeys: String, CodingKey {
        case id, label, kind, language, cwd, command, source, entrypoint
        case isDefault = "is_default"
        case manifestPath = "manifest_path"
        case requiredToolchains = "required_toolchains"
    }
    var domainModel: ProjectRunTarget {
        ProjectRunTarget(
            id: id,
            label: label ?? id,
            kind: kind ?? "custom",
            language: language,
            cwd: cwd ?? "",
            command: command ?? "",
            source: source ?? "unknown",
            isDefault: isDefault ?? false,
            entrypoint: entrypoint,
            manifestPath: manifestPath,
            requiredToolchains: requiredToolchains ?? []
        )
    }
}

private struct StateDTO: Decodable, Sendable {
    var projectID: String?
    var running: Bool?
    var busy: Bool?
    var status: String?
    var instances: [InstanceDTO]?
    enum CodingKeys: String, CodingKey { case running, busy, status, instances; case projectID = "project_id" }
    func domainModel(fallbackProjectID: String) -> ProjectRunState {
        ProjectRunState(
            projectID: projectID ?? fallbackProjectID,
            status: status ?? "idle",
            isBusy: busy ?? false,
            isRunning: running ?? false,
            instances: (instances ?? []).compactMap(\.domainModel)
        )
    }
}

private struct InstanceDTO: Decodable, Sendable {
    var terminalID: String?
    var terminalName: String?
    var cwd: String?
    var status: String?
    var busy: Bool?
    var running: Bool?
    enum CodingKeys: String, CodingKey {
        case cwd, status, busy, running
        case terminalID = "terminal_id"
        case terminalName = "terminal_name"
    }
    var domainModel: ProjectRunInstance? {
        guard let terminalID else { return nil }
        return .init(
            id: terminalID,
            name: terminalName ?? terminalID,
            cwd: cwd,
            status: status ?? "unknown",
            isBusy: busy ?? false,
            isRunning: running ?? false
        )
    }
}

private struct EnvironmentDTO: Decodable, Sendable {
    var optionsByKind: [String: [ToolchainOptionDTO]]?
    var configurationFiles: [ConfigurationFileDTO]?
    var validationIssues: [ValidationIssueDTO]?
    var selectedToolchains: [String: String]?
    var customToolchains: [String: CustomToolchainDTO]?
    var environmentVariables: [String: String]?
    var terminalUIEnabled: Bool?
    enum CodingKeys: String, CodingKey {
        case optionsByKind = "options_by_kind"
        case configurationFiles = "config_files"
        case validationIssues = "validation_issues"
        case selectedToolchains = "selected_toolchains"
        case customToolchains = "custom_toolchains"
        case environmentVariables = "env_vars"
        case terminalUIEnabled = "terminal_ui_enabled"
    }
    var domainModel: ProjectRunEnvironment {
        ProjectRunEnvironment(
            toolchainOptions: (optionsByKind ?? [:]).mapValues { $0.map(\.domainModel) },
            configurationFiles: (configurationFiles ?? []).map(\.domainModel),
            validationIssues: (validationIssues ?? []).map(\.domainModel),
            selectedToolchains: selectedToolchains ?? [:],
            customToolchains: (customToolchains ?? [:]).mapValues(\.domainModel),
            environmentVariables: environmentVariables ?? [:],
            terminalUIEnabled: terminalUIEnabled ?? false
        )
    }
}

private struct ToolchainOptionDTO: Decodable, Sendable {
    var id: String
    var kind: String?
    var label: String?
    var version: String?
    var path: String?
    var source: String?
    var isDefault: Bool?
    enum CodingKeys: String, CodingKey { case id, kind, label, version, path, source; case isDefault = "is_default" }
    var domainModel: ProjectRunToolchainOption {
        .init(id: id, kind: kind ?? "", label: label ?? id, version: version, path: path ?? "", source: source ?? "auto", isDefault: isDefault ?? false)
    }
}

private struct ConfigurationFileDTO: Decodable, Sendable {
    var kind: String?
    var label: String?
    var path: String?
    var preview: String?
    var source: String?
    var domainModel: ProjectRunConfigurationFile {
        .init(kind: kind ?? "config", label: label ?? "配置文件", path: path ?? "", preview: preview, source: source ?? "project")
    }
}

private struct ValidationIssueDTO: Decodable, Sendable {
    var kind: String?
    var message: String?
    var targetID: String?
    var targetLabel: String?
    var path: String?
    var hint: String?
    enum CodingKeys: String, CodingKey {
        case kind, message, path, hint
        case targetID = "target_id"
        case targetLabel = "target_label"
    }
    var domainModel: ProjectRunValidationIssue {
        .init(kind: kind ?? "validation", message: message ?? "未知问题", targetID: targetID, targetLabel: targetLabel, path: path, hint: hint)
    }
}

private struct CustomToolchainDTO: Decodable, Sendable {
    var kind: String?
    var label: String?
    var path: String?
    var domainModel: ProjectRunCustomToolchain { .init(kind: kind ?? "", label: label ?? "", path: path ?? "") }
}

private struct RunMutationDTO: Decodable, Sendable { var success: Bool?; var status: String? }
private struct DefaultTargetRequest: Encodable {
    var targetID: String
    enum CodingKeys: String, CodingKey { case targetID = "target_id" }
}
private struct StartRequest: Encodable {
    var targetID: String
    var createIfMissing: Bool
    enum CodingKeys: String, CodingKey { case targetID = "target_id"; case createIfMissing = "create_if_missing" }
}
private struct CustomToolchainRequest: Encodable { var kind: String; var label: String; var path: String }
private struct EnvironmentUpdateRequest: Encodable {
    var selectedToolchains: [String: String]
    var customToolchains: [String: CustomToolchainRequest]
    var environmentVariables: [String: String]
    enum CodingKeys: String, CodingKey {
        case selectedToolchains = "selected_toolchains"
        case customToolchains = "custom_toolchains"
        case environmentVariables = "env_vars"
    }
}

private extension String {
    var encodedPathComponent: String { addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? self }
}
