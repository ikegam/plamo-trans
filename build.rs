//! Builds the vendored llama.cpp (vendor/llama.cpp, patched) as static libraries and links them in,
//! so `cargo build --release` produces one self-contained `trs` binary.
//!
//! GPU backends:  --features cuda | vulkan      (Metal is used automatically on macOS)
//! Environment:
//!   PLAMO_PORTABLE=1     build for a baseline CPU instead of this machine (release binaries)
//!   PLAMO_CMAKE_ARGS     extra CMake arguments, space separated (e.g. -DCMAKE_CUDA_ARCHITECTURES=121)

use std::path::{Path, PathBuf};

fn main() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("vendor/llama.cpp");
    let os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();
    let cuda = std::env::var_os("CARGO_FEATURE_CUDA").is_some();
    let vulkan = std::env::var_os("CARGO_FEATURE_VULKAN").is_some();
    let portable = std::env::var("PLAMO_PORTABLE").is_ok_and(|v| !v.is_empty() && v != "0");

    let mut cfg = cmake::Config::new(&src);
    cfg.profile("Release")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("LLAMA_BUILD_COMMON", "OFF")
        .define("LLAMA_BUILD_TESTS", "OFF")
        .define("LLAMA_BUILD_TOOLS", "OFF")
        .define("LLAMA_BUILD_EXAMPLES", "OFF")
        .define("LLAMA_BUILD_SERVER", "OFF")
        .define("LLAMA_BUILD_APP", "OFF")
        .define("LLAMA_OPENSSL", "OFF")
        .define("GGML_OPENMP", "OFF") // ggml's own thread pool; no libgomp/libomp dependency
        .define("GGML_CUDA", if cuda { "ON" } else { "OFF" })
        .define("GGML_VULKAN", if vulkan { "ON" } else { "OFF" })
        .build_target("llama");
    if portable {
        cfg.define("GGML_NATIVE", "OFF");
        if arch == "x86_64" {
            // x86-64-v3 (Haswell, 2013+)
            for f in ["SSE42", "AVX", "AVX2", "BMI2", "FMA", "F16C"] {
                cfg.define(format!("GGML_{f}"), "ON");
            }
        }
    }
    if let Ok(extra) = std::env::var("PLAMO_CMAKE_ARGS") {
        for a in extra.split_whitespace() {
            if let Some((k, v)) = a.trim_start_matches("-D").split_once('=') {
                cfg.define(k, v);
            }
        }
    }
    let out = cfg.build();
    let build = out.join("build");

    // static libraries are spread over the build tree; add every directory that holds one
    let mut dirs = Vec::new();
    find_lib_dirs(&build, &mut dirs);
    for d in &dirs {
        println!("cargo:rustc-link-search=native={}", d.display());
    }
    let mut libs = vec!["llama", "ggml"];
    if cuda {
        libs.push("ggml-cuda");
    }
    if vulkan {
        libs.push("ggml-vulkan");
    }
    if os == "macos" {
        libs.extend(["ggml-metal", "ggml-blas"]);
    }
    libs.extend(["ggml-cpu", "ggml-base"]);
    for l in libs {
        println!("cargo:rustc-link-lib=static={l}");
    }

    if os == "macos" {
        println!("cargo:rustc-link-lib=dylib=c++");
        for fw in ["Foundation", "Metal", "MetalKit", "Accelerate"] {
            println!("cargo:rustc-link-lib=framework={fw}");
        }
    } else {
        println!("cargo:rustc-link-lib=dylib=stdc++");
    }
    if cuda {
        for d in ["CUDA_PATH", "CUDA_HOME"].iter().filter_map(|v| std::env::var(v).ok()) {
            println!("cargo:rustc-link-search=native={d}/lib64");
        }
        for d in ["/usr/local/cuda/lib64", "/opt/cuda/lib64", "/usr/local/cuda/targets/sbsa-linux/lib"] {
            if Path::new(d).is_dir() {
                println!("cargo:rustc-link-search=native={d}");
            }
        }
        for l in ["cudart", "cublas", "cublasLt", "cuda"] {
            println!("cargo:rustc-link-lib=dylib={l}");
        }
    }
    if vulkan {
        println!("cargo:rustc-link-lib=dylib=vulkan");
    }

    cc::Build::new()
        .file("shim.c")
        .include(src.join("include"))
        .include(src.join("ggml/include"))
        .compile("pt_shim");

    println!("cargo:rerun-if-changed=shim.c");
    println!("cargo:rerun-if-changed=vendor/llama.cpp");
    println!("cargo:rerun-if-env-changed=PLAMO_PORTABLE");
    println!("cargo:rerun-if-env-changed=PLAMO_CMAKE_ARGS");
}

fn find_lib_dirs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut has_lib = false;
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            find_lib_dirs(&p, out);
        } else if p.extension().is_some_and(|x| x == "a" || x == "lib") {
            has_lib = true;
        }
    }
    if has_lib {
        out.push(dir.to_path_buf());
    }
}
