// swift-tools-version: 6.1
import PackageDescription

let package = Package(
    name: "RiftUIPackages",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "RiftUI", targets: ["RiftUISPM"]),
        .library(name: "RiftUICore", targets: ["RiftUICore"])
    ],
    targets: [
        .target(
            name: "CRiftMachBridge",
            path: "Modules/CRiftMachBridge",
            publicHeadersPath: "include"
        ),
        .target(
            name: "RiftUICore",
            dependencies: ["CRiftMachBridge"],
            path: "Modules/RiftUICore/Sources"
        ),
        .executableTarget(
            name: "RiftUISPM",
            dependencies: ["RiftUICore"],
            path: "App/Sources"
        ),
        .testTarget(
            name: "RiftUICoreTests",
            dependencies: ["RiftUICore"],
            path: "Tests/RiftUICoreTests"
        )
    ]
)
