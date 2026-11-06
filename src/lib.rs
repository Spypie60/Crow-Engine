mod error;
mod jvm;
mod log;
mod mods;
mod state;

pub use error::*;
pub use jvm::*;
pub use log::*;
pub use mods::*;
pub use state::*;

use anyhow::{Context, Result};
use jni::{sys::jint, Env, JavaVM};
use std::{
    ffi::c_void,
    sync::atomic::Ordering,
    thread,
    time::{Duration, Instant},
};

use crate::{jvm::with_env, log::clogger_warn};

fn cleanup_crow() {
    println!("[CROW] Closing...");
    
}

#[cfg(windows)]
#[unsafe(no_mangle)]
pub extern "system" fn DllMain(_: *mut c_void, reason: u32, _: *mut c_void) -> i32 {
    match reason {
        1 => {
            println!("[Crow] Minecraft Loading...");
            1
        }
        0 => {
            cleanup_crow();
            1
        }
        _ => 1,
    }
}

#[allow(unused)]
#[unsafe(no_mangle)]
pub extern "system" fn JNI_OnLoad(vm: JavaVM, _reserved: *mut c_void) -> jint {
    let _ = JVM.set(vm);
    init();
    jni::sys::JNI_VERSION_24
}

#[allow(unused)]
#[unsafe(no_mangle)]
pub extern "system" fn JNI_OnUnload(_vm: JavaVM, _reserved: *mut c_void) {
    cleanup_crow();
}

#[allow(unused)]
#[unsafe(no_mangle)]
pub extern "system" fn Agent_OnLoad(
    vm: *mut jni::sys::JavaVM,
    _options: *mut std::os::raw::c_char,
    _reserved: *mut std::os::raw::c_void,
) -> jni::sys::jint {
    println!("Loading Mixins");
    crow_mixer::Agent_OnLoad(vm, _options, _reserved)
}
/*
#[allow(unused)]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Agent_OnUnload(
    _vm: *mut jni::sys::JavaVM,
    _reserved: *mut std::ffi::c_void,
) {
    cleanup_crow();
}*/

#[unsafe(no_mangle)]
pub extern "C" fn init() {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::spawn(|| {
        if let Err(e) = crow_start() {
            eprintln!("[CROW ERROR] {e:?}");
        }
    });
}

fn crow_start() -> Result<()> {
    let begin = Instant::now();
    while JVM.get().is_none() {
        if begin.elapsed() > Duration::from_secs(30) {
            anyhow::bail!("JVM was never initialized (JNI_OnLoad never ran)");
        }
        thread::sleep(Duration::from_millis(50));
    }

    with_env(|env| {
        clogger_warn(env, "Working");
        init();
        Ok(())
    })
}
