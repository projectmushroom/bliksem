// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "Bliksem",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(
            name: "Bliksem",
            path: "Sources/Bliksem",
            resources: [.copy("Resources/Px437_IBM_VGA_9x16.ttf")]
        ),
        .testTarget(name: "BliksemTests", dependencies: ["Bliksem"], path: "Tests/BliksemTests"),
    ]
)
