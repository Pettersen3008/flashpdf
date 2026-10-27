fn main() {
    // Node resolves the N-API symbols at load time; the linker never sees them.
    // ponytail: macOS only, add the Windows delay-load hook when CI adds a Windows target.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-cdylib-link-arg=-Wl,-undefined,dynamic_lookup");
    }
}
