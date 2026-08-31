@preconcurrency import AppKit
import Foundation
import MCP

@main
enum VisualComputerUseMCPMain {
    @MainActor
    static func main() {
        let arguments = Array(CommandLine.arguments.dropFirst())

        do {
            if arguments.contains("--onboarding") {
                PermissionOnboarding.runStandalone()
                return
            }

            if arguments.contains("--doctor") {
                let data = try permissionDiagnosticsData()
                FileHandle.standardOutput.write(data)
                FileHandle.standardOutput.write(Data("\n".utf8))
                return
            }

            if let outputIndex = arguments.firstIndex(of: "--doctor-output"),
               arguments.indices.contains(outputIndex + 1) {
                let outputURL = URL(fileURLWithPath: arguments[outputIndex + 1])
                let data = try permissionDiagnosticsData()
                try data.write(to: outputURL, options: .atomic)
                return
            }

            let application = NSApplication.shared
            application.setActivationPolicy(.accessory)
            Task.detached {
                await runMCPServer()
            }
            application.run()
        } catch {
            fail(error)
        }
    }

    private static func permissionDiagnosticsData() throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        return try encoder.encode(PermissionSupport.diagnostics())
    }

    private nonisolated static func runMCPServer() async {
        do {
            let service = MCPService()
            let server = await service.makeServer()
            let transport = StdioTransport()
            try await server.start(transport: transport)
            await server.waitUntilCompleted()
            await MainActor.run {
                NSApp.terminate(nil)
            }
        } catch {
            fail(error)
        }
    }

    private nonisolated static func fail(_ error: Error) -> Never {
        let message =
            "visual-computer-use-mcp failed: \(error.localizedDescription)\n"
        FileHandle.standardError.write(Data(message.utf8))
        Foundation.exit(EXIT_FAILURE)
    }
}
