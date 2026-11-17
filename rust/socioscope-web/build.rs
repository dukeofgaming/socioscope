fn main() {
    // socioscope links sherpa-onnx as a shared library copied next to the binary.
    // That rpath is set on the CLI crate and is not inherited by this package.
    let origin = if cfg!(target_os = "macos") {
        "@loader_path"
    } else {
        "$ORIGIN"
    };
    println!("cargo:rustc-link-arg=-Wl,-rpath,{origin}");
}
