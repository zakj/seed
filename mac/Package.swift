// swift-tools-version: 6.0
import PackageDescription

let package = Package(
    name: "Seed",
    platforms: [.macOS("15.0")],
    dependencies: [
        .package(url: "https://github.com/gonzalezreal/textual.git", from: "0.5.0"),
    ],
    targets: [
        .target(name: "SeedKit"),
        .executableTarget(
            name: "Seed",
            dependencies: ["SeedKit", .product(name: "Textual", package: "textual")]
        ),
        .testTarget(name: "SeedKitTests", dependencies: ["SeedKit"]),
    ]
)
