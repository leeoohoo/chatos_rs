import ChatOSCore
import CryptoKit
import Foundation

struct NativeProjectRunAnalysis: Sendable {
    var targets: [ProjectRunTarget]
    var configurationFiles: [ProjectRunConfigurationFile]
    var toolchainOptions: [String: [ProjectRunToolchainOption]]
}

struct NativeProjectRunAnalyzer: Sendable {
    private static let maximumDirectories = 2_500
    private static let maximumDepth = 6
    private static let ignoredDirectories: Set<String> = [
        ".chatos", ".git", ".idea", ".next", ".venv", ".vscode", "build", "dist",
        "node_modules", "target", "venv", "vendor",
    ]

    func analyze(root: URL) throws -> NativeProjectRunAnalysis {
        var isDirectory: ObjCBool = false
        guard FileManager.default.fileExists(atPath: root.path, isDirectory: &isDirectory),
              isDirectory.boolValue else {
            throw NativeProjectRunError.projectDirectoryUnavailable
        }
        var queue: [(URL, Int)] = [(root, 0)]
        var cursor = 0
        var visited = 0
        var targets: [ProjectRunTarget] = []
        var configurationURLs: Set<URL> = []

        while cursor < queue.count,
              visited < Self.maximumDirectories,
              targets.count < 100 {
            let (directory, depth) = queue[cursor]
            cursor += 1
            visited += 1
            let children = (try? FileManager.default.contentsOfDirectory(
                at: directory,
                includingPropertiesForKeys: [.isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey],
                options: []
            )) ?? []
            var files: [String: URL] = [:]
            for child in children {
                let values = try? child.resourceValues(forKeys: [.isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey])
                guard values?.isSymbolicLink != true else { continue }
                let lower = child.lastPathComponent.lowercased()
                if values?.isDirectory == true {
                    if depth < Self.maximumDepth, !Self.ignoredDirectories.contains(lower) {
                        queue.append((child, depth + 1))
                    }
                } else if values?.isRegularFile == true {
                    files[lower] = child
                }
            }
            detectNode(in: directory, files: files, output: &targets, configs: &configurationURLs)
            detectRust(in: directory, files: files, output: &targets, configs: &configurationURLs)
            detectPython(in: directory, files: files, output: &targets, configs: &configurationURLs)
            detectGo(in: directory, files: files, output: &targets, configs: &configurationURLs)
            detectJava(in: directory, files: files, output: &targets, configs: &configurationURLs)
            detectSwift(in: directory, files: files, output: &targets, configs: &configurationURLs)
        }

        targets = deduplicated(targets).sorted {
            let left = priority($0)
            let right = priority($1)
            return left == right ? $0.label < $1.label : left > right
        }
        if let first = targets.first {
            targets = targets.map { target in
                var target = target
                target.isDefault = target.id == first.id
                return target
            }
        }
        let configFiles = configurationURLs
            .sorted { $0.path < $1.path }
            .prefix(60)
            .map { configurationFile($0, root: root) }
        return .init(
            targets: targets,
            configurationFiles: configFiles,
            toolchainOptions: discoverToolchains()
        )
    }

