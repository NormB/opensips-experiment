use bindgen::{
    builder,
    callbacks::{IntKind, ParseCallbacks},
    EnumVariation,
};
use std::{env, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir: PathBuf = env::var_os("OUT_DIR").ok_or("OUT_DIR not set")?.into();

    let mut builder = builder();

    if let Ok(src_dir) = env::var("OPENSIPS_SRC_DIR") {
        builder = builder.clang_arg("-I").clang_arg(src_dir);
    }

    let bindings = builder
        .header("opensips_bindings.h")
        // This has a duplicate definition
        .blocklist_item("IPPORT_RESERVED")
        // Modules look a bit nicer for enums
        .default_enum_style(EnumVariation::ModuleConsts)
        // Adjust types to avoid casts
        .parse_callbacks(Box::new(AdjustMacroTypes))
        // Trust bindgen to generate the right thing
        .layout_tests(false)
        .generate()?;

    let out_path = out_dir.join("bindings.rs");

    bindings.write_to_file(out_path)?;

    Ok(())
}

#[derive(Debug)]
struct AdjustMacroTypes;

impl ParseCallbacks for AdjustMacroTypes {
    fn int_macro(&self, name: &str, _value: i64) -> Option<IntKind> {
        let cmd_flag_macro_names = [
            "REQUEST_ROUTE",
            "FAILURE_ROUTE",
            "ONREPLY_ROUTE",
            "BRANCH_ROUTE",
            "ERROR_ROUTE",
            "LOCAL_ROUTE",
            "STARTUP_ROUTE",
            "TIMER_ROUTE",
            "EVENT_ROUTE",
        ];

        if cmd_flag_macro_names.iter().any(|&n| n == name) {
            Some(IntKind::Int)
        } else {
            None
        }
    }
}
