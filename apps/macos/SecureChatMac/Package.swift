// swift-tools-version: 5.9

import PackageDescription

let package = Package(
    name: "SecureChatMac",
    platforms: [
        .macOS(.v14)
    ],
    products: [
        .executable(name: "SecureChatMac", targets: ["SecureChatMac"])
    ],
    targets: [
        .systemLibrary(
            name: "SecureChatFFI",
            path: "Sources/SecureChatFFI"
        ),
        .executableTarget(
            name: "SecureChatMac",
            dependencies: ["SecureChatFFI"],
            linkerSettings: [
                .linkedLibrary("secure_chat_ffi")
            ]
        )
    ]
)

