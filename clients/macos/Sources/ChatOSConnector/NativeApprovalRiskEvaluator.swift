import Foundation

struct NativeApprovalRisk: Sendable {
    var level: String
    var reason: String?
}

enum NativeApprovalRiskEvaluator {
    static func evaluate(command: String, arguments: [String]) -> NativeApprovalRisk {
        let text = ([command] + arguments).joined(separator: " ").lowercased()
        let highRiskMarkers = [
            "rm -rf", "sudo ", "mkfs", "diskutil erase", "dd if=", "shutdown",
            "reboot", "kill -9", "git reset --hard", "git clean -fd", "curl | sh",
            "curl | bash", "chmod -r 777", "> /dev/",
        ]
        if let marker = highRiskMarkers.first(where: text.contains) {
            return .init(level: "high", reason: "命令包含高风险操作：\(marker)")
        }
        let writeMarkers = [
            " rm ", " mv ", " cp ", "install ", "npm install", "cargo install",
            "git commit", "git push", "chmod ", "chown ", "mkdir ", "touch ",
        ]
        if writeMarkers.contains(where: { " \(text) ".contains($0) }) {
            return .init(level: "medium", reason: "命令可能修改文件、依赖或远程状态。")
        }
        return .init(level: "low", reason: nil)
    }
}