    private func detectNode(
        in directory: URL,
        files: [String: URL],
        output: inout [ProjectRunTarget],
        configs: inout Set<URL>
    ) {
        guard let manifest = files["package.json"],
              let data = try? Data(contentsOf: manifest),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }
        configs.insert(manifest)
        let manager: String
        if files["pnpm-lock.yaml"] != nil { manager = "pnpm" }
        else if files["yarn.lock"] != nil { manager = "yarn" }
        else { manager = "npm" }
        let scripts = object["scripts"] as? [String: Any] ?? [:]
        for name in ["dev", "start", "serve", "test", "build"] where scripts[name] != nil {
            let command = manager == "npm" ? "npm run \(name)" : "\(manager) \(name)"
            output.append(target(
                label: "Node: \(manager) \(name)", kind: "node", language: "JavaScript / TypeScript",
                cwd: directory, command: command, source: "package.json:scripts.\(name)",
                entrypoint: nil, manifest: manifest, toolchains: ["node", manager]
            ))
        }
    }

    private func detectRust(
        in directory: URL,
        files: [String: URL],
        output: inout [ProjectRunTarget],
        configs: inout Set<URL>
    ) {
        guard let manifest = files["cargo.toml"] else { return }
        configs.insert(manifest)
        if FileManager.default.fileExists(atPath: directory.appendingPathComponent("src/main.rs").path) {
            output.append(target(
                label: "Rust: cargo run", kind: "rust", language: "Rust", cwd: directory,
                command: "cargo run", source: "Cargo.toml", entrypoint: "src/main.rs",
                manifest: manifest, toolchains: ["cargo"]
            ))
        }
        output.append(target(
            label: "Rust: cargo test", kind: "rust", language: "Rust", cwd: directory,
            command: "cargo test", source: "Cargo.toml", entrypoint: nil,
            manifest: manifest, toolchains: ["cargo"]
        ))
    }

    private func detectPython(
        in directory: URL,
        files: [String: URL],
        output: inout [ProjectRunTarget],
        configs: inout Set<URL>
    ) {
        let manifest = files["pyproject.toml"] ?? files["requirements.txt"]
        if let manifest { configs.insert(manifest) }
        if let main = files["main.py"] {
            output.append(target(
                label: "Python: main.py", kind: "python", language: "Python", cwd: directory,
                command: "python3 main.py", source: "main.py", entrypoint: "main.py",
                manifest: manifest ?? main, toolchains: ["python"]
            ))
        }
        if let app = files["app.py"] {
            output.append(target(
                label: "Python: app.py", kind: "python", language: "Python", cwd: directory,
                command: "python3 app.py", source: "app.py", entrypoint: "app.py",
                manifest: manifest ?? app, toolchains: ["python"]
            ))
        }
        if manifest != nil || files["pytest.ini"] != nil {
            output.append(target(
                label: "Python: pytest", kind: "python", language: "Python", cwd: directory,
                command: "python3 -m pytest", source: manifest?.lastPathComponent ?? "pytest.ini",
                entrypoint: nil, manifest: manifest, toolchains: ["python"]
            ))
        }
    }

    private func detectGo(
        in directory: URL,
        files: [String: URL],
        output: inout [ProjectRunTarget],
        configs: inout Set<URL>
    ) {
        guard let manifest = files["go.mod"] else { return }
        configs.insert(manifest)
        output.append(target(
            label: "Go: run", kind: "go", language: "Go", cwd: directory,
            command: "go run .", source: "go.mod", entrypoint: ".",
            manifest: manifest, toolchains: ["go"]
        ))
        output.append(target(
            label: "Go: test", kind: "go", language: "Go", cwd: directory,
            command: "go test ./...", source: "go.mod", entrypoint: nil,
            manifest: manifest, toolchains: ["go"]
        ))
    }

    private func detectJava(
        in directory: URL,
        files: [String: URL],
        output: inout [ProjectRunTarget],
        configs: inout Set<URL>
    ) {
        if let manifest = files["pom.xml"] {
            configs.insert(manifest)
            let command = files["mvnw"] == nil ? "mvn spring-boot:run" : "./mvnw spring-boot:run"
            output.append(target(
                label: "Java: Spring Boot (Maven)", kind: "java", language: "Java", cwd: directory,
                command: command, source: "pom.xml", entrypoint: nil,
                manifest: manifest, toolchains: files["mvnw"] == nil ? ["java", "mvn"] : ["java"]
            ))
        }
        if let manifest = files["build.gradle"] ?? files["build.gradle.kts"] {
            configs.insert(manifest)
            let command = files["gradlew"] == nil ? "gradle bootRun" : "./gradlew bootRun"
            output.append(target(
                label: "Java: Spring Boot (Gradle)", kind: "java", language: "Java / Kotlin", cwd: directory,
                command: command, source: manifest.lastPathComponent, entrypoint: nil,
                manifest: manifest, toolchains: files["gradlew"] == nil ? ["java", "gradle"] : ["java"]
            ))
        }
    }

    private func detectSwift(
        in directory: URL,
        files: [String: URL],
        output: inout [ProjectRunTarget],
        configs: inout Set<URL>
    ) {
        guard let manifest = files["package.swift"] else { return }
        configs.insert(manifest)
        output.append(target(
            label: "Swift: swift run", kind: "swift", language: "Swift", cwd: directory,
            command: "swift run", source: "Package.swift", entrypoint: nil,
            manifest: manifest, toolchains: ["swift"]
        ))
        output.append(target(
            label: "Swift: swift test", kind: "swift", language: "Swift", cwd: directory,
            command: "swift test", source: "Package.swift", entrypoint: nil,
            manifest: manifest, toolchains: ["swift"]
        ))
    }

    private func target(
        label: String,
        kind: String,
        language: String,
        cwd: URL,
        command: String,
        source: String,
        entrypoint: String?,
        manifest: URL?,
        toolchains: [String]
    ) -> ProjectRunTarget {
        let identity = "\(kind)\u{0}\(cwd.path)\u{0}\(command)"
        let digest = SHA256.hash(data: Data(identity.utf8)).prefix(10)
        let id = digest.map { String(format: "%02x", $0) }.joined()
        return .init(
            id: id, label: label, kind: kind, language: language, cwd: cwd.path,
            command: command, source: source, isDefault: false, entrypoint: entrypoint,
            manifestPath: manifest?.path, requiredToolchains: toolchains
        )
    }

    private func deduplicated(_ targets: [ProjectRunTarget]) -> [ProjectRunTarget] {
        var seen: Set<String> = []
        return targets.filter { seen.insert($0.id).inserted }
    }

    private func priority(_ target: ProjectRunTarget) -> Int {
        let command = target.command.lowercased()
        if command.contains("npm run dev") || command == "pnpm dev" || command == "yarn dev" { return 100 }
        if command.contains("npm run start") || command == "pnpm start" || command == "yarn start" { return 95 }
        if command.contains("spring-boot:run") { return 92 }
        if command.contains("bootrun") { return 90 }
        if command.contains("main.py") { return 88 }
        if command == "go run ." { return 85 }
        if command == "cargo run" || command == "swift run" { return 84 }
        if command.contains("test") { return 40 }
        return 70
    }

    private func configurationFile(_ url: URL, root: URL) -> ProjectRunConfigurationFile {
        let data = try? Data(contentsOf: url, options: [.mappedIfSafe])
        let preview = data.flatMap { data -> String? in
            guard data.count <= 128 * 1_024, !data.prefix(8_000).contains(0) else { return nil }
            return String(String(decoding: data, as: UTF8.self).prefix(12_000))
        }
        let resolvedURL = url.standardizedFileURL.resolvingSymlinksInPath()
        let resolvedRoot = root.standardizedFileURL.resolvingSymlinksInPath()
        let rootPrefix = resolvedRoot.path.hasSuffix("/") ? resolvedRoot.path : resolvedRoot.path + "/"
        let relative = resolvedURL.path.hasPrefix(rootPrefix)
            ? String(resolvedURL.path.dropFirst(rootPrefix.count))
            : resolvedURL.path
        return .init(
            kind: url.lastPathComponent.lowercased(), label: url.lastPathComponent,
            path: relative, preview: preview, source: "本机项目"
        )
    }

    private func discoverToolchains() -> [String: [ProjectRunToolchainOption]] {
        let candidates: [String: [String]] = [
            "node": ["/opt/homebrew/bin/node", "/usr/local/bin/node", "/usr/bin/node"],
            "npm": ["/opt/homebrew/bin/npm", "/usr/local/bin/npm", "/usr/bin/npm"],
            "pnpm": ["/opt/homebrew/bin/pnpm", "/usr/local/bin/pnpm"],
            "yarn": ["/opt/homebrew/bin/yarn", "/usr/local/bin/yarn"],
            "python": ["/opt/homebrew/bin/python3", "/usr/local/bin/python3", "/usr/bin/python3"],
            "cargo": [FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".cargo/bin/cargo").path],
            "go": ["/opt/homebrew/bin/go", "/usr/local/bin/go", "/usr/bin/go"],
            "java": ["/usr/bin/java", "/opt/homebrew/bin/java"],
            "mvn": ["/opt/homebrew/bin/mvn", "/usr/local/bin/mvn"],
            "gradle": ["/opt/homebrew/bin/gradle", "/usr/local/bin/gradle"],
            "swift": ["/usr/bin/swift", "/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/bin/swift"],
        ]
        return candidates.reduce(into: [:]) { result, item in
            let options = item.value.filter(FileManager.default.isExecutableFile(atPath:)).map { path in
                ProjectRunToolchainOption(
                    id: path, kind: item.key, label: URL(fileURLWithPath: path).lastPathComponent,
                    version: nil, path: path, source: "本机", isDefault: false
                )
            }
            if !options.isEmpty { result[item.key] = options }
        }
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
