// swift-tools-version:5.3

import PackageDescription

let package = Package(
  name: "tauri-plugin-native-browser-pane",
  platforms: [
    .macOS(.v10_13),
    .iOS(.v14),
  ],
  products: [
    .library(
      name: "tauri-plugin-native-browser-pane",
      type: .static,
      targets: ["tauri-plugin-native-browser-pane"]
    )
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-native-browser-pane",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources"
    )
  ]
)
