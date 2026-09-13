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
        .executable(name: "ChatOSKeychainBroker", targets: ["ChatOSKeychainBroker"]),
        .executable(name: "ChatOSSwift", targets: ["ChatOSApp"]),
    ],
    targets: [
        .target(name: "ChatOSAgentRuntime", dependencies: ["ChatOSCore"]),
        .target(name: "ChatOSCore"),
        .target(
            name: "ChatOSMacSecurity",
            linkerSettings: [.linkedFramework("Security")]
        ),
        .target(
            name: "ChatOSAPI",
            dependencies: ["ChatOSCore", "ChatOSAgentRuntime"]
        ),
        .target(
            name: "ChatOSConnector",
            dependencies: ["ChatOSCore", "ChatOSAgentRuntime", "ChatOSMacSecurity"],
            linkerSettings: [
                .linkedFramework("ApplicationServices"),
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
            name: "ChatOSKeychainBroker",
            dependencies: ["ChatOSMacSecurity"],
            linkerSettings: [.linkedFramework("Security")]
        ),
        .executableTarget(
            name: "ChatOSApp",
            dependencies: ["ChatOSCore", "ChatOSAPI", "ChatOSConnector", "ChatOSAgentRuntime"],
            linkerSettings: [
                .linkedFramework("ApplicationServices"),
                .linkedFramework("Carbon"),
                .linkedFramework("Security"),
                .linkedFramework("WebKit"),
            ]
        ),
        .testTarget(
            name: "ChatOSAgentRuntimeTests",
            dependencies: ["ChatOSAgentRuntime", "ChatOSCore"]
        ),
        .testTarget(
            name: "ChatOSCoreTests",
            dependencies: ["ChatOSCore"]
        ),
        .testTarget(
            name: "ChatOSAPITests",
            dependencies: ["ChatOSAPI", "ChatOSCore"]
        ),
        .testTarget(
            name: "ChatOSConnectorTests",
            dependencies: ["ChatOSConnector", "ChatOSCore", "ChatOSKeychainBroker"]
        ),
        .testTarget(
            name: "ChatOSAppTests",
            dependencies: ["ChatOSApp", "ChatOSCore", "ChatOSKeychainBroker"]
        ),
    ]
)
