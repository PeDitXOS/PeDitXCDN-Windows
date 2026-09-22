fn main() {
    // Embed Windows manifest for admin elevation (requireAdministrator)
    tauri_build::try_build(tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new()
            .manifest(include_str!("PeDitXCDN.exe.manifest"))
        )
    ).expect("failed to build tauri app");
}
