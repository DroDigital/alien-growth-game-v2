//! intel_tex_2's prebuilt kernels include C++ objects; link the C++ runtime
//! (MSVC links it automatically).
fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("apple") {
        println!("cargo:rustc-link-lib=c++");
    } else if target.contains("linux") {
        println!("cargo:rustc-link-lib=stdc++");
    }
}
