// swift-tools-version: 6.2

import PackageDescription

let package = Package(
    name: "ChatOSSwift",
    platforms: [
        .macOS(.v14),
    ],
    products: [
        .library(name: "ChatOSCore", targets: ["ChatOSCore"]),
        .library(name: "ChatOSAPI", targets: ["ChatOSAPI"]),
        .library(name: "ChatOSConnector", targets: ["ChatOSConnector"]),
        .executable(name: "ChatOSSwift", targets: ["ChatOSApp"]),
    ],
    targets: [
        .target(name: "ChatOSCore"),
        .target(
            name: "ChatOSAPI",
            dependencies: ["ChatOSCore"]
        ),
        .target(
            name: "ChatOSConnector",
            dependencies: ["ChatOSCore"],
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
            name: "ChatOSApp",
            dependencies: ["ChatOSCore", "ChatOSAPI", "ChatOSConnector"],
            linkerSettings: [
                .linkedFramework("ApplicationServices"),
                .linkedFramework("Carbon"),
                .linkedFramework("Security"),
                .linkedFramework("WebKit"),
                .linkedLibrary("sqlite3"),
            ]
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
            dependencies: ["ChatOSConnector", "ChatOSCore"]
        ),
        .testTarget(
            name: "ChatOSAppTests",
            dependencies: ["ChatOSApp", "ChatOSCore"]
        ),
    ]
)
