fn main() {
    configure_windows_sdl3_link();
    configure_windows_antihooking_link();
    configure_moonlight_common_link();
    tauri_build::build();
}

fn configure_windows_sdl3_link() {
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
    let native_lib_dir = repo_root
        .join("libs")
        .join("windows")
        .join("lib")
        .join(arch);
    println!(
        "cargo:rustc-link-search=native={}",
        native_lib_dir.display()
    );
    println!("cargo:rustc-link-lib=dylib=SDL3");
}

fn configure_windows_antihooking_link() {
    println!("cargo:rustc-check-cfg=cfg(antihooking_linked)");
    println!("cargo:rerun-if-env-changed=ANTIHOOKING_LIB_DIR");

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
    let lib_dir = std::env::var("ANTIHOOKING_LIB_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| detect_antihooking_lib_dir(repo_root, arch));
    let Some(lib_dir) = lib_dir else {
        return;
    };

    println!("cargo:rustc-link-search=native={}", lib_dir.display());
    println!("cargo:rustc-link-lib=dylib=AntiHooking");
    println!("cargo:rustc-cfg=antihooking_linked");
}

fn detect_antihooking_lib_dir(
    repo_root: &std::path::Path,
    arch: &str,
) -> Option<std::path::PathBuf> {
    [
        repo_root
            .join("build")
            .join(format!("build-{arch}-release"))
            .join("AntiHooking")
            .join("release"),
        repo_root
            .join("build")
            .join(format!("build-{arch}-debug"))
            .join("AntiHooking")
            .join("debug"),
    ]
    .into_iter()
    .find(|candidate| candidate.join("AntiHooking.lib").exists())
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
    println!("cargo:rustc-link-lib=dylib=avcodec");
    println!("cargo:rustc-link-lib=dylib=avutil");
    println!("cargo:rustc-link-lib=dylib=opus");
    println!("cargo:rustc-link-lib=dylib=swscale");
}
