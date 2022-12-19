fn main() {
    println!("cargo:rerun-if-changed=config/config.yaml");
    println!("cargo:rerun-if-changed=config/rom.nes");
}
