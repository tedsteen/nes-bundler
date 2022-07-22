fn main() {
    println!("cargo:rerun-if-changed=assets/build_config.yaml");
    println!("cargo:rerun-if-changed=assets/rom.nes");
}
