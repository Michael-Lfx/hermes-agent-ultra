use std::env;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo::rustc-check-cfg=cfg(kokoro_rknn_ffi)");
    if env::var("CARGO_FEATURE_ROCKCHIP").is_err() {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| manifest_dir.clone());

    let prebuilt = env::var("KOKORO_PREBUILT_LIB")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            let cache = root.join(".cross-cache/kokoro-hybrid/libkokoro_ffi.a");
            cache.exists().then_some(cache)
        });

    if let Some(lib) = prebuilt {
        link_prebuilt(&lib);
        println!("cargo:rustc-cfg=kokoro_rknn_ffi");
        return;
    }

    if env::var("KOKORO_BUILD").ok().as_deref() != Some("1") {
        println!(
            "cargo:warning=Kokoro hybrid RKNN FFI not linked (set KOKORO_BUILD=1 + deps, or KOKORO_PREBUILT_LIB); TTS falls back to sherpa CPU kokoro"
        );
        return;
    }

    let sysroot = env::var("KOKORO_SYSROOT").unwrap_or_default();
    let target = env::var("TARGET").unwrap_or_default();

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++20")
        .include(manifest_dir.join("kokoro"))
        .file(manifest_dir.join("kokoro/kokoro_ffi.cpp"));

    if !sysroot.is_empty() {
        let sys = PathBuf::from(&sysroot);
        build.flag(format!("--sysroot={sysroot}"));
        for inc in ["usr/include", "usr/local/include", "include"] {
            let p = sys.join(inc);
            if p.exists() {
                build.include(&p);
                build.include(p.join("onnxruntime"));
                build.include(p.join("rknpu"));
            }
        }
    } else {
        for inc in ["/usr/include", "/usr/local/include"] {
            build.include(inc);
            build.include(format!("{inc}/onnxruntime"));
        }
    }

    if target.contains("aarch64")
        && let Ok(gcc) = env::var("CARGO_TARGET_AARCH64_UNKNOWN_LINUX_GNU_LINKER")
    {
        build.compiler(gcc.replace("-gcc", "-g++"));
    }

    build.compile("kokoro_ffi");

    println!("cargo:rustc-cfg=kokoro_rknn_ffi");
    println!("cargo:rustc-link-lib=dylib=onnxruntime");
    println!("cargo:rustc-link-lib=dylib=rknnrt");
    println!("cargo:rustc-link-lib=dylib=stdc++");
    println!("cargo:rustc-link-lib=dylib=m");
    println!("cargo:rustc-link-lib=dylib=pthread");
    println!("cargo:rustc-link-lib=dylib=dl");

    if !sysroot.is_empty() {
        println!("cargo:rustc-link-search=native={sysroot}/usr/lib");
        println!("cargo:rustc-link-search=native={sysroot}/usr/lib/aarch64-linux-gnu");
        println!("cargo:rustc-link-search=native={sysroot}/lib");
    }
}

fn link_prebuilt(lib: &Path) {
    if let Some(dir) = lib.parent() {
        println!("cargo:rustc-link-search=native={}", dir.display());
    }
    if let Ok(dir) = env::var("RK_NPU_LIB_DIR") {
        println!("cargo:rustc-link-search=native={dir}");
    }
    if let Ok(dir) = env::var("SHERPA_ONNX_LIB_DIR") {
        println!("cargo:rustc-link-search=native={dir}");
    }
    println!("cargo:rustc-link-lib=static=kokoro_ffi");
    println!("cargo:rustc-link-lib=dylib=onnxruntime");
    println!("cargo:rustc-link-lib=dylib=rknnrt");
    println!("cargo:rustc-link-lib=dylib=stdc++");
    println!("cargo:rustc-link-lib=dylib=m");
    println!("cargo:rustc-link-lib=dylib=pthread");
    println!("cargo:rustc-link-lib=dylib=dl");
}
