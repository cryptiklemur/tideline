//! Generated FFI bindings for suil-0. The actual file is produced by build.rs
//! into `$OUT_DIR/suil_sys.rs` so we always track the system suil-0 headers.
//!
//! When suil-0 is not installed (or bindgen failed) the build emits an empty
//! file, in which case `pub use self::*` simply re-exports nothing. Callers
//! must guard usage behind the `have_suil` cfg.

#![allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    dead_code
)]

include!(concat!(env!("OUT_DIR"), "/suil_sys.rs"));

// Force the linker to pull in libsuil-0 even when the build-script-emitted
// `cargo:rustc-link-lib=suil-0` directive doesn't make it through (which
// happens in some workspace + linker configurations). The empty extern block
// just carries the link attribute.
#[link(name = "suil-0")]
unsafe extern "C" {}
