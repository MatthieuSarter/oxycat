//! Embeds the icon (and the version information) in the Windows executable.

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    // Checks the target, not the host, so that a cross-compiled Windows build gets its icon too.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("assets/icon.ico")
            .compile()
            .expect("embedding the Windows resources (icon) failed");
    }
}
