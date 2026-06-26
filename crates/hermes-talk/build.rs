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
            let cache = root.join(".cross-cache/kokoro-server/libkokoro_ffi.a");
            cache.exists().then_some(cache)
        });

    if let Some(lib) = prebuilt {
        link_prebuilt(&lib);
        println!("cargo:rustc-cfg=kokoro_rknn_ffi");
        return;
    }

    if env::var("KOKORO_BUILD").ok().as_deref() != Some("1") {
        println!(
            "cargo:warning=Kokoro RKNN FFI not linked (set KOKORO_BUILD=1 + deps, or KOKORO_PREBUILT_LIB); TTS falls back to sherpa CPU kokoro"
        );
        return;
    }

    let kokoro_server =
        env::var("KOKORO_SERVER_DIR").unwrap_or_else(|_| "/home/leeyang/kokoro-server".to_string());
    let kokoro_src = PathBuf::from(&kokoro_server);
    if !kokoro_src.join("src/kokoro.cpp").exists() {
        println!(
            "cargo:warning=KOKORO_SERVER_DIR={kokoro_server} missing sources; skipping Kokoro FFI build"
        );
        return;
    }

    let misaki = kokoro_src.join("misaki-cpp");
    let sysroot = env::var("KOKORO_SYSROOT").unwrap_or_default();
    let target = env::var("TARGET").unwrap_or_default();

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++20")
        .define("USE_RKNN", None)
        .include(kokoro_src.join("src"))
        .include(misaki.join("include"))
        .include(manifest_dir.join("kokoro"))
        .file(manifest_dir.join("kokoro/kokoro_ffi.cpp"))
        .file(kokoro_src.join("src/kokoro.cpp"))
        .file(kokoro_src.join("src/onnx-decoder.cpp"))
        .file(kokoro_src.join("src/rknn-decoder.cpp"))
        .file(kokoro_src.join("src/istft.cpp"))
        .file(kokoro_src.join("src/g2p.cpp"))
        .file(kokoro_src.join("src/phonemizer.cpp"))
        .file(misaki.join("src/g2p.cpp"))
        .file(misaki.join("src/fallback.cpp"))
        .file(misaki.join("src/lexicon.cpp"))
        .file(misaki.join("src/tagger.cpp"))
        .file(misaki.join("src/num2words_en.cpp"));

    if !sysroot.is_empty() {
        let sys = PathBuf::from(&sysroot);
        build.flag(format!("--sysroot={sysroot}"));
        for inc in ["usr/include", "usr/local/include", "include"] {
            let p = sys.join(inc);
            if p.exists() {
                build.include(&p);
                build.include(p.join("onnxruntime"));
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
    println!("cargo:rustc-link-lib=dylib=espeak-ng");
    println!("cargo:rustc-link-lib=dylib=openblas");
    println!("cargo:rustc-link-lib=dylib=fmt");
    println!("cargo:rustc-link-lib=dylib=spdlog");
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
    println!("cargo:rustc-link-lib=static=kokoro_ffi");
    println!("cargo:rustc-link-lib=dylib=onnxruntime");
    println!("cargo:rustc-link-lib=dylib=rknnrt");
    println!("cargo:rustc-link-lib=dylib=espeak-ng");
    println!("cargo:rustc-link-lib=dylib=openblas");
    println!("cargo:rustc-link-lib=dylib=fmt");
    println!("cargo:rustc-link-lib=dylib=spdlog");
    println!("cargo:rustc-link-lib=dylib=stdc++");
    println!("cargo:rustc-link-lib=dylib=m");
    println!("cargo:rustc-link-lib=dylib=pthread");
    println!("cargo:rustc-link-lib=dylib=dl");
}
