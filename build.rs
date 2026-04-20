/// Emits the system Swift runtime search path required by macOS examples,
/// tests, and applications that link `iced_live_cast`.
fn main() {
    if std::env::var_os("CARGO_CFG_TARGET_OS").as_deref() != Some("macos".as_ref()) {
        return;
    }

    println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
}
