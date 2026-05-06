// swift-tools-version: 5.10
import PackageDescription

// To enable Kokoro CoreML TTS:
//   1. Clone https://github.com/mattmireles/kokoro-coreml into a sibling directory
//      (e.g. ../third_party/kokoro-coreml)
//   2. Uncomment the dependency and target-dependency lines below.
//   3. Rebuild.
//
// Note: kokoro-coreml's Package.swift lives in swift/Package.swift, so we
// reference the swift/ subdirectory via a local path.

let package = Package(
    name: "TranslatorVirtualMicHost",
    platforms: [.macOS(.v14)],
    products: [
        .executable(name: "TranslatorVirtualMicHost", targets: ["TranslatorVirtualMicHost"]),
    ],
    // dependencies: [
    //     .package(path: "../third_party/kokoro-coreml/swift"),
    // ],
    targets: [
        .executableTarget(
            name: "TranslatorVirtualMicHost",
            path: "Sources",
            // dependencies: [
            //     .product(name: "KokoroPipeline", package: "swift"),
            // ],
            linkerSettings: [
                .linkedFramework("SwiftUI"),
                .linkedFramework("AppKit"),
                .linkedFramework("AVFoundation"),
                .linkedFramework("CoreAudio"),
                .linkedFramework("CoreMedia"),
                .linkedFramework("AudioToolbox"),
            ]
        ),
    ]
)
