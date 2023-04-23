use bindgen::builder;
use std::{env, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir: PathBuf = env::var_os("OUT_DIR").ok_or("OUT_DIR not set")?.into();

    let bindings = builder()
        .header("../opensips/sr_module.h")
        .blocklist_item("IPPORT_RESERVED")
        .generate()?;

    let out_path = out_dir.join("bindings.rs");

    bindings.write_to_file(out_path)?;

    Ok(())
}
