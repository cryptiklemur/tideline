// Re-emit the rpath(s) advertised by carla-sys as -Wl,-rpath linker args on
// every artifact this crate produces (binary + test executables). carla-sys
// itself is a rlib so it can't emit rustc-link-arg directly — it surfaces the
// paths via DEP_CARLA_SYS_RPATHS metadata for us to apply here.
//
// Carla on Arch installs libcarla_standalone2.so under /usr/lib/carla, which
// is not on the default loader path; without this rpath the test/bin
// executables fail at runtime with "cannot open shared object file".

fn main() {
    if let Ok(rpaths) = std::env::var("DEP_CARLA_STANDALONE2_RPATHS") {
        for rpath in rpaths.split(':').filter(|s| !s.is_empty()) {
            let arg = format!("-Wl,-rpath,{rpath}");
            println!("cargo:rustc-link-arg-bins={arg}");
            println!("cargo:rustc-link-arg-tests={arg}");
            // `rustc-link-arg-tests` only covers integration tests (tests/*.rs).
            // Lib unit tests live in the `--lib --test` artifact, which needs
            // the rpath too — emit it via `rustc-link-arg` so the lib unit
            // test binary picks it up. (rlib builds skip the linker so the
            // flag is harmless there.)
            println!("cargo:rustc-link-arg={arg}");
        }
    }
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=DEP_CARLA_STANDALONE2_RPATHS");
}
