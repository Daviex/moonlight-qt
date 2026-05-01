fn main() {
    configure_moonlight_common_link();
    tauri_build::build();
}

fn configure_moonlight_common_link() {
    println!("cargo:rerun-if-env-changed=MOONLIGHT_COMMON_C_LIB_DIR");
    println!("cargo:rerun-if-env-changed=MOONLIGHT_COMMON_C_STATIC");

    let Ok(lib_dir) = std::env::var("MOONLIGHT_COMMON_C_LIB_DIR") else {
        return;
    };

    println!("cargo:rustc-link-search=native={lib_dir}");
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
