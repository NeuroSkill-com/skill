fn main() {
    // A build script's `#[cfg]` describes the HOST running it, not the target,
    // so cross-compiling from a Mac to Windows or Linux used to hand this
    // Objective-C file to the target's C compiler. Same fix as skill-calendar:
    // read CARGO_CFG_TARGET_OS (`cc` is an unconditional build-dependency).
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    {
        println!("cargo:rerun-if-changed=src/vision_ocr.m");

        // macOS's BSD ar doesn't support the -D (deterministic) flag that the
        // cc crate passes by default.  Use llvm-ar when available.
        let mut build = cc::Build::new();
        build.file("src/vision_ocr.m").flag("-fobjc-arc");
        if std::path::Path::new("/opt/homebrew/opt/llvm/bin/llvm-ar").exists() {
            build.archiver("/opt/homebrew/opt/llvm/bin/llvm-ar");
        }
        build.compile("vision_ocr");

        // Link required frameworks
        println!("cargo:rustc-link-lib=framework=Vision");
        println!("cargo:rustc-link-lib=framework=CoreGraphics");
        println!("cargo:rustc-link-lib=framework=Foundation");
    }
}
