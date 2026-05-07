// Generates suil-0 FFI bindings for the LV2 UI hosting layer (Step 11/13).
// The published `suil-sys` crates on crates.io are stale; we vendor our own
// bindings here so we always track the system's `suil-0` headers.

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=PKG_CONFIG_PATH");

    let suil = pkg_config::Config::new()
        .atleast_version("0.10")
        .probe("suil-0");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let bindings_path = out_dir.join("suil_sys.rs");

    match suil {
        Ok(lib) => {
            // Belt-and-suspenders: emit the link directives explicitly. The
            // pkg_config crate is supposed to emit these from `probe()`, but
            // in some configs they don't make it to the linker — and missing
            // -lsuil-0 means undefined symbols at link time.
            for lib_name in &lib.libs {
                println!("cargo:rustc-link-lib={lib_name}");
            }
            for path in &lib.link_paths {
                println!("cargo:rustc-link-search=native={}", path.display());
            }
            let mut builder = bindgen::Builder::default()
                .header_contents("suil_wrap.h", "#include <suil/suil.h>")
                .allowlist_function("suil_.*")
                .allowlist_type("Suil.*")
                .allowlist_type("LV2_Feature")
                .allowlist_var("SUIL_.*")
                .layout_tests(false);
            for path in &lib.include_paths {
                builder = builder.clang_arg(format!("-I{}", path.display()));
            }
            match builder.generate() {
                Ok(bindings) => {
                    bindings
                        .write_to_file(&bindings_path)
                        .expect("write suil_sys bindings");
                    println!("cargo:rustc-cfg=have_suil");
                }
                Err(e) => {
                    println!("cargo:warning=suil bindgen failed: {e}; UI hosting disabled");
                    std::fs::write(&bindings_path, "// suil bindings unavailable\n").ok();
                }
            }
        }
        Err(e) => {
            println!("cargo:warning=suil-0 not found via pkg-config: {e}; UI hosting disabled");
            std::fs::write(&bindings_path, "// suil bindings unavailable\n").ok();
        }
    }
}
