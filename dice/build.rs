use std::env;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

fn main() {
    let out = &PathBuf::from(env::var_os("OUT_DIR").unwrap());
    if cfg!(feature = "stm32f429") {
        File::create(out.join("memory.x"))
            .unwrap()
            .write_all(include_bytes!("../memory_f4.x"))
            .unwrap();
    } else if cfg!(feature = "stm32h743"){
        File::create(out.join("memory.x"))
            .unwrap()
            .write_all(include_bytes!("../memory_h7.x"))
            .unwrap();
    }
    println!("cargo:rustc-link-search={}", out.display());
}
