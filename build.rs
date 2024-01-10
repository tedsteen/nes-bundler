fn main() {
    println!("cargo:rerun-if-changed=config/config.yaml");
    println!("cargo:rerun-if-changed=config/rom.nes");

    println!("cargo:rerun-if-changed=src/audio/stretch/signalsmith-stretch/signalsmith-stretch.h");
    println!("cargo:rerun-if-changed=src/audio/stretch/signalsmith-stretch-wrapper.hpp");
    println!("cargo:rerun-if-changed=src/audio/stretch/signalsmith-stretch-wrapper.cpp");
    println!("cargo:rerun-if-changed=src/audio/stretch/mod.rs");
    let mut code = cxx_build::bridge("src/audio/stretch/mod.rs");
    let code = code
        .file("src/audio/stretch/signalsmith-stretch-wrapper.cpp")
        .flag_if_supported("-std=c++11");

    #[cfg(not(target_os = "windows"))]
    code.flag("-O3");
    #[cfg(target_os = "windows")]
    code.flag("/O2");

    code.compile("signalsmith-stretch");
}
