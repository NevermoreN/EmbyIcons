fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        // Emby's minimal container does not include Skia's font dependencies.
        // Keep the packaged libraries private to this plugin, including their
        // transitive dependencies. Do not change the host's library search path.
        println!(
            "cargo:rustc-link-arg-cdylib=-Wl,--disable-new-dtags,-rpath,$ORIGIN/embyicons-native"
        );
    }
}
