import ChatOSCore
import AppKit
import SwiftUI

enum LocalConnectorCustomMCPExamples {
    static let rea = LocalConnectorCustomMCPDraft(
        displayName: "Reverse Engineer Anything",
        description: "Reverse engineering tools exposed through REA MCP.",
        componentKey: "reverse-engineer-anything",
        packageName: "rea-agents",
        packageVersion: "6.5.0",
        binaryName: "rea",
        arguments: ["mcp"],
        accessProfile: .developerAnalysis
    )
}
struct CustomPluginInstallerSheet: View {
    private enum InstallKind: String, CaseIterable, Identifiable {
        case npmMCP
        case pluginFolder
        case gitPlugin
        case npmPlugin
        var id: String { rawValue }
    }

    @EnvironmentObject private var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @ObservedObject var viewModel: LocalConnectorControlCenterViewModel
    @State private var kind: InstallKind = .npmMCP
    @State private var displayName = ""
    @State private var description = ""
    @State private var componentKey = ""
    @State private var packageName = ""
    @State private var packageVersion = ""
    @State private var binaryName = ""
    @State private var argumentsText = ""
    @State private var accessProfile: LocalConnectorCustomMCPDraft.AccessProfile = .restricted
    @State private var folderPath = ""
    @State private var gitReference = ""
    @State private var subdirectory = ""
    @State private var inspection: LocalConnectorPluginInspection?
    @State private var errorMessage: String?

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text(model.localized("安装自定义 MCP / Plugin", english: "Install Custom MCP / Plugin"))
                        .font(.title2.weight(.semibold))
                    Text(model.localized(
                        "先隔离预检工具清单，再确认写入本机安装记录。",
                        english: "Inspect the tool list in isolation before saving the installation locally."
                    ))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button(
                    model.localized("交给安装助手", english: "Use Installation Assistant"),
                    systemImage: "bubble.left.and.text.bubble.right"
                ) {
                    dismiss()
                    model.selection = .contact(WorkspaceSystemContact.pluginInstallerID)
                }
                Button(model.localized("关闭", english: "Close")) { dismiss() }
            }
            .padding(20)

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    Picker("", selection: $kind) {
                        Text(model.localized("从 npm 安装 MCP", english: "MCP from npm"))
                            .tag(InstallKind.npmMCP)
                        Text(model.localized("导入插件 / MCP 目录", english: "Plugin / MCP Folder"))
                            .tag(InstallKind.pluginFolder)
                        Text(model.localized("Git 插件", english: "Git Plugin"))
                            .tag(InstallKind.gitPlugin)
                        Text(model.localized("npm 插件", english: "npm Plugin"))
                            .tag(InstallKind.npmPlugin)
                    }
                    .pickerStyle(.segmented)

                    if kind == .npmMCP {
                        npmForm
                    } else if kind == .pluginFolder {
                        folderForm
                    } else {
                        remotePluginForm
                    }

                    Label(
                        preflightSafetyText,
                        systemImage: "lock.shield"
                    )
                    .font(.caption)
                    .foregroundStyle(.secondary)

                    if let errorMessage {
                        Text(errorMessage)
                            .font(.caption)
                            .foregroundStyle(.red)
                            .textSelection(.enabled)
                    }

                    if let inspection {
                        inspectionView(inspection)
                    }
                }
                .padding(20)
            }

            Divider()
            HStack {
                if viewModel.isInspectingCustomPlugin || viewModel.isInstallingCustomPlugin {
                    ProgressView().controlSize(.small)
                }
                Spacer()
                Button(model.localized("预检工具", english: "Inspect Tools")) { inspect() }
                    .disabled(isBusy || !canInspect)
                Button(model.localized("确认安装", english: "Confirm Install")) { install() }
                    .buttonStyle(.borderedProminent)
                    .disabled(isBusy || inspection == nil)
            }
            .padding(16)
        }
        .frame(minWidth: 680, idealWidth: 760, minHeight: 620, idealHeight: 720)
        .onChange(of: inputSignature) { _, _ in
            inspection = nil
            errorMessage = nil
        }
    }

    private var npmForm: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text(model.localized("固定版本 npm MCP", english: "Pinned npm MCP"))
                    .font(.headline)
                Spacer()
                Button(model.localized("填入 REA 示例", english: "Use REA Example")) {
                    let example = LocalConnectorCustomMCPExamples.rea
                    displayName = example.displayName
                    description = example.description
                    componentKey = example.componentKey
                    packageName = example.packageName
                    packageVersion = example.packageVersion
                    binaryName = example.binaryName
                    argumentsText = example.arguments.joined(separator: "\n")
                    accessProfile = example.accessProfile
                }
            }
            HStack {
                field(model.localized("显示名称", english: "Display Name"), text: $displayName)
                field(model.localized("组件 Key", english: "Component Key"), text: $componentKey)
            }
            HStack {
                field(model.localized("npm 包名", english: "npm Package"), text: $packageName)
                field(model.localized("精确版本", english: "Exact Version"), text: $packageVersion)
                field(model.localized("Bin 名称", english: "Bin Name"), text: $binaryName)
            }
            field(model.localized("说明", english: "Description"), text: $description)
            VStack(alignment: .leading, spacing: 5) {
                Text(model.localized("访问级别", english: "Access Profile"))
                    .font(.caption.weight(.medium))
                Picker("", selection: $accessProfile) {
                    Text(model.localized("受限（仅本地）", english: "Restricted (Local Only)"))
                        .tag(LocalConnectorCustomMCPDraft.AccessProfile.restricted)
                    Text(model.localized("网络客户端", english: "Network Client"))
                        .tag(LocalConnectorCustomMCPDraft.AccessProfile.networkClient)
                    Text(model.localized("开发与逆向分析", english: "Developer Analysis"))
                        .tag(LocalConnectorCustomMCPDraft.AccessProfile.developerAnalysis)
                }
                .pickerStyle(.segmented)
                Text(accessProfileDescription)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            VStack(alignment: .leading, spacing: 5) {
                Text(model.localized("参数（每行一个，不经过 shell）", english: "Arguments (one per line, no shell parsing)"))
                    .font(.caption.weight(.medium))
                TextEditor(text: $argumentsText)
                    .font(.system(.body, design: .monospaced))
                    .frame(height: 82)
                    .padding(5)
                    .background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: 7))
            }
        }
    }

    private var folderForm: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(model.localized(
                "选择插件或 MCP 的根目录",
                english: "Choose the plugin or MCP root folder"
            ))
                .font(.headline)
            Text(model.localized(
                "ChatOS 会自动识别 ChatOS Plugin、Agent Plugin、Codex、Claude、独立 Skill 和主流应用的 MCP JSON 配置。请选择清单文件所在的这一层。",
                english: "ChatOS recognizes ChatOS, portable Agent, Codex and Claude Plugins, standalone Skills, and MCP JSON configurations from mainstream agent apps. Choose the folder that contains the manifest."
            ))
                .font(.caption)
                .foregroundStyle(.secondary)

            VStack(alignment: .leading, spacing: 6) {
                Label(
                    model.localized("支持自动识别", english: "Recognized automatically"),
                    systemImage: "checkmark.circle.fill"
                )
                .font(.caption.weight(.semibold))
                .foregroundStyle(.green)
                Text("SKILL.md  ·  plugin.json  ·  chatos.plugin.json  ·  mcp.json  ·  .mcp.json\n.codex-plugin  ·  .claude-plugin  ·  .cursor/mcp.json  ·  .vscode/mcp.json")
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
            }
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.secondary.opacity(0.06), in: RoundedRectangle(cornerRadius: 9))

            HStack {
                TextField(
                    model.localized(
                        "例如：/Users/me/Downloads/my-plugin",
                        english: "Example: /Users/me/Downloads/my-plugin"
                    ),
                    text: $folderPath
                )
                    .textFieldStyle(.roundedBorder)
                Button(
                    model.localized("选择根目录…", english: "Choose Root Folder…")
                ) { chooseFolder() }
            }
        }
    }

    private var remotePluginForm: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(kind == .gitPlugin
                ? model.localized("从 Git 仓库安装 Agent Plugin", english: "Agent Plugin from Git")
                : model.localized("从 npm 安装完整 Agent Plugin", english: "Complete Agent Plugin from npm"))
                .font(.headline)
            Text(model.localized(
                "适用于包含 Plugin 清单、SKILL.md 或 MCP 配置的完整包；如果 npm 包只是一个 MCP 命令，请改用“从 npm 安装 MCP”。",
                english: "Use this for a complete package containing a plugin manifest, SKILL.md, or MCP configuration. If the npm package only exposes an MCP command, use “MCP from npm” instead."
            ))
                .font(.caption)
                .foregroundStyle(.secondary)
            if kind == .gitPlugin {
                field(model.localized("HTTPS Git 仓库 URL", english: "HTTPS Git Repository URL"), text: $folderPath)
                HStack {
                    field(model.localized("分支 / Tag / Commit（可选）", english: "Branch / Tag / Commit (Optional)"), text: $gitReference)
                    field(model.localized("插件子目录（可选）", english: "Plugin Subdirectory (Optional)"), text: $subdirectory)
                }
            } else {
                HStack {
                    field(model.localized("npm 包名", english: "npm Package"), text: $packageName)
                    field(model.localized("精确版本", english: "Exact Version"), text: $packageVersion)
                }
                field(model.localized("插件子目录（可选）", english: "Plugin Subdirectory (Optional)"), text: $subdirectory)
            }
        }
    }

    private var preflightSafetyText: String {
        if kind == .npmMCP {
            return model.localized(
                "预检不会运行 npm install scripts；命令和参数不会作为 shell 字符串解析。",
                english: "Preflight never runs npm install scripts. Commands and arguments are never parsed as a shell string."
            )
        }
        return kind == .pluginFolder
            ? model.localized(
                "导入前会复制到隔离目录并检查清单、路径和实际 MCP 工具；原目录不会被修改。",
                english: "Before import, ChatOS copies the package into isolation and verifies its manifest, paths, and actual MCP tools. The original folder is not modified."
            )
            : model.localized(
                "ChatOS 不运行 npm install scripts 或仓库脚本；Git/npm 内容会在隔离目录预检，并在确认安装时重新获取和核对指纹。",
                english: "ChatOS never runs npm install scripts or repository scripts. Git/npm content is inspected in isolation, then reacquired and fingerprinted again at install time."
            )
    }

    private func field(_ title: String, text: Binding<String>) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(title).font(.caption.weight(.medium))
            TextField(title, text: text).textFieldStyle(.roundedBorder)
        }
    }

    private func inspectionView(_ value: LocalConnectorPluginInspection) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Label(
                model.localized("预检通过", english: "Preflight Passed"),
                systemImage: "checkmark.shield.fill"
            )
            .font(.headline)
            .foregroundStyle(.green)
            Text("\(value.displayName) · \(value.version)")
                .font(.subheadline.weight(.semibold))
            Text(value.sourceSummary)
                .font(.caption.monospaced())
                .foregroundStyle(.secondary)
                .textSelection(.enabled)
            if !value.requestedPermissions.isEmpty {
                Text(model.localized("请求权限", english: "Requested permissions") + ": "
                    + value.requestedPermissions.joined(separator: ", "))
                    .font(.caption.monospaced())
                    .foregroundStyle(.orange)
                    .textSelection(.enabled)
            }
            ForEach(value.components) { component in
                VStack(alignment: .leading, spacing: 5) {
                    Text(componentTitle(component))
                        .font(.caption.weight(.semibold))
                    if component.kind == .mcp {
                        Text(component.toolNames.prefix(24).joined(separator: ", "))
                            .font(.caption.monospaced())
                            .foregroundStyle(.tertiary)
                            .textSelection(.enabled)
                        if component.toolNames.count > 24 {
                            Text("+\(component.toolNames.count - 24) more")
                                .font(.caption2)
                                .foregroundStyle(.tertiary)
                        }
                    }
                    if component.kind == .hook {
                        if !component.hookRuntimes.isEmpty {
                            Text("Runtime: " + component.hookRuntimes.joined(separator: ", "))
                                .font(.caption2.monospaced())
                                .foregroundStyle(.secondary)
                                .textSelection(.enabled)
                        }
                        Text(component.hookEvents.joined(separator: ", "))
                            .font(.caption.monospaced())
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                        if !component.hookAsyncEvents.isEmpty {
                            Text("Background: " + component.hookAsyncEvents.joined(separator: ", "))
                                .font(.caption2.monospaced())
                                .foregroundStyle(.secondary)
                                .textSelection(.enabled)
                        }
                        ForEach(Array(component.hookCommands.enumerated()), id: \.offset) { _, command in
                            Text(command)
                                .font(.caption2.monospaced())
                                .foregroundStyle(.tertiary)
                                .textSelection(.enabled)
                        }
                    }
                }
            }
            Text("SHA-256 \(value.fingerprint)")
                .font(.caption2.monospaced())
                .foregroundStyle(.tertiary)
                .textSelection(.enabled)
        }
        .padding(14)
        .background(.green.opacity(0.07), in: RoundedRectangle(cornerRadius: 10))
    }

    private func componentTitle(_ component: LocalConnectorPluginInspectionComponent) -> String {
        switch component.kind {
        case .skill:
            "\(component.displayName) · Skill"
        case .mcp:
            "\(component.displayName) · \(component.toolNames.count) tools"
        case .hook:
            "\(component.displayName) · \(component.hookDialect ?? "Hook") · \(component.hookEvents.count) events"
        }
    }

    private var isBusy: Bool {
        viewModel.isInspectingCustomPlugin || viewModel.isInstallingCustomPlugin
    }

    private var canInspect: Bool {
        if kind == .pluginFolder || kind == .gitPlugin {
            return !folderPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        }
        if kind == .npmPlugin {
            return [packageName, packageVersion]
                .allSatisfy { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
        }
        return [displayName, componentKey, packageName, packageVersion, binaryName]
            .allSatisfy { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
    }

    private var draft: LocalConnectorCustomMCPDraft {
        .init(
            displayName: displayName,
            description: description,
            componentKey: componentKey,
            packageName: packageName,
            packageVersion: packageVersion,
            binaryName: binaryName,
            arguments: argumentsText.split(whereSeparator: \.isNewline).map(String.init),
            environment: [:],
            accessProfile: accessProfile
        )
    }

    private var inputSignature: String {
        [kind.rawValue, displayName, description, componentKey, packageName, packageVersion,
         binaryName, argumentsText, accessProfile.rawValue, folderPath, gitReference,
         subdirectory].joined(separator: "\u{0}")
    }

    private var accessProfileDescription: String {
        switch accessProfile {
        case .restricted:
            model.localized(
                "只启动 MCP，不开放外网、开发工具、浏览器或项目文件。",
                english: "Starts the MCP without network, developer-tool, browser, or project access."
            )
        case .networkClient:
            model.localized(
                "允许 MCP 连接外部 API；仍不能读取项目或启动浏览器。",
                english: "Allows outbound API access, but not project reads or browser launches."
            )
        case .developerAnalysis:
            model.localized(
                "用于开发与逆向工具：可读项目、调用开发工具、启动受管浏览器并保存证据。",
                english: "For development and reverse engineering: project reads, developer tools, managed browser launches, and evidence artifacts."
            )
        }
    }

    private var sourceDraft: LocalConnectorPluginSourceDraft {
        switch kind {
        case .pluginFolder:
            .init(kind: .localFolder, location: folderPath, subdirectory: nil)
        case .gitPlugin:
            .init(
                kind: .gitRepository,
                location: folderPath,
                gitReference: gitReference,
                subdirectory: subdirectory
            )
        case .npmPlugin:
            .init(
                kind: .npmPackage,
                location: packageName,
                exactVersion: packageVersion,
                subdirectory: subdirectory
            )
        case .npmMCP:
            .init(kind: .npmPackage, location: packageName, exactVersion: packageVersion)
        }
    }

    private func inspect() {
        inspection = nil
        errorMessage = nil
        if kind == .npmMCP {
            viewModel.inspectCustomMCP(draft) { result in handleInspection(result) }
        } else {
            viewModel.inspectPluginSource(sourceDraft) { result in handleInspection(result) }
        }
    }

    private func handleInspection(_ result: Result<LocalConnectorPluginInspection, Error>) {
        switch result {
        case let .success(value): inspection = value
        case let .failure(error): errorMessage = error.localizedDescription
        }
    }

    private func install() {
        guard let inspection else { return }
        errorMessage = nil
        let completion: @MainActor (Result<Void, Error>) -> Void = { result in
            switch result {
            case .success: dismiss()
            case let .failure(error): errorMessage = error.localizedDescription
            }
        }
        if kind == .npmMCP {
            viewModel.installCustomMCP(
                draft,
                expectedFingerprint: inspection.fingerprint,
                completion: completion
            )
        } else {
            viewModel.installPluginSource(
                sourceDraft,
                expectedFingerprint: inspection.fingerprint,
                completion: completion
            )
        }
    }

    private func chooseFolder() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.prompt = model.localized("选择", english: "Choose")
        if panel.runModal() == .OK { folderPath = panel.url?.path ?? folderPath }
    }
}
