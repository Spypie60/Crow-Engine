use jni::Env;
use std::{
    ffi::{c_char, CStr},
    sync::{atomic::AtomicBool, Mutex, OnceLock},
};

pub const CROW_VERSION: &CStr = unsafe {
    CStr::from_bytes_with_nul_unchecked(concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes())
};

pub static MOD_NAME: Mutex<Option<String>> = Mutex::new(None);
pub static JVM: OnceLock<jni::JavaVM> = OnceLock::new();
pub static STARTED: AtomicBool = AtomicBool::new(false);

#[repr(C)]
pub struct Main {
    pub env: *mut Env<'static>,
    pub crow_version: *const c_char,
}

pub fn mod_name(_env: &mut Env<'_>, name: impl ToString) {
    if let Ok(mut slot) = MOD_NAME.lock() {
        *slot = Some(name.to_string());
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn ccrow_version() -> *const c_char {
    CROW_VERSION.as_ptr()
}
