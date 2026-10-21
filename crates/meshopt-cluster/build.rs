// Builds meshoptimizer v1.3 (MIT, vendor/meshoptimizer) and the C wrapper in cpp/ with cc.
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let vendor = manifest.join("../../vendor/meshoptimizer");
    let src = vendor.join("src");
    let cpp = manifest.join("cpp");

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .include(&src)
        .include(vendor.join("demo"))
        .include(&cpp);

    // meshoptimizer's asserts follow NDEBUG; keep them on in debug builds to catch misuse early
    if env::var("PROFILE").as_deref() == Ok("release") {
        build.define("NDEBUG", None);
    }

    let mut sources: Vec<PathBuf> = fs::read_dir(&src)
        .expect("read vendor/meshoptimizer/src")
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "cpp"))
        .collect();
    sources.sort();
    sources.push(cpp.join("mlc_wrapper.cpp"));

    for path in &sources {
        println!("cargo:rerun-if-changed={}", path.display());
        build.file(path);
    }
    for header in [
        src.join("meshoptimizer.h"),
        vendor.join("demo/clusterlod.h"),
        cpp.join("mlc_wrapper.h"),
    ] {
        println!("cargo:rerun-if-changed={}", header.display());
    }

    build.compile("meshopt_cluster");
}
