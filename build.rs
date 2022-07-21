fn main() {
    println!("cargo:rerun-if-changed=assets/build_config.json");
    println!("cargo:rerun-if-changed=assets/rom.nes");
}