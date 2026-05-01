fn main() {
    configure_moonlight_common_link();
    tauri_build::build();
}

fn configure_moonlight_common_link() {
    println!("cargo:rustc-check-cfg=cfg(moonlight_common_c_linked)");
    println!("cargo:rerun-if-env-changed=MOONLIGHT_COMMON_C_LIB_DIR");
    println!("cargo:rerun-if-env-changed=MOONLIGHT_COMMON_C_STATIC");

    let Ok(lib_dir) = std::env::var("MOONLIGHT_COMMON_C_LIB_DIR") else {
        return;
    };

    println!("cargo:rustc-link-search=native={lib_dir}");
    configure_windows_media_link();
    println!("cargo:rustc-cfg=moonlight_common_c_linked");
    let link_kind = if std::env::var("MOONLIGHT_COMMON_C_STATIC")
        .map(|value| value == "1")
        .unwrap_or(false)
    {
        "static"
    } else {
        "dylib"
    };
    println!("cargo:rustc-link-lib={link_kind}=moonlight-common-c");
}

fn configure_windows_media_link() {
    if !cfg!(target_os = "windows") {
        return;
    }

    let manifest_dir = std::path::PathBuf::from(
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo"),
    );
    let Some(repo_root) = manifest_dir
        .parent()
        .and_then(std::path::Path::parent)
        .and_then(std::path::Path::parent)
    else {
        return;
    };
    let arch = match std::env::var("CARGO_CFG_TARGET_ARCH").ok().as_deref() {
        Some("aarch64") => "arm64",
        _ => "x64",
    };
    let media_lib_dir = repo_root
        .join("libs")
        .join("windows")
        .join("lib")
        .join(arch);
    println!("cargo:rustc-link-search=native={}", media_lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=opus");
}
