// swift-tools-version: 6.1

import PackageDescription

let package = Package(
    name: "visual-computer-use-mcp",
    platforms: [
        .macOS(.v13)
    ],
    products: [
        .executable(
            name: "visual-computer-use-mcp",
            targets: ["VisualComputerUseMCP"]
        )
    ],
    dependencies: [
        .package(
            url: "https://github.com/modelcontextprotocol/swift-sdk.git",
            exact: "0.12.1"
        )
    ],
    targets: [
        .executableTarget(
            name: "VisualComputerUseMCP",
            dependencies: [
                .product(name: "MCP", package: "swift-sdk")
            ],
            linkerSettings: [
                .linkedFramework("ApplicationServices"),
                .linkedFramework("AppKit"),
                .linkedFramework("CoreGraphics"),
                .linkedFramework("ImageIO"),
                .linkedFramework("UniformTypeIdentifiers")
            ]
        ),
        .testTarget(
            name: "VisualComputerUseMCPTests",
            dependencies: ["VisualComputerUseMCP"]
        )
    ]
)
