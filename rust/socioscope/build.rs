use std::path::Path;
use std::process::Command;

fn main() {
    // The sherpa-onnx shared libraries are copied next to the binary.
    let origin = if cfg!(target_os = "macos") {
        "@loader_path"
    } else {
        "$ORIGIN"
    };
    println!("cargo:rustc-link-arg=-Wl,-rpath,{origin}");

    let Ok(output) = Command::new("g++")
        .arg("-print-file-name=libstdc++.so")
        .output()
    else {
        return;
    };
    if !output.status.success() {
        return;
    }
    let path = String::from_utf8_lossy(&output.stdout);
    let path = path.trim();
    if let Some(directory) = Path::new(path).parent() {
        if directory.is_dir() {
            println!("cargo:rustc-link-search={}", directory.display());
        }
    }
}
