import ChatOSCodeReadWorkerProtocol
import CryptoKit
import Darwin
import Foundation

struct ProjectRunWorkerAnalyzer {
    private static let maximumDirectories = 2_500
    private static let maximumDepth = 6
    private static let defaultMaximumChildrenPerDirectory = 10_000
    private static let maximumManifestBytes = 2 * 1_024 * 1_024
    private static let maximumConfigurationPreviewBytes = 128 * 1_024
    private static let ignoredDirectories: Set<String> = [
        ".chatos", ".git", ".idea", ".next", ".venv", ".vscode", "build", "dist",
        "node_modules", "target", "venv", "vendor",
    ]
    private static let detectionFileNames = [
        "app.py", "build.gradle", "build.gradle.kts", "Cargo.toml", "go.mod",
        "gradlew", "main.py", "mvnw", "package.json", "Package.swift",
        "pnpm-lock.yaml", "pom.xml", "pyproject.toml", "pytest.ini",
        "requirements.txt", "yarn.lock",
    ]

    let root: URL
    let deadlineUnixMilliseconds: Int64

    func analyze(
        _ arguments: [String: CodeReadWorkerJSONValue]
    ) throws -> CodeReadWorkerJSONValue {
        let maximumChildrenPerDirectory = min(
            max(Int(number(arguments["max_children_per_directory"]) ?? Double(
                Self.defaultMaximumChildrenPerDirectory
            )), 1),
            Self.defaultMaximumChildrenPerDirectory
        )
        var queue: [(URL, Int)] = [(root, 0)]
        var cursor = 0
        var visited = 0
        var targets: [WorkerRunTarget] = []
        var configurationURLs: Set<URL> = []

        while cursor < queue.count,
              visited < Self.maximumDirectories,
              targets.count < 100 {
            try checkDeadline()
            let (directory, depth) = queue[cursor]
            cursor += 1
            visited += 1
            var files = try knownDetectionFiles(in: directory)
            let children = FileManager.default.enumerator(
                at: directory,
                includingPropertiesForKeys: [.isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey],
                options: [.skipsSubdirectoryDescendants],
                errorHandler: { _, _ in false }
            )
            var inspectedChildren = 0
            while let child = children?.nextObject() as? URL {
                try checkDeadline()
                guard inspectedChildren < maximumChildrenPerDirectory else { break }
                inspectedChildren += 1
                let values = try? child.resourceValues(forKeys: [
                    .isDirectoryKey, .isRegularFileKey, .isSymbolicLinkKey,
                ])
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
            try detectNode(in: directory, files: files, output: &targets, configs: &configurationURLs)
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
        let configurationFiles = try configurationURLs
            .sorted { $0.path < $1.path }
            .prefix(60)
            .map(configurationFile)
        return .object([
            "targets": .array(targets.map(\.jsonValue)),
            "configuration_files": .array(configurationFiles),
            "toolchain_options": .object(discoverToolchains()),
            "visited_directories": .number(Double(visited)),
        ])
    }

    private func knownDetectionFiles(in directory: URL) throws -> [String: URL] {
        var files: [String: URL] = [:]
        for name in Self.detectionFileNames {
            try checkDeadline()
            let candidate = directory.appendingPathComponent(name, isDirectory: false)
            let values = try? candidate.resourceValues(forKeys: [.isRegularFileKey, .isSymbolicLinkKey])
            guard values?.isRegularFile == true, values?.isSymbolicLink != true else { continue }
            files[name.lowercased()] = candidate
        }
        return files
    }

    private func detectNode(
        in directory: URL,
        files: [String: URL],
        output: inout [WorkerRunTarget],
        configs: inout Set<URL>
    ) throws {
        guard let manifest = files["package.json"] else { return }
        configs.insert(manifest)
        guard let data = try boundedData(at: manifest, maximumBytes: Self.maximumManifestBytes),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return }
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
        output: inout [WorkerRunTarget],
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
        output: inout [WorkerRunTarget],
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
        output: inout [WorkerRunTarget],
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
        output: inout [WorkerRunTarget],
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
        output: inout [WorkerRunTarget],
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
    ) -> WorkerRunTarget {
        let identity = "\(kind)\u{0}\(cwd.path)\u{0}\(command)"
        let digest = SHA256.hash(data: Data(identity.utf8)).prefix(10)
        return .init(
            id: digest.map { String(format: "%02x", $0) }.joined(),
            label: label,
            kind: kind,
            language: language,
            cwd: cwd.path,
            command: command,
            source: source,
            isDefault: false,
            entrypoint: entrypoint,
            manifestPath: manifest?.path,
            requiredToolchains: toolchains
        )
    }

    private func deduplicated(_ targets: [WorkerRunTarget]) -> [WorkerRunTarget] {
        var seen: Set<String> = []
        return targets.filter { seen.insert($0.id).inserted }
    }

    private func priority(_ target: WorkerRunTarget) -> Int {
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

    private func configurationFile(_ url: URL) throws -> CodeReadWorkerJSONValue {
        try checkDeadline()
        let data = try boundedData(at: url, maximumBytes: Self.maximumConfigurationPreviewBytes)
        let preview = data.flatMap { data -> String? in
            guard !data.prefix(8_000).contains(0) else { return nil }
            return String(String(decoding: data, as: UTF8.self).prefix(12_000))
        }
        return .object([
            "kind": .string(url.lastPathComponent.lowercased()),
            "label": .string(url.lastPathComponent),
            "path": .string(relativePath(url)),
            "preview": preview.map(CodeReadWorkerJSONValue.string) ?? .null,
            "source": .string("本机项目"),
        ])
    }

    private func boundedData(at url: URL, maximumBytes: Int) throws -> Data? {
        try checkDeadline()
        guard maximumBytes > 0 else { return nil }
        let descriptor = url.path.withCString {
            Darwin.open($0, O_RDONLY | O_NOFOLLOW | O_CLOEXEC)
        }
        guard descriptor >= 0 else { return nil }
        defer { _ = Darwin.close(descriptor) }
        var status = stat()
        guard fstat(descriptor, &status) == 0,
              (status.st_mode & S_IFMT) == S_IFREG,
              status.st_size >= 0,
              status.st_size <= maximumBytes else { return nil }
        var data = Data()
        data.reserveCapacity(Int(status.st_size))
        var buffer = [UInt8](repeating: 0, count: 64 * 1_024)
        while true {
            try checkDeadline()
            let count = Darwin.read(descriptor, &buffer, buffer.count)
            if count == 0 { break }
            if count < 0, errno == EINTR { continue }
            guard count > 0, data.count <= maximumBytes - count else { return nil }
            data.append(contentsOf: buffer[0..<count])
        }
        return data
    }

    private func discoverToolchains() -> [String: CodeReadWorkerJSONValue] {
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
                CodeReadWorkerJSONValue.object([
                    "id": .string(path),
                    "kind": .string(item.key),
                    "label": .string(URL(fileURLWithPath: path).lastPathComponent),
                    "version": .null,
                    "path": .string(path),
                    "source": .string("本机"),
                    "is_default": .bool(false),
                ])
            }
            if !options.isEmpty { result[item.key] = .array(options) }
        }
    }

