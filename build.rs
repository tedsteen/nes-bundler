fn main() {
    println!("cargo:rerun-if-changed=config/build_config.yaml");
    println!("cargo:rerun-if-changed=config/rom.nes");
}
