import ChatOSCodeReadWorkerClient
import ChatOSCodeReadWorkerProtocol
import ChatOSCore
import Foundation

struct NativeProjectRunAnalysis: Sendable {
    var targets: [ProjectRunTarget]
    var configurationFiles: [ProjectRunConfigurationFile]
    var toolchainOptions: [String: [ProjectRunToolchainOption]]
}

struct NativeProjectRunAnalyzer: Sendable {
    private static let defaultMaximumChildrenPerDirectory = 10_000

    private let maximumChildrenPerDirectory: Int
    private let codeReadWorker: any CodeReadWorkerExecuting

    init(
        maximumChildrenPerDirectory: Int = Self.defaultMaximumChildrenPerDirectory,
        codeReadWorker: any CodeReadWorkerExecuting = CodeReadWorkerProcessClient.shared
    ) {
        self.maximumChildrenPerDirectory = min(
            max(1, maximumChildrenPerDirectory),
            Self.defaultMaximumChildrenPerDirectory
        )
        self.codeReadWorker = codeReadWorker
    }

    func analyze(root: URL) async throws -> NativeProjectRunAnalysis {
        let value: CodeReadWorkerJSONValue
        do {
            value = try await codeReadWorker.execute(
                operation: "project_run_analyze_v1",
                arguments: [
                    "max_children_per_directory": .number(Double(maximumChildrenPerDirectory)),
                ],
                projectRoot: root,
                requestCWD: nil,
                defaultToolRoot: nil,
                ripgrepURL: nil
            )
        } catch let error as CodeReadWorkerProcessClientError {
            if case let .requestRejected(code, _) = error,
               ["not_found", "not_directory"].contains(code) {
                throw NativeProjectRunError.projectDirectoryUnavailable
            }
            throw error
        }
        return try decode(value)
    }

    private func decode(_ value: CodeReadWorkerJSONValue) throws -> NativeProjectRunAnalysis {
        let object = try value.projectRunObject()
        let targets = try object.projectRunArray("targets").map { value in
            let target = try value.projectRunObject()
            return ProjectRunTarget(
                id: try target.projectRunString("id"),
                label: try target.projectRunString("label"),
                kind: try target.projectRunString("kind"),
                language: target.projectRunOptionalString("language"),
                cwd: try target.projectRunString("cwd"),
                command: try target.projectRunString("command"),
                source: try target.projectRunString("source"),
                isDefault: try target.projectRunBool("is_default"),
                entrypoint: target.projectRunOptionalString("entrypoint"),
                manifestPath: target.projectRunOptionalString("manifest_path"),
                requiredToolchains: try target.projectRunArray("required_toolchains").map {
                    guard case let .string(value) = $0 else {
                        throw CodeReadWorkerProcessClientError.invalidResponse
                    }
                    return value
                }
            )
        }
        let configurationFiles = try object.projectRunArray("configuration_files").map { value in
            let file = try value.projectRunObject()
            return ProjectRunConfigurationFile(
                kind: try file.projectRunString("kind"),
                label: try file.projectRunString("label"),
                path: try file.projectRunString("path"),
                preview: file.projectRunOptionalString("preview"),
                source: try file.projectRunString("source")
            )
        }
        guard case let .object(toolchainValues)? = object["toolchain_options"] else {
            throw CodeReadWorkerProcessClientError.invalidResponse
        }
        let toolchainOptions = try toolchainValues.mapValues { value in
            guard case let .array(options) = value else {
                throw CodeReadWorkerProcessClientError.invalidResponse
            }
            return try options.map { value in
                let option = try value.projectRunObject()
                return ProjectRunToolchainOption(
                    id: try option.projectRunString("id"),
                    kind: try option.projectRunString("kind"),
                    label: try option.projectRunString("label"),
                    version: option.projectRunOptionalString("version"),
                    path: try option.projectRunString("path"),
                    source: try option.projectRunString("source"),
                    isDefault: try option.projectRunBool("is_default")
                )
            }
        }
        return .init(
            targets: targets,
            configurationFiles: configurationFiles,
            toolchainOptions: toolchainOptions
        )
    }
}

enum NativeProjectRunError: LocalizedError {
    case projectNotRegistered
    case projectDirectoryUnavailable
    case targetNotFound
    case instanceNotFound
    case processLaunchFailed(String)

    var errorDescription: String? {
        switch self {
        case .projectNotRegistered: "项目尚未连接本机目录"
        case .projectDirectoryUnavailable: "项目目录不存在或当前不可访问"
        case .targetNotFound: "没有找到所选运行目标，请重新分析项目"
        case .instanceNotFound: "运行实例不存在或已经结束"
        case let .processLaunchFailed(message): "启动本机进程失败：\(message)"
        }
    }
}

private extension CodeReadWorkerJSONValue {
    func projectRunObject() throws -> [String: CodeReadWorkerJSONValue] {
        guard case let .object(value) = self else {
            throw CodeReadWorkerProcessClientError.invalidResponse
        }
        return value
    }
}

private extension Dictionary where Key == String, Value == CodeReadWorkerJSONValue {
    func projectRunString(_ key: String) throws -> String {
        guard case let .string(value)? = self[key] else {
            throw CodeReadWorkerProcessClientError.invalidResponse
        }
        return value
    }

    func projectRunOptionalString(_ key: String) -> String? {
        guard case let .string(value)? = self[key] else { return nil }
        return value
    }

    func projectRunBool(_ key: String) throws -> Bool {
        guard case let .bool(value)? = self[key] else {
            throw CodeReadWorkerProcessClientError.invalidResponse
        }
        return value
    }

    func projectRunArray(_ key: String) throws -> [CodeReadWorkerJSONValue] {
        guard case let .array(value)? = self[key] else {
            throw CodeReadWorkerProcessClientError.invalidResponse
        }
        return value
    }
}
