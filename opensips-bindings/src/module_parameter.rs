use core::cell::UnsafeCell;
use core::ffi::CStr;
use core::num::NonZeroI32;
use core::ptr;
use std::os::raw::{c_char, c_int, c_void};

use crate::generated as opensips;

#[repr(C)]
pub struct Integer(UnsafeCell<c_int>);

// This *requires* that the plugin is only used in a single-threaded
// fashion.
unsafe impl Sync for Integer {}

impl Integer {
    pub const fn new() -> Self {
        Self(UnsafeCell::new(0))
    }

    fn get(&self) -> c_int {
        unsafe { *self.0.get() }
    }

    pub fn get_value(&self) -> Option<NonZeroI32> {
        NonZeroI32::new(self.get())
    }

    const fn as_mut(&self) -> *mut c_int {
        self.0.get()
    }

    #[doc(hidden)]
    pub const fn as_param_pointer(&self) -> *mut c_void {
        self.as_mut().cast()
    }
}

#[repr(C)]
pub struct String(UnsafeCell<*mut c_char>);

// This *requires* that the plugin is only used in a single-threaded
// fashion.
unsafe impl Sync for String {}

impl String {
    pub const fn new() -> Self {
        Self(UnsafeCell::new(ptr::null_mut()))
    }

    fn get(&self) -> *mut c_char {
        unsafe { *self.0.get() }
    }

    /// Gets the value as a valid UTF-8 Rust string.
    ///
    /// # Safety
    ///
    /// You must ensure that the pointer, if non-NULL, points to a
    /// valid C string.
    pub unsafe fn get_value(&self) -> Option<&str> {
        let value = self.get();

        if value.is_null() {
            return None;
        }

        CStr::from_ptr(value).to_str().ok()
    }

    const fn as_mut(&self) -> *mut *mut c_char {
        self.0.get()
    }

    #[doc(hidden)]
    pub const fn as_param_pointer(&self) -> *mut c_void {
        self.as_mut().cast()
    }
}

pub trait ModuleParameter {
    const OPENSIPS_TYPE: u32;

    // We would prefer to have these as trait methods, but we cannot
    // have `const fn` in traits yet.
    //
    // const fn new() -> Self;
    // const fn as_param_pointer(&self) -> *mut c_void;
}

impl ModuleParameter for Integer {
    const OPENSIPS_TYPE: u32 = opensips::INT_PARAM;
}

impl ModuleParameter for String {
    const OPENSIPS_TYPE: u32 = opensips::STR_PARAM;
}

/// Generates a `static PARAMS` with the specified names and types.
///
/// ```rust,norun
/// opensips::module_parameters! {
///     #[name = "any-name-you-want"]
///     static NUMBERS: module_parameter::Integer;
///
///     #[name = "ReallyAnyName"]
///     static LETTERS: module_parameter::String;
/// }
/// ```
#[macro_export]
macro_rules! module_parameters {
    ($(
        #[name = $name:literal]
        static $var_name:ident: $ty:ty;
    )*) => {
        $(
            static $var_name: $ty = <$ty>::new();
        )*

        static PARAMS: &[opensips::param_export_t] = &[
            $(
                opensips::param_export_t {
                    name: cstr_lit!($name),
                    type_: <$ty as $crate::module_parameter::ModuleParameter>::OPENSIPS_TYPE,
                    param_pointer: $var_name.as_param_pointer(),
                },
            )*
            opensips::param_export_t::NULL,
        ];
    };
}
