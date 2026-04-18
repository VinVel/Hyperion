const COMMANDS: &[&str] = &["get_secret", "set_secret", "delete_secret"];

fn main() {
    tauri_plugin::Builder::new(COMMANDS)
        .android_path("android")
        .build();
}
