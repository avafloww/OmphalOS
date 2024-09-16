use std::env::var;

fn main() {
    // set cfg(esp32s3), cfg(esp32c6), etc. based on features
    println!("cargo::rustc-check-cfg=cfg(esp32s3)");
    println!("cargo::rustc-check-cfg=cfg(esp32c6)");
    if var("CARGO_FEATURE_CPU_ESP32S3").is_ok() {
        println!("cargo::rustc-cfg=esp32s3");
    } else if var("CARGO_FEATURE_CPU_ESP32C6").is_ok() {
        println!("cargo::rustc-cfg=esp32c6");
    }

    println!("cargo::rerun-if-changed=build.rs");
}
