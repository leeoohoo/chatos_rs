import ChatOSCore
import Darwin
import Foundation

actor NativePluginApplicationRuntime {
    private struct RunningApplication {
        var process: Process
        var baseURL: URL
        var healthPath: String
        var standardOutput: Pipe
        var standardError: Pipe
    }

    private var running: [String: RunningApplication] = [:]

    func launch(
        record: NativeInstalledPluginRecord,
        manifest: NativePluginManifest,
        contribution: NativePluginManifest.UIContribution,
        runtimeRootURL: URL,
        application: LocalConnectorPluginApplication
    ) async throws -> LocalConnectorPluginApplicationLaunch {
        let key = application.id
        if let current = running[key], current.process.isRunning,
           await isHealthy(baseURL: current.baseURL, healthPath: current.healthPath) {
            return .init(application: application, url: current.baseURL)
        }
        stop(key: key)

        let installationURL = URL(fileURLWithPath: record.installationPath, isDirectory: true)
            .standardizedFileURL
        guard manifest.schemaVersion == 3, manifest.version == record.version else {
            throw NativeConnectorError.pluginInstallation("Plugin 应用清单与已安装 Release 不一致")
        }

        guard let runtime = contribution.runtime else {
            let sourceURL = try safeInstalledFile(
                contribution.source.path,
                installationURL: installationURL,
                description: "Plugin UI"
            )
            return .init(application: application, url: sourceURL)
        }
        guard runtime.type == "local_http" else {
            throw NativeConnectorError.pluginInstallation("Plugin UI runtime 类型不受支持")
        }
        guard manifest.permissions.contains(where: {
            $0.permission == "process.spawn"
                && $0.required
                && ($0.components.isEmpty || $0.components.contains(contribution.componentKey))
        }) else {
            throw NativeConnectorError.pluginInstallation("Plugin 应用缺少必需的 process.spawn 权限")
        }
        let executableURL = try resolvePackageExecutable(
            named: runtime.bin,
            installationURL: installationURL
        )
        let port = try availableLoopbackPort()
        let baseURL = URL(string: "http://127.0.0.1:\(port)/")!
        let healthPath = try validatedHealthPath(runtime.healthPath)
        let timeoutMilliseconds = min(120_000, max(100, runtime.launchTimeoutMs ?? 15_000))

        let pluginHash = NativePluginManifestLoader.sha256(record.pluginID)
        let dataURL = runtimeRootURL.appendingPathComponent("data/\(pluginHash)", isDirectory: true)
        let cacheURL = runtimeRootURL.appendingPathComponent("cache/\(pluginHash)", isDirectory: true)
        for directory in [dataURL, cacheURL] {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        }

        let process = Process()
        process.executableURL = executableURL
        process.arguments = runtime.args
        process.currentDirectoryURL = installationURL
        let output = Pipe()
        let error = Pipe()
        output.fileHandleForReading.readabilityHandler = { handle in _ = handle.availableData }
        error.fileHandleForReading.readabilityHandler = { handle in _ = handle.availableData }
        process.standardOutput = output
        process.standardError = error
        var environment = ProcessInfo.processInfo.environment
        environment["PATH"] = Self.runtimePath(environment["PATH"])
        environment.merge([
            "CHATOS_PLUGIN_ROOT": installationURL.path,
            "CHATOS_PLUGIN_DATA_DIR": dataURL.path,
            "CHATOS_PLUGIN_CACHE_DIR": cacheURL.path,
            "CHATOS_PLUGIN_APP_HOST": "127.0.0.1",
            "CHATOS_PLUGIN_APP_PORT": String(port),
            "CHATOS_PLUGIN_ID": record.pluginID,
            "CHATOS_PLUGIN_COMPONENT_KEY": contribution.componentKey,
        ], uniquingKeysWith: { _, runtimeValue in runtimeValue })
        process.environment = environment

        do {
            try process.run()
        } catch {
            throw NativeConnectorError.pluginInstallation(
                "Plugin 应用后端启动失败：\(error.localizedDescription)"
            )
        }
        running[key] = .init(
            process: process,
            baseURL: baseURL,
            healthPath: healthPath,
            standardOutput: output,
            standardError: error
        )
        do {
            try await waitUntilHealthy(
                process: process,
                baseURL: baseURL,
                healthPath: healthPath,
                timeoutMilliseconds: timeoutMilliseconds
            )
        } catch {
            stop(key: key)
            throw error
        }
        return .init(application: application, url: baseURL)
    }

    func stop(pluginID: String) {
        let prefix = "\(pluginID):"
        for key in running.keys.filter({ $0.hasPrefix(prefix) }) {
            stop(key: key)
        }
    }

    func stopAll() {
        for key in Array(running.keys) {
            stop(key: key)
        }
    }

    private func stop(key: String) {
        guard let instance = running.removeValue(forKey: key) else { return }
        instance.standardOutput.fileHandleForReading.readabilityHandler = nil
        instance.standardError.fileHandleForReading.readabilityHandler = nil
        if instance.process.isRunning {
            instance.process.terminate()
        }
    }

    private func waitUntilHealthy(
        process: Process,
        baseURL: URL,
        healthPath: String,
        timeoutMilliseconds: UInt64
    ) async throws {
        let clock = ContinuousClock()
        let deadline = clock.now + .milliseconds(Int64(timeoutMilliseconds))
        while clock.now < deadline {
            guard process.isRunning else {
                throw NativeConnectorError.pluginInstallation("Plugin 应用后端在启动阶段退出")
            }
            if await isHealthy(baseURL: baseURL, healthPath: healthPath) { return }
            try await Task.sleep(for: .milliseconds(80))
        }
        throw NativeConnectorError.pluginInstallation("等待 Plugin 应用后端就绪超时")
    }

    private func isHealthy(baseURL: URL, healthPath: String) async -> Bool {
        guard let url = URL(string: healthPath, relativeTo: baseURL)?.absoluteURL else { return false }
        var request = URLRequest(url: url)
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.timeoutInterval = 0.7
        do {
            let (_, response) = try await URLSession.shared.data(for: request)
            return (response as? HTTPURLResponse).map { (200..<300).contains($0.statusCode) } == true
        } catch {
            return false
        }
    }

    private func validatedHealthPath(_ value: String?) throws -> String {
        let path = value ?? "/api/health"
        guard path.hasPrefix("/"), path.count <= 2_048,
              !path.contains(".."), !path.contains("?"), !path.contains("#"),
              !path.contains("\0") else {
            throw NativeConnectorError.pluginInstallation("Plugin 应用健康检查路径无效")
        }
        return path
    }

    private func resolvePackageExecutable(named name: String, installationURL: URL) throws -> URL {
        guard safeExecutableName(name) else {
            throw NativeConnectorError.pluginInstallation("Plugin 应用可执行文件名无效")
        }
        let packageJSONURL = installationURL.appendingPathComponent("package.json")
        let data = try Data(contentsOf: packageJSONURL, options: .mappedIfSafe)
        guard let package = try JSONSerialization.jsonObject(with: data) as? [String: Any],
              let packageName = package["name"] as? String else {
            throw NativeConnectorError.pluginInstallation("Plugin package.json 无效")
        }
        let target: String?
        if let bins = package["bin"] as? [String: Any] {
            target = bins[name] as? String
        } else if let single = package["bin"] as? String,
                  packageName.split(separator: "/").last.map(String.init) == name {
            target = single
        } else {
            target = nil
        }
        guard let target else {
            throw NativeConnectorError.pluginInstallation("Plugin 应用入口未在 package.json.bin 中声明")
        }
        let executableURL = try safeInstalledFile(
            target,
            installationURL: installationURL,
            description: "Plugin executable"
        )
        let values = try executableURL.resourceValues(forKeys: [
            .isRegularFileKey, .isSymbolicLinkKey, .isExecutableKey,
        ])
        guard values.isRegularFile == true,
              values.isSymbolicLink != true,
              values.isExecutable == true else {
            throw NativeConnectorError.pluginInstallation("Plugin 应用可执行文件不可用")
        }
        return executableURL
    }

    private func safeInstalledFile(
        _ path: String,
        installationURL: URL,
        description: String
    ) throws -> URL {
        let normalized = path.hasPrefix("./") ? String(path.dropFirst(2)) : path
        guard !normalized.isEmpty,
              !normalized.hasPrefix("/"),
              !normalized.contains("\0"),
              !normalized.split(separator: "/").contains("..") else {
            throw NativeConnectorError.pluginInstallation("\(description) 路径无效")
        }
        let url = installationURL.appendingPathComponent(normalized).standardizedFileURL
        guard url.path.hasPrefix(installationURL.path + "/"),
              FileManager.default.fileExists(atPath: url.path) else {
            throw NativeConnectorError.pluginInstallation("\(description) 文件不存在")
        }
        return url
    }

    private func safeExecutableName(_ value: String) -> Bool {
        !value.isEmpty
            && value.count <= 128
            && !value.contains("/")
            && value != "."
            && value != ".."
            && value.utf8.allSatisfy {
                $0 < 128 && (CharacterSet.alphanumerics.contains(UnicodeScalar($0))
                    || $0 == 45 || $0 == 95 || $0 == 46)
            }
    }

    private func availableLoopbackPort() throws -> UInt16 {
        let descriptor = socket(AF_INET, SOCK_STREAM, 0)
        guard descriptor >= 0 else {
            throw NativeConnectorError.pluginInstallation("无法分配 Plugin 应用端口")
        }
        defer { close(descriptor) }
        var address = sockaddr_in()
        address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
        address.sin_family = sa_family_t(AF_INET)
        address.sin_port = 0
        address.sin_addr = in_addr(s_addr: inet_addr("127.0.0.1"))
        let bindResult = withUnsafePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                Darwin.bind(descriptor, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard bindResult == 0 else {
            throw NativeConnectorError.pluginInstallation("无法绑定 Plugin 应用端口")
        }
        var length = socklen_t(MemoryLayout<sockaddr_in>.size)
        let nameResult = withUnsafeMutablePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                getsockname(descriptor, $0, &length)
            }
        }
        guard nameResult == 0 else {
            throw NativeConnectorError.pluginInstallation("无法读取 Plugin 应用端口")
        }
        return UInt16(bigEndian: address.sin_port)
    }

    private static func runtimePath(_ existing: String?) -> String {
        let preferred = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"]
        let current = existing?.split(separator: ":").map(String.init) ?? []
        return Array(NSOrderedSet(array: preferred + current))
            .compactMap { $0 as? String }
            .joined(separator: ":")
    }
}
