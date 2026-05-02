use std::env;
use std::path::PathBuf;

fn main() {
    // Probe carla-standalone via pkg-config: emits cargo:rustc-link-lib= directives
    // for libcarla_standalone2 and resolves the include paths we need to feed clang.
    let lib = pkg_config::Config::new()
        .probe("carla-standalone")
        .expect("pkg-config failed to find carla-standalone (install carla-git ≥ 2.6.0)");

    // pkg_config emits link search dirs and `-l` args, but carla on Arch ships
    // libcarla_standalone2.so under /usr/lib/carla (not on the default rpath).
    // Export the rpath as cargo metadata so downstream crates (which actually
    // produce executable artifacts — bins, tests, examples) can re-emit it via
    // their own build.rs as a -Wl,-rpath linker arg.
    let rpaths: Vec<String> = lib
        .link_paths
        .iter()
        .map(|p| p.display().to_string())
        .collect();
    println!("cargo:rpaths={}", rpaths.join(":"));

    let mut builder = bindgen::Builder::default()
        .header("/usr/include/carla/CarlaHost.h")
        .header("/usr/include/carla/CarlaBackend.h")
        .clang_arg("-x")
        .clang_arg("c++")
        .clang_arg("-std=c++17")
        // CarlaHost.h pulls in CarlaBackend.h which lives in a C++ namespace.
        // Bindgen handles namespaces fine; we just need the include paths.
        .layout_tests(false)
        .generate_comments(false)
        // Allowlist only the surface we care about. The headers also pull in
        // <algorithm>, <cmath>, <limits> when compiled as C++ — without an
        // allowlist bindgen would try to bind half the STL.
        .allowlist_function("carla_.*")
        .allowlist_type("Carla.*")
        .allowlist_type(".*BinaryType.*")
        .allowlist_type(".*PluginType.*")
        .allowlist_var("ENGINE_.*")
        .allowlist_var("PLUGIN_.*")
        .allowlist_var("BINARY_.*");

    for path in &lib.include_paths {
        builder = builder.clang_arg(format!("-I{}", path.display()));
    }
    // Carla's headers also reference includes/CarlaDefines.h via a sibling dir.
    builder = builder.clang_arg("-I/usr/include/carla/includes");

    let bindings = builder
        .generate()
        .expect("bindgen failed to generate carla bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("failed to write bindings.rs");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=/usr/include/carla/CarlaHost.h");
    println!("cargo:rerun-if-changed=/usr/include/carla/CarlaBackend.h");
}
