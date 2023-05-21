use core::{mem, ptr};
use std::os::raw::{c_char, c_int};

// This is the bindgen-created output...
mod generated {
    #![allow(dead_code)]
    #![allow(non_camel_case_types)]
    #![allow(non_upper_case_globals)]
    #![allow(improper_ctypes)]
    #![allow(non_snake_case)]
    #![allow(clippy::missing_safety_doc)]
    #![allow(clippy::useless_transmute)]

    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub use generated::*;

pub mod command;
pub mod module_parameter;

// ... and what follows are additions we've made

// C Strings are NUL-terminated
#[macro_export]
macro_rules! cstr_lit {
    ($s:literal) => {
        concat!($s, "\0").as_ptr()
    };
    // It seems like a mistake that some strings are marked as
    // mutable; this should be throughly checked.
    (mut $s:literal) => {
        concat!($s, "\0").as_ptr() as *mut c_char
    };
}

// Since we are placing this data as a `static`, Rust needs to
// enforce that the data is OK to be used across multiple threads
// concurrently.
//
// I'm making the following assumptions that should be otherwise
// validated:
//
// 1. The data is never written to.
// 2. The modules are run in a single-threaded fashion.
unsafe impl Sync for module_exports {}
unsafe impl Sync for dep_export_t {}
unsafe impl Sync for cmd_export_t {}

// It appears opensips uses sentinel values to terminate arrays,
// so we might as well make those easy to create.

impl cmd_export_t {
    pub const NULL: Self = Self {
        name: ptr::null(),
        function: None,
        params: [cmd_param::NULL; 9],
        flags: 0,
    };
}

impl module_dependency {
    pub const NULL: Self = Self {
        mod_type: module_type::MOD_TYPE_NULL,
        mod_name: ptr::null_mut(),
        type_: 0,
    };
}

impl modparam_dependency_t {
    pub const NULL: Self = Self {
        script_param: ptr::null_mut(),
        get_deps_f: None,
    };
}

impl cmd_param {
    pub const NULL: Self = Self {
        flags: 0,
        fixup: None,
        free_fixup: None,
    };
}

unsafe impl Sync for param_export_t {}

impl param_export_t {
    pub const NULL: Self = Self {
        name: ptr::null(),
        type_: 0,
        param_pointer: ptr::null_mut(),
    };
}

unsafe impl Sync for mi_export_t {}

impl mi_export_t {
    pub const NULL: Self = Self {
        name: ptr::null_mut(),
        help: ptr::null_mut(),
        flags: 0,
        init_f: None,
        recipes: [mi_recipe_t::NULL; 48],
    };
}

impl mi_recipe_t {
    pub const NULL: Self = Self {
        cmd: None,
        params: [ptr::null_mut(); 10],
    };
}

// The `dep_export_t` structure uses a Flexible Array Member
// (FAM). These are quite annoying to deal with. Here, I create a
// parallel structure that uses a const generic array. This
// *should* have the same memory layout as `dep_export_t`, but we
// can only use it with a compile-time known length (which is all
// we need for now).

#[repr(C)]
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

// This is a `static inline` function which bindgen doesn't
// generate. Define it ourselves.
#[inline]
pub fn load_sig_api() -> Option<sig_binds> {
    // # Safety
    //
    // `find_export` is called with a static string and the same
    // parameter for flags as every other call I can see in the
    // opensips codebase.
    //
    // `transmute` is equivalent to the function pointer cast in the
    // original C code, and relies on the fact that any
    // `Option<function pointer>` has the same memory layout and
    // restrictions as any other.
    let load_sig: load_sig_f = unsafe {
        // import the SL auto-loading function
        let load_sig_raw = find_export(cstr_lit!("load_sig"), 0);
        mem::transmute(load_sig_raw)
    };

    let Some(load_sig) = load_sig else {
        // TODO: LM_ERR("can't import load_sig\n");
        return None;
    };

    let mut sigb = sig_binds {
        reply: None,
        gen_totag: None,
    };

    // # Safety
    //
    // We have properly initialized `sigb`.
    unsafe {
        // let the auto-loading function load all TM stuff
        if load_sig(&mut sigb) == -1 {
            return None;
        };
    }

    Some(sigb)
}

#[inline]
pub fn init_mi_result_ok() -> *mut mi_response_t {
    unsafe { init_mi_result_string("OK".as_ptr(), 2) }
}

#[inline]
pub fn is_worker_proc(rank: c_int) -> bool {
    rank >= 1
}

impl str_ {
    pub fn try_as_str(&self) -> Result<&str, core::str::Utf8Error> {
        let len = self.len.try_into().expect("TODO: report error");
        // TODO: safety
        let s = unsafe { core::slice::from_raw_parts(self.s, len) };
        core::str::from_utf8(s)
    }

    pub fn as_str(&self) -> &str {
        self.try_as_str().unwrap()
    }
}

pub trait StrExt {
    fn as_opensips_str(&self) -> str_;
}

impl StrExt for str {
    fn as_opensips_str(&self) -> str_ {
        str_ {
            // It seems like a mistake that these strings are
            // marked as mutable as they are used with constant
            // data; likely the opensips types should be fixed.
            s: self.as_ptr() as *mut c_char,
            len: self.len().try_into().unwrap_or(0),
        }
    }
}

impl sip_msg {
    pub fn header_iter(&self) -> impl Iterator<Item = &hdr_field> {
        core::iter::from_fn({
            let mut head_raw = self.headers;

            move || {
                // # Safety
                //
                // We are checking for NULL, but otherwise we trust
                // that OpenSIPS has correctly initialized this data
                // and is not going to attempt to modify it.
                let head = unsafe { head_raw.as_ref() };
                if let Some(head) = head {
                    head_raw = head.next;
                }
                head
            }
        })
    }
}
