use bindgen::{
    builder,
    callbacks::{IntKind, ParseCallbacks},
    EnumVariation,
};
use std::{env, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_dir: PathBuf = env::var_os("OUT_DIR").ok_or("OUT_DIR not set")?.into();
    let cargo_cfg_target_arch = env::var("CARGO_CFG_TARGET_ARCH")?;

    let mut builder = builder();

    if let Ok(src_dir) = env::var("OPENSIPS_SRC_DIR") {
        builder = builder.clang_arg("-I").clang_arg(src_dir);
    }

    let bindings = builder
        .clang_arg(format!(
            "-D CARGO_CFG_TARGET_ARCH__{cargo_cfg_target_arch}=1"
        ))
        .header("bindings.h")
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

        let cmd_param_macro_names = [
            "CMD_PARAM_INT",
            "CMD_PARAM_STR",
            "CMD_PARAM_VAR",
            "CMD_PARAM_REGEX",
            "CMD_PARAM_OPT",
            "CMD_PARAM_FIX_NULL",
            "CMD_PARAM_NO_EXPAND",
            "CMD_PARAM_STATIC",
        ];

        let lump_rpl_macro_names = [
            "LUMP_RPL_HDR",
            "LUMP_RPL_BODY",
            "LUMP_RPL_NODUP",
            "LUMP_RPL_NOFREE",
            "LUMP_RPL_SHMEM",
        ];

        if cmd_flag_macro_names.contains(&name)
            || cmd_param_macro_names.contains(&name)
            || lump_rpl_macro_names.contains(&name)
        {
            Some(IntKind::Int)
        } else {
            None
        }
    }
}
