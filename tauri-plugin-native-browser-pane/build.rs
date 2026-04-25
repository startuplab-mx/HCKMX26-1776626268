const COMMANDS: &[&str] = &["open", "navigate", "set_bounds", "close"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .ios_path("ios")
        .android_path("android")
        .build();
}
