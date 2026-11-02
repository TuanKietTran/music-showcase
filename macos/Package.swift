// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "MusicShowcase",
    platforms: [
        .macOS(.v14)
    ],
    targets: [
        .executableTarget(
            name: "MusicShowcase",
            path: "Sources/MusicShowcase"
        )
    ]
)
