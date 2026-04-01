// swift-tools-version: 5.10
import PackageDescription

let package = Package(
    name: "TranslatorVirtualMicHost",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "TranslatorVirtualMicHost", targets: ["TranslatorVirtualMicHost"]),
    ],
    targets: [
        .executableTarget(
            name: "TranslatorVirtualMicHost",
            path: "Sources",
            linkerSettings: [
                .linkedFramework("SwiftUI"),
                .linkedFramework("AppKit"),
                .linkedFramework("AVFoundation"),
                .linkedFramework("CoreAudio"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("AudioToolbox"),
            ]
        )
    ]
)