    private func relativePath(_ url: URL) -> String {
        let resolved = url.standardizedFileURL.resolvingSymlinksInPath()
        if resolved.path == root.path { return "." }
        let prefix = root.path.hasSuffix("/") ? root.path : root.path + "/"
        return resolved.path.hasPrefix(prefix) ? String(resolved.path.dropFirst(prefix.count)) : "."
    }

    private func checkDeadline() throws {
        if Int64(Date().timeIntervalSince1970 * 1_000) >= deadlineUnixMilliseconds {
            throw CodeReadWorkerCoreError.deadlineExceeded
        }
    }

    private func number(_ value: CodeReadWorkerJSONValue?) -> Double? {
        guard case let .number(number)? = value else { return nil }
        return number
    }
}

private struct WorkerRunTarget {
    var id: String
    var label: String
    var kind: String
    var language: String
    var cwd: String
    var command: String
    var source: String
    var isDefault: Bool
    var entrypoint: String?
    var manifestPath: String?
    var requiredToolchains: [String]

    var jsonValue: CodeReadWorkerJSONValue {
        .object([
            "id": .string(id),
            "label": .string(label),
            "kind": .string(kind),
            "language": .string(language),
            "cwd": .string(cwd),
            "command": .string(command),
            "source": .string(source),
            "is_default": .bool(isDefault),
            "entrypoint": entrypoint.map(CodeReadWorkerJSONValue.string) ?? .null,
            "manifest_path": manifestPath.map(CodeReadWorkerJSONValue.string) ?? .null,
            "required_toolchains": .array(requiredToolchains.map(CodeReadWorkerJSONValue.string)),
        ])
    }
}
