//! Generates Kotlin bindings for utre_bridge: `cargo run -p uniffi-bindgen -- generate ...`.
//! Pinned to the same UniFFI version as the library so the bindings always match.
fn main() {
    uniffi::uniffi_bindgen_main()
}
