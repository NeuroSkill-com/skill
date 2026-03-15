fn main() {
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rerun-if-changed=src/vision_ocr.m");

        cc::Build::new()
            .file("src/vision_ocr.m")
            .flag("-fobjc-arc")
            .compile("vision_ocr");

        // Link required frameworks
        println!("cargo:rustc-link-lib=framework=Vision");
        println!("cargo:rustc-link-lib=framework=CoreGraphics");
        println!("cargo:rustc-link-lib=framework=Foundation");
    }
}
