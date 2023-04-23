use std::{
    os::raw::{c_char, c_int, c_void},
    ptr,
};

mod bindings {
    #![allow(dead_code)]
    #![allow(non_camel_case_types)]
    #![allow(non_upper_case_globals)]
    #![allow(improper_ctypes)]
    #![allow(non_snake_case)]

    use core::ptr;

    // This is the bindgen-created output...

    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

    // ... and what follows are additions we've made

    // Since we are placing this data as a `static`, Rust needs to
    // enforce that the data is OK to be used across multiple threads
    // concurrently.
    //
    // I'm making the following assumptions that should be otherwise
    // validated:
    //
    // 1. The data is never written to.
    // 1. The modules are run in a single-threaded fashion.
    unsafe impl Sync for module_exports {}
    unsafe impl Sync for dep_export_t {}
    unsafe impl Sync for cmd_export_t {}

    // It appears opensips uses sentinel values fo terminate arrays,
    // so we might as well make those easy to create.

    pub const NULL_MODULE_DEPENDENCY: module_dependency = module_dependency {
        mod_type: module_type_MOD_TYPE_NULL,
        mod_name: ptr::null_mut(),
        type_: 0,
    };

    pub const NULL_MODPARAM_DEPENDENCY: modparam_dependency_t = modparam_dependency_t {
        script_param: ptr::null_mut(),
        get_deps_f: None,
    };

    pub const NULL_CMD_PARAM: cmd_param = cmd_param {
        flags: 0,
        fixup: None,
        free_fixup: None,
    };

    unsafe impl Sync for param_export_t {}

    pub const NULL_PARAM_EXPORT: param_export_t = param_export_t {
        name: ptr::null(),
        type_: 0,
        param_pointer: ptr::null_mut(),
    };

    // The `dep_export_t` structure uses a Flexible Array Member
    // (FAM). These are quite annoying to deal with. Here, I create a
    // parallel structure that uses a const generic array. This
    // *should* have the same memory layout as `dep_export_t`, but we
    // can only use it with a compile-time known length (which is all
    // we need for now).

    pub struct dep_export_concrete<const N: usize> {
        pub md: [module_dependency_t; 10usize],
        pub mpd: [modparam_dependency_t; N],
    }

    unsafe impl<const N: usize> Sync for dep_export_concrete<N> {}

    impl<const N: usize> dep_export_concrete<N> {
        pub const fn as_ptr(&self) -> *const dep_export_t {
            (self as *const Self).cast()
        }
    }
}

// C Strings are NUL-terminated
macro_rules! cstr_lit {
    ($s:literal) => {
        concat!($s, "\0").as_ptr()
    };
    // It seems like a mistake that some strings are marked as
    // mutable; this should be throughly checked.
    (mut $s:literal) => {
        concat!($s, "\0").as_ptr() as *mut u8
    };
}

#[no_mangle]
pub static exports: bindings::module_exports = bindings::module_exports {
    name: cstr_lit!("option"),
    type_: bindings::module_type_MOD_TYPE_DEFAULT,
    version: cstr_lit!("1.0"),
    compile_flags: cstr_lit!("TODO: options module doesn't use this?"),
    dlflags: bindings::DEFAULT_DLFLAGS,
    load_f: None,
    deps: DEPS.as_ptr(),
    cmds: CMDS.as_ptr(),
    acmds: ptr::null(),
    params: PARAMS.as_ptr(),
    stats: ptr::null(),
    mi_cmds: ptr::null(),
    items: ptr::null(),
    trans: ptr::null(),
    procs: ptr::null(),
    preinit_f: None,
    init_f: Some(mod_init),
    response_f: None,
    destroy_f: None,
    init_child_f: None,
    reload_ack_f: None,
};

static DEPS: bindings::dep_export_concrete<1> = bindings::dep_export_concrete {
    md: [
        bindings::module_dependency {
            mod_type: bindings::module_type_MOD_TYPE_DEFAULT,
            mod_name: cstr_lit!(mut "signaling"),
            type_: bindings::DEP_ABORT,
        },
        bindings::NULL_MODULE_DEPENDENCY,
        bindings::NULL_MODULE_DEPENDENCY,
        bindings::NULL_MODULE_DEPENDENCY,
        bindings::NULL_MODULE_DEPENDENCY,
        bindings::NULL_MODULE_DEPENDENCY,
        bindings::NULL_MODULE_DEPENDENCY,
        bindings::NULL_MODULE_DEPENDENCY,
        bindings::NULL_MODULE_DEPENDENCY,
        bindings::NULL_MODULE_DEPENDENCY,
    ],
    mpd: [bindings::NULL_MODPARAM_DEPENDENCY],
};

static CMDS: &[bindings::cmd_export_t] = &[
    bindings::cmd_export_t {
        name: cstr_lit!("options_reply"),
        function: Some(opt_reply),
        params: [bindings::NULL_CMD_PARAM; 9],
        // Odd that the types of the constant are mixed; perhaps can
        // be fixed in bindgen?
        flags: bindings::REQUEST_ROUTE as _,
    },
    bindings::cmd_export_t {
        name: ptr::null(),
        function: None,
        params: [bindings::NULL_CMD_PARAM; 9],
        flags: 0,
    },
];

static PARAMS: &[bindings::param_export_t] = &[
    bindings::param_export_t {
        name: cstr_lit!("accept"),
        type_: bindings::STR_PARAM,
        param_pointer: unsafe { ACCEPT_PARAM.cast() },
    },
    bindings::param_export_t {
        name: cstr_lit!("accept_encoding"),
        type_: bindings::STR_PARAM,
        param_pointer: unsafe { ACCEPT_ENCODING_PARAM.cast() },
    },
    bindings::param_export_t {
        name: cstr_lit!("accept_language"),
        type_: bindings::STR_PARAM,
        param_pointer: unsafe { ACCEPT_LANGUAGE_PARAM.cast() },
    },
    bindings::param_export_t {
        name: cstr_lit!("support"),
        type_: bindings::STR_PARAM,
        param_pointer: unsafe { SUPPORT_PARAM.cast() },
    },
    bindings::NULL_PARAM_EXPORT,
];

// The use of `static mut` frequently indicates super-sketchy
// code. This *definitely* requires that the plugin is only used in a
// single-threaded fashion.
static mut ACCEPT_PARAM: *mut c_char = ptr::null_mut();
static mut ACCEPT_ENCODING_PARAM: *mut c_char = ptr::null_mut();
static mut ACCEPT_LANGUAGE_PARAM: *mut c_char = ptr::null_mut();
static mut SUPPORT_PARAM: *mut c_char = ptr::null_mut();

unsafe extern "C" fn mod_init() -> c_int {
    // TODO: Implement body here
    0
}

unsafe extern "C" fn opt_reply(
    _arg1: *mut bindings::sip_msg,
    _ctx: *mut c_void,
    _arg2: *mut c_void,
    _arg3: *mut c_void,
    _arg4: *mut c_void,
    _arg5: *mut c_void,
    _arg6: *mut c_void,
    _arg7: *mut c_void,
    _arg8: *mut c_void,
) -> i32 {
    // TODO: Implement body here
    0
}
