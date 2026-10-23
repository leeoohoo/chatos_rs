// swift-tools-version: 6.2

import PackageDescription

let package = Package(
    name: "ChatOSSwift",
    platforms: [
        .macOS(.v14),
    ],
    products: [
        .library(name: "ChatOSAgentRuntime", targets: ["ChatOSAgentRuntime"]),
        .library(name: "ChatOSCore", targets: ["ChatOSCore"]),
        .library(name: "ChatOSAPI", targets: ["ChatOSAPI"]),
        .library(name: "ChatOSConnector", targets: ["ChatOSConnector"]),
        .library(name: "ChatOSMediaWorkerProtocol", targets: ["ChatOSMediaWorkerProtocol"]),
        .executable(name: "ChatOSSwift", targets: ["ChatOSApp"]),
        .executable(name: "ChatOSMediaWorker", targets: ["ChatOSMediaWorker"]),
        .executable(name: "chatos_plugin_ps", targets: ["ChatOSPluginProcessList"]),
    ],
    dependencies: [
        .package(
            url: "https://github.com/migueldeicaza/SwiftTerm.git",
            exact: "1.20.0"
        ),
    ],
    targets: [
        .target(
            name: "ChatOSProcessRuntime",
            publicHeadersPath: "include"
        ),
        .target(name: "ChatOSNetworking"),
        .target(name: "ChatOSMediaWorkerProtocol"),
        .target(
            name: "ChatOSMediaWorkerCore",
            dependencies: ["ChatOSMediaWorkerProtocol"],
            linkerSettings: [
                .linkedFramework("AVFoundation"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("CoreGraphics"),
                .linkedFramework("ImageIO"),
            ]
        ),
        .target(name: "ChatOSModelCapabilities", resources: [.process("Resources")]),
        .target(
            name: "ChatOSAgentRuntime",
            dependencies: ["ChatOSNetworking", "ChatOSModelCapabilities"]
        ),
        .target(
            name: "ChatOSCore",
            dependencies: ["ChatOSModelCapabilities"],
            resources: [
                .process("Resources"),
                .copy("AgentSkills/Skills"),
                .copy("AgentSkills/Prompts"),
                .copy("AgentSkills/Catalog/ChatOSSkillCatalog.json"),
            ]
        ),
        .target(
            name: "ChatOSAPI",
            dependencies: ["ChatOSCore", "ChatOSAgentRuntime", "ChatOSNetworking"]
        ),
        .target(
            name: "ChatOSConnector",
            dependencies: [
                "ChatOSCore",
                "ChatOSAgentRuntime",
                "ChatOSNetworking",
                "ChatOSProcessRuntime",
                "ChatOSModelCapabilities",
                .product(name: "SwiftTerm", package: "SwiftTerm"),
            ],
            linkerSettings: [
                .linkedLibrary("sqlite3"),
                .linkedFramework("ApplicationServices"),
                .linkedFramework("LocalAuthentication"),
                .linkedFramework("AppKit"),
                .linkedFramework("AVFoundation"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("CoreVideo"),
                .linkedFramework("CoreGraphics"),
                .linkedFramework("ScreenCaptureKit"),
                .linkedFramework("Security"),
            ]
        ),
        .executableTarget(
            name: "ChatOSApp",
            dependencies: [
                "ChatOSCore",
                "ChatOSAPI",
                "ChatOSConnector",
                "ChatOSAgentRuntime",
                "ChatOSNetworking",
                "ChatOSMediaWorkerProtocol",
                .product(name: "SwiftTerm", package: "SwiftTerm"),
            ],
            linkerSettings: [
                .linkedFramework("ApplicationServices"),
                .linkedFramework("Carbon"),
                .linkedFramework("Security"),
                .linkedFramework("WebKit"),
                .linkedFramework("AVFoundation"),
                .linkedLibrary("sqlite3"),
            ]
        ),
        .executableTarget(
            name: "ChatOSMediaWorker",
            dependencies: ["ChatOSMediaWorkerCore", "ChatOSMediaWorkerProtocol"]
        ),
        .executableTarget(name: "ChatOSPluginProcessList"),
        .testTarget(
            name: "ChatOSAgentRuntimeTests",
            dependencies: ["ChatOSAgentRuntime"]
        ),
        .testTarget(
            name: "ChatOSCoreTests",
            dependencies: ["ChatOSCore", "ChatOSModelCapabilities"]
        ),
        .testTarget(
            name: "ChatOSAPITests",
            dependencies: ["ChatOSAPI", "ChatOSCore", "ChatOSNetworking"]
        ),
        .testTarget(
            name: "ChatOSConnectorTests",
            dependencies: ["ChatOSConnector", "ChatOSCore", "ChatOSNetworking"],
            resources: [.copy("Fixtures")]
        ),
        .testTarget(
            name: "ChatOSAppTests",
            dependencies: [
                "ChatOSApp",
                "ChatOSCore",
                "ChatOSNetworking",
                "ChatOSMediaWorkerCore",
                "ChatOSMediaWorkerProtocol",
            ]
        ),
        .testTarget(
            name: "ChatOSMediaWorkerTests",
            dependencies: ["ChatOSMediaWorkerCore", "ChatOSMediaWorkerProtocol"]
        ),
    ]
)
