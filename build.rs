use std::path::PathBuf;

fn main() {
    // llama.cpp tree built with CMake (shared libs): ./llama.cpp (see scripts/build-llama.sh), or $LLAMA_DIR.
    let llama = std::env::var("LLAMA_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("llama.cpp"));
    let llama = llama.canonicalize().expect("llama.cpp directory not found");
    cc::Build::new()
        .file("shim.c")
        .include(llama.join("include"))
        .include(llama.join("ggml/include"))
        .compile("pt_shim");
    let lib = llama.join("build/bin");
    println!("cargo:rustc-link-search=native={}", lib.display());
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", lib.display());
    for l in ["llama", "ggml", "ggml-base"] {
        println!("cargo:rustc-link-lib=dylib={l}");
    }
    println!("cargo:rerun-if-changed=shim.c");
    println!("cargo:rerun-if-env-changed=LLAMA_DIR");
}
