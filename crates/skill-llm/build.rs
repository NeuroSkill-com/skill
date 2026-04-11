// Ensure static linking of libllama.a for the skill-llm crate
fn main() {
    println!("cargo:rustc-link-lib=static=llama");
    
    // Try to locate libllama.a in common build directories
    let search_paths = [
        "src-tauri/target/llama-cmake-cache/*/lib",
        "target/llama-cmake-cache/*/lib",
        "../target/llama-cmake-cache/*/lib",
    ];
    
    for pattern in search_paths {
        if let Ok(paths) = glob::glob(pattern) {
            for path in paths.flatten() {
                if path.join("libllama.a").exists() {
                    println!("cargo:rustc-link-search=native={}", path.display());
                    return;
                }
            }
        }
    }
    
    // Fallback: Print a warning if libllama.a is not found
    println!("cargo:warning=libllama.a not found in common build directories. Static linking may fail.");
}