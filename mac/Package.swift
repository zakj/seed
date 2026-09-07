// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "Seed",
    platforms: [.macOS("15.0")],
    dependencies: [
        .package(url: "https://github.com/apple/swift-markdown.git", from: "0.8.0")
    ],
    targets: [
        .target(
            name: "SeedKit",
            dependencies: [.product(name: "Markdown", package: "swift-markdown")]
        ),
        .executableTarget(name: "Seed", dependencies: ["SeedKit"]),
        .testTarget(name: "SeedKitTests", dependencies: ["SeedKit"]),
    ]
)
