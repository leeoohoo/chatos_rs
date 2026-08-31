import ChatOSCore
import Foundation

enum LocalConnectorControlTab: String, CaseIterable, Identifiable {
    case connection = "设备配对"
    case plugins = "外挂程式"
    case terminal = "本机终端"
    case models = "模型配置"
    case approvals = "命令审批"
    case runtime = "运行与权限"
    case sandbox = "权限控制"

    var id: Self { self }

    func title(language: ChatOSLanguage) -> String {
        guard language == .english else { return rawValue }
        return switch self {
        case .connection: "Device & Gateway"
        case .plugins: "Plugins & Skills"
        case .terminal: "Local Terminal"
        case .models: "Model Configuration"
        case .approvals: "Command Approvals"
        case .runtime: "Runtime & Permissions"
        case .sandbox: "Access Control"
        }
    }

    var systemImage: String {
        switch self {
        case .connection: "externaldrive.connected.to.line.below"
        case .plugins: "puzzlepiece.extension"
        case .terminal: "terminal"
        case .models: "brain.head.profile"
        case .approvals: "checkmark.shield"
        case .runtime: "slider.horizontal.3"
        case .sandbox: "lock.shield"
        }
    }

    var eyebrow: String {
        switch self {
        case .connection: "CONNECTION"
        case .plugins: "PLUGINS & SKILLS"
        case .terminal: "LOCAL TERMINAL"
        case .models: "MODELS"
        case .approvals: "APPROVAL"
        case .runtime: "RUNTIME & PERMISSIONS"
        case .sandbox: "PERMISSION POLICY"
        }
    }

    var description: String {
        switch self {
        case .connection: "查看当前设备、网关长连接和本机能力边界。"
        case .plugins: "安装和管理运行在这台 Mac 上的 Plugin 与 Skills。"
        case .terminal: "通过与任务相同的审批和权限链路执行本机命令。"
        case .models: "同步云端模型，并检查命令审批模型是否可用。"
        case .approvals: "处理待审批操作，配置默认审批级别并查看审计历史。"
        case .runtime: "管理本机运行模式与 Skills、MCP 所需的系统权限。"
        case .sandbox: "控制任务可访问的文件、网络范围和审批策略。"
        }
    }

    func description(language: ChatOSLanguage) -> String {
        guard language == .english else { return description }
        return switch self {
        case .connection: "Inspect this device, its gateway connection, and local capability boundaries."
        case .plugins: "Install and manage plugins and skills running on this Mac."
        case .terminal: "Run local commands through the same approval and permission pipeline used by tasks."
        case .models: "Sync cloud models and verify that the command approval model is available."
        case .approvals: "Handle pending operations, configure default approval levels, and review audit history."
        case .runtime: "Manage runtime modes and the macOS permissions required by skills and MCP servers."
        case .sandbox: "Control the files, networks, and approval policies available to tasks."
        }
    }
}
