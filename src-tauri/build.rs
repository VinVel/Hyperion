/*
 * Copyright (c) 2026 VinVel
 *
 * SPDX-License-Identifier: AGPL-3.0-only
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, version 3 only.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program. If not, see <https://www.gnu.org/licenses/>.
 *
 * Project home: hyperion.velcore.net
 */

fn main() {
    #[allow(
        unused_mut,
        reason = "On Windows this must me mutable. For consistency it now is everywhere."
    )]
    let mut attributes = tauri_build::Attributes::new();
    if target_is_windows_msvc() {
        attributes = attributes
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest());
        add_manifest();
    }
    tauri_build::try_build(attributes).unwrap();
}

fn target_is_windows_msvc() -> bool {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = std::env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();

    target_os == "windows" && target_env == "msvc"
}

// Embed the manifest for Windows MSVC targets to fix 0xc0000139 in tests
fn add_manifest() {
    static WINDOWS_MANIFEST_FILE: &str = "windows-app-manifest.xml";

    let manifest = std::env::current_dir().unwrap().join(WINDOWS_MANIFEST_FILE);

    println!("cargo:rerun-if-changed={}", manifest.display());
    // Embed the Windows application manifest file.
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!(
        "cargo:rustc-link-arg=/MANIFESTINPUT:{}",
        manifest.to_str().unwrap()
    );
    // Turn linker warnings into errors.
    println!("cargo:rustc-link-arg=/WX");
}
