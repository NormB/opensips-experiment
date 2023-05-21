use std::os::raw::c_void;

use crate::generated as opensips;

trait CommandFunctionParam {
    const PARAM: opensips::cmd_param;

    unsafe fn from_void_ptr(p: *mut c_void) -> Self;
}

impl<'a> CommandFunctionParam for &'a str {
    const PARAM: opensips::cmd_param = opensips::cmd_param {
        flags: opensips::CMD_PARAM_STR,
        fixup: None,
        free_fixup: None,
    };

    /// # Safety
    ///
    /// This value needs to be non-NULL and be a valid UTF-8 C string.
    unsafe fn from_void_ptr(p: *mut c_void) -> Self {
        let p = p.cast::<opensips::str_>();
        let p = &*p;
        p.as_str()
    }
}

pub trait CommandFunction<Args> {
    const PARAMS: [opensips::cmd_param; 9];

    fn adapt(
        self,
        msg: *mut opensips::sip_msg,
        arg1: *mut c_void,
        arg2: *mut c_void,
        arg3: *mut c_void,
        arg4: *mut c_void,
        arg5: *mut c_void,
        arg6: *mut c_void,
        arg7: *mut c_void,
        arg8: *mut c_void,
    ) -> i32;
}

impl<F> CommandFunction<()> for F
where
    F: Fn() -> i32,
{
    const PARAMS: [opensips::cmd_param; 9] = [opensips::cmd_param::NULL; 9];

    fn adapt(
        self,
        _: *mut opensips::sip_msg,
        _: *mut c_void,
        _: *mut c_void,
        _: *mut c_void,
        _: *mut c_void,
        _: *mut c_void,
        _: *mut c_void,
        _: *mut c_void,
        _: *mut c_void,
    ) -> i32 {
        self()
    }
}

macro_rules! impl_command_function {
    ([[$($arg:ident),*], [$($n:tt),*]]) => {
        impl<F, $($arg,)*> CommandFunction<(&mut opensips::sip_msg, $($arg,)*)> for F
        where
            F: Fn(&mut opensips::sip_msg, $($arg,)*) -> i32,
            $($arg: CommandFunctionParam,)*
        {
            const PARAMS: [opensips::cmd_param; 9] = [
                $($arg::PARAM,)*
                $({stringify!($n); opensips::cmd_param::NULL},)*
                opensips::cmd_param::NULL,
            ];

            #[allow(non_snake_case)]
            fn adapt(
            self,
                #[allow(unused_variables)] msg: *mut opensips::sip_msg,
                $( $arg: *mut c_void,)*
                $($n: *mut c_void,)*
            ) -> i32 {
                // SAFETY: [OpenSIPS::valid]
                unsafe {
                    let msg = &mut *msg;

                    $(
                        let $arg = $arg::from_void_ptr($arg);
                    )*

                    self(msg, $($arg,)*)
                }
            }
        }
    }
}

impl_command_function!([[], [_, _, _, _, _, _, _, _]]);
impl_command_function!([[A1], [_, _, _, _, _, _, _]]);
impl_command_function!([[A1, A2], [_, _, _, _, _, _]]);
impl_command_function!([[A1, A2, A3], [_, _, _, _, _]]);
impl_command_function!([[A1, A2, A3, A4], [_, _, _, _]]);
impl_command_function!([[A1, A2, A3, A4, A5], [_, _, _]]);
impl_command_function!([[A1, A2, A3, A4, A5, A6], [_, _]]);
impl_command_function!([[A1, A2, A3, A4, A5, A6, A7], [_]]);
impl_command_function!([[A1, A2, A3, A4, A5, A6, A7, A8], []]);

/// Generates a `static CMDS` with the specified functions. Shims that
/// adapt from the OpenSIPS types will be automatically created. The
/// provided functions must either have no arguments or one [`&mut
/// opensips::sip_msg`] followed by up to eight additional arguments
/// of [known types][CommandFunctionParam].
///
/// ```rust,norun
/// opensips::commands! {
///     #[name = "any-name-you-want"]
///     fn the_name_of_a_function;
///
///     #[name = "ReallyAnyName"]
///     fn another_function;
/// }
/// ```
#[macro_export]
macro_rules! commands {
    ($(
        #[name = $name:literal]
        fn $fn_name:ident;
    )*) => {
        mod command_shim {
            use $crate::command::CommandFunction;
            use ::opensips::sip_msg;
            use ::std::os::raw::c_void;

            $(
                pub extern "C" fn $fn_name(
                    msg: *mut sip_msg,
                    arg1: *mut c_void,
                    arg2: *mut c_void,
                    arg3: *mut c_void,
                    arg4: *mut c_void,
                    arg5: *mut c_void,
                    arg6: *mut c_void,
                    arg7: *mut c_void,
                    arg8: *mut c_void,
                ) -> i32 {
                    super::$fn_name.adapt(msg, arg1, arg2, arg3, arg4, arg5, arg6, arg7, arg8)
                }
            )*
        }

        static CMDS: &[opensips::cmd_export_t] = {
            const fn get_params_for_command<CF, A>(_: &CF) -> [opensips::cmd_param; 9]
            where
                CF: $crate::command::CommandFunction<A>,
            {
                CF::PARAMS
            }

            &[
                $(
                    opensips::cmd_export_t {
                        name: cstr_lit!($name),
                        function: Some(command_shim::$fn_name),
                        params: get_params_for_command(&$fn_name),
                        // TODO: What do the flags really mean?
                        flags: opensips::REQUEST_ROUTE,
                    },
                )*
                    opensips::cmd_export_t::NULL,
            ]
        };
    };
}
