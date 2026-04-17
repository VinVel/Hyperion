// swift-tools-version:5.3

import PackageDescription

let package = Package(
  name: "hyperion-mobile-plugins",
  platforms: [
    .macOS(.v10_13),
    .iOS(.v13),
  ],
  products: [
    .library(
      name: "hyperion-mobile-plugins",
      type: .static,
      targets: ["hyperion-mobile-plugins"]
    )
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "hyperion-mobile-plugins",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources"
    )
  ]
)
