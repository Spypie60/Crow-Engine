use anyhow::{anyhow, Context, Result};
use jni::{
    jni_str,
    objects::JValue,
    signature::{FieldSignature, RuntimeMethodSignature},
    sys::jint,
    vm::ScopeToken,
    Env, JavaVM,
};
use libloading::Library;
use std::{
    ffi::{c_char, c_void, CStr},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

fn get_os_ext() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "dll"
    }
    #[cfg(target_os = "linux")]
    {
        "so"
    }
    #[cfg(target_os = "macos")]
    {
        "dylib"
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        "so"
    }
}

pub enum WasNat {
    Wasm(wasmtime::Module),
    Native(Library),
}

static LOADED_MODS: Mutex<Vec<WasNat>> = Mutex::new(Vec::new());
static TICK_LISTENERS: Mutex<Vec<unsafe extern "C" fn(f32)>> = Mutex::new(Vec::new());
static MOD_NAME: Mutex<Option<String>> = Mutex::new(None);
static JVM: OnceLock<JavaVM> = OnceLock::new();
static WASM_ENGINE: OnceLock<wasmtime::Engine> = OnceLock::new();
static STARTED: AtomicBool = AtomicBool::new(false);
static TEMP_CACHE: OnceLock<PathBuf> = OnceLock::new();

const CROW_VERSION: &CStr = unsafe {
    CStr::from_bytes_with_nul_unchecked(concat!(env!("CARGO_PKG_VERSION"), "\0").as_bytes())
};

/// ABI passed to `crow_init` in each native mod.
/// `env` is only valid for the duration of that call — do not store it.
#[repr(C)]
pub struct Main {
    pub env: *mut Env<'static>,
    pub crow_version: *const c_char,
}

fn temp_cache() -> &'static Path {
    TEMP_CACHE.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("crow_cache_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        dir
    })
}

fn wasm_engine() -> &'static wasmtime::Engine {
    WASM_ENGINE.get_or_init(wasmtime::Engine::default)
}

fn decompressor(raw: &[u8]) -> Result<Vec<u8>> {
    fn decode(mut decoder: impl Read) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        decoder.read_to_end(&mut out)?;
        Ok(out)
    }
    if raw.starts_with(&[0x1f, 0x8b]) {
        return decode(flate2::read::GzDecoder::new(raw));
    }

    match decode(flate2::read::ZlibDecoder::new(raw)) {
        Ok(out) if !out.is_empty() || raw.is_empty() => Ok(out),
        _ => decode(flate2::read::DeflateDecoder::new(raw)),
    }
}

fn jni_err(err: impl std::fmt::Debug) -> anyhow::Error {
    anyhow!("{err:?}")
}

/// Attach this thread to the JVM and run `f`. The `Env` must not outlive `f`.
pub fn with_env<F, R>(f: F) -> Result<R>
where
    F: FnOnce(&mut Env<'_>) -> Result<R>,
{
    let jvm = JVM.get().context("[Crow Engine] JVM GLOBAL NOT FOUND")?;
    let mut scope = ScopeToken::default();
    let mut attachment = unsafe {
        jvm.get_env_attachment(&mut scope)
            .map_err(jni_err)
            .context("[Crow Engine] Bad day for getting Env")?
    };
    let env = attachment.borrow_env_mut();
    f(env)
}

fn load_wasms(dir: &Path, loaded: &mut Vec<WasNat>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("wasm") {
            continue;
        }
        match fs::read(&path).and_then(|bytes| {
            wasmtime::Module::new(wasm_engine(), &bytes)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
        }) {
            Ok(module) => {
                println!("[CROW] Loaded wasm module: {:?}", path);
                loaded.push(WasNat::Wasm(module));
            }
            Err(e) => println!("[CROW ERROR] Failed to load wasm {:?}: {e}", path),
        }
    }
}

fn load_natives(dir: &Path, loaded: &mut Vec<WasNat>) {
    let Ok(entries) = fs::read_dir(dir) else {
        println!("[CROW ERROR] Could not read mods directory: {:?}", dir);
        return;
    };

    let cache = temp_cache();

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("crow") {
            continue;
        }
        println!("[CROW] Extracting and loading: {:?}", path);

        let compressed = match fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                println!("[CROW ERROR] Failed to read {:?}: {e}", path);
                continue;
            }
        };
        let decompressed = match decompressor(&compressed) {
            Ok(b) => b,
            Err(e) => {
                println!("[CROW ERROR] Failed to decompress {:?}: {e}", path);
                continue;
            }
        };

        if decompressed.starts_with(b"\0asm") {
            match wasmtime::Module::new(wasm_engine(), &decompressed) {
                Ok(module) => {
                    println!("[CROW] Loaded wasm from .crow: {:?}", path);
                    loaded.push(WasNat::Wasm(module));
                }
                Err(e) => println!("[CROW ERROR] Invalid wasm in {:?}: {e}", path),
            }
            continue;
        }

        let dll_name = format!(
            "{}.{}",
            path.file_stem().and_then(|s| s.to_str()).unwrap_or("mod"),
            get_os_ext()
        );
        let dll_path = cache.join(dll_name);
        if let Err(e) = fs::write(&dll_path, &decompressed) {
            println!("[CROW ERROR] Failed to write {:?}: {e}", dll_path);
            continue;
        }

        match unsafe { Library::new(&dll_path) } {
            Ok(lib) => loaded.push(WasNat::Native(lib)),
            Err(e) => println!("[CROW ERROR] Failed to load {:?}: {e}", path),
        }
    }
}

unsafe fn init_mod(lib: &Library, env: &mut Env<'_>) {
    let symbol = match unsafe { lib.get::<unsafe extern "C" fn(*mut Main) -> i32>(b"crow_init\0") }
    {
        Ok(s) => s,
        Err(e) => {
            println!("[CROW ERROR] Missing crow_init: {e}");
            return;
        }
    };

    let mut main = Main {
        env: env as *mut Env<'_> as *mut Env<'static>,
        crow_version: CROW_VERSION.as_ptr(),
    };
    let rc = unsafe { symbol(&mut main) };
    if rc != 0 {
        println!("[CROW ERROR] crow_init returned {rc}");
    }
}

fn cleanup_crow() {
    println!("[CROW] Closing...");
    if let Ok(mut mods) = LOADED_MODS.lock() {
        mods.clear();
    }
    if let Ok(mut listeners) = TICK_LISTENERS.lock() {
        listeners.clear();
    }
    if let Some(cache) = TEMP_CACHE.get() {
        let _ = fs::remove_dir_all(cache);
    }
}

#[cfg(windows)]
#[unsafe(no_mangle)]
pub extern "system" fn DllMain(_: *mut c_void, reason: u32, _: *mut c_void) -> i32 {
    match reason {
        1 => {
            // DLL_PROCESS_ATTACH
            crow_main();
            1
        }
        0 => {
            // DLL_PROCESS_DETACH
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
    crow_main();
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
    _options: *const std::os::raw::c_char,
    _reserved: *mut std::os::raw::c_void,
) -> jni::sys::jint {
    println!("Loading Mixins");
    crow_mixer::Agent_OnLoad(vm, _options, _reserved)
}
#[allow(unused)]
#[unsafe(no_mangle)]
pub unsafe extern "system" fn Agent_OnUnload(
    _vm: *mut jni::sys::JavaVM,
    _reserved: *mut std::ffi::c_void,
) {
    cleanup_crow();
}
/// `true` when the Minecraft client is running.
pub fn crunning(env: &mut Env<'_>) -> bool {
    let Ok(client_class) = env.find_class(jni_str!("net/minecraft/client/Minecraft")) else {
        return false;
    };
    let sig = unsafe {
        FieldSignature::from_raw_parts(
            jni_str!("Lnet/minecraft/client/Minecraft;"),
            jni::signature::JavaType::Object,
        )
    };
    let instance = match env
        .get_static_field(client_class, jni_str!("instance"), sig)
        .and_then(|v| v.l())
    {
        Ok(obj) => obj,
        Err(_) => return false,
    };
    let sig = unsafe {
        FieldSignature::from_raw_parts(
            jni_str!("Z"),
            jni::signature::JavaType::Primitive(jni::signature::Primitive::Boolean),
        )
    };
    env.get_field(instance, jni_str!("running"), sig)
        .and_then(|v| v.z())
        .unwrap_or(false)
}

#[derive(Clone, Copy)]
enum LogLevel {
    Info,
    Warn,
    Error,
}

fn log_java(env: &mut Env<'_>, level: LogLevel, message: impl ToString) {
    let Ok(log_manager) = env.find_class(jni_str!("org/apache/logging/log4j/LogManager")) else {
        eprintln!("[CROW] {}", message.to_string());
        return;
    };
    let get_logger =
        RuntimeMethodSignature::from_str("(Ljava/lang/String;)Lorg/apache/logging/log4j/Logger;")
            .unwrap();
    let engine_name = match env.new_string("CrowEngine") {
        Ok(s) => s,
        Err(_) => return,
    };
    let Ok(logger_obj) = env
        .call_static_method(
            log_manager,
            jni_str!("getLogger"),
            get_logger.method_signature(),
            &[JValue::Object(&engine_name)],
        )
        .and_then(|v| v.l())
    else {
        return;
    };
    let method = match level {
        LogLevel::Info => jni_str!("info"),
        LogLevel::Warn => jni_str!("warn"),
        LogLevel::Error => jni_str!("error"),
    };
    let sig = RuntimeMethodSignature::from_str("(Ljava/lang/String;)V").unwrap();
    let sig = sig.method_signature();
    if let Ok(j_msg) = env.new_string(message.to_string()) {
        let _ = env.call_method(logger_obj, method, sig, &[JValue::Object(&j_msg)]);
    }
}

pub fn clogger(env: &mut Env<'_>, message: impl ToString) {
    log_java(env, LogLevel::Info, message);
}

pub fn clogger_warn(env: &mut Env<'_>, message: impl ToString) {
    log_java(env, LogLevel::Warn, message);
}

pub fn clogger_err(env: &mut Env<'_>, message: impl ToString) {
    log_java(env, LogLevel::Error, message);
}

/// Stores a brand name for mods to read. Replacing `getServerModName()` needs a Java mixin.
pub fn mod_name(_env: &mut Env<'_>, name: impl ToString) {
    if let Ok(mut slot) = MOD_NAME.lock() {
        *slot = Some(name.to_string());
    }
}

pub fn get_tps(env: &mut Env<'_>) -> Result<f32> {
    let client_class = env
        .find_class(jni_str!("net/minecraft/client/Minecraft"))
        .map_err(jni_err)?;
    let sig = unsafe {
        FieldSignature::from_raw_parts(
            jni_str!("Lnet/minecraft/client/Minecraft;"),
            jni::signature::JavaType::Object,
        )
    };
    let instance = env
        .get_static_field(client_class, jni_str!("instance"), sig)
        .and_then(|v| v.l())
        .map_err(jni_err)?;

    let sig = RuntimeMethodSignature::from_str("()Lnet/minecraft/server/MinecraftServer;").unwrap();
    let sig = sig.method_signature();
    let server = match env
        .call_method(instance, jni_str!("getSingleplayerServer"), sig, &[])
        .and_then(|v| v.l())
    {
        Ok(s) if !s.is_null() => s,
        _ => return Ok(20.0),
    };

    let mgr_sig = unsafe {
        FieldSignature::from_raw_parts(
            jni_str!("Lnet/minecraft/world/TickRateManager;"),
            jni::signature::JavaType::Object,
        )
    };
    let manager = match env
        .get_field(server, jni_str!("tickRateManager"), mgr_sig)
        .and_then(|v| v.l())
    {
        Ok(m) if !m.is_null() => m,
        _ => return Ok(20.0),
    };

    let sig = RuntimeMethodSignature::from_str("()F").unwrap();
    let sig = sig.method_signature();
    match env
        .call_method(manager, jni_str!("tickrate"), sig, &[])
        .and_then(|v| v.f())
    {
        Ok(tps) if tps > 0.0 => Ok(tps),
        _ => Ok(20.0),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn crow_register_tick(tick: unsafe extern "C" fn(f32)) {
    if let Ok(mut listeners) = TICK_LISTENERS.lock() {
        listeners.push(tick);
    }
}

pub fn crow_broadcast_tick(mut env: Env<'_>, _class: jni::objects::JClass) {
    let tps = get_tps(&mut env).unwrap_or(20.0).max(0.001);
    let dt = 1.0 / tps;
    if let Ok(listeners) = TICK_LISTENERS.lock() {
        for tick_func in listeners.iter() {
            unsafe {
                tick_func(dt);
            }
        }
    }
}

pub fn crow_manepear(env: &mut Env<'_>) -> Result<()> {
    let mods_dir = PathBuf::from("./mods");
    if !mods_dir.exists() {
        fs::create_dir_all(&mods_dir).context("failed to create ./mods")?;
        println!("[CROW] Created empty mods directory.");
        return Ok(());
    }

    let mut loaded = Vec::new();
    load_natives(&mods_dir, &mut loaded);
    load_wasms(&mods_dir, &mut loaded);

    for item in &loaded {
        if let WasNat::Native(lib) = item {
            unsafe {
                init_mod(lib, env);
            }
        }
    }

    let count = loaded.len();
    if let Ok(mut mods) = LOADED_MODS.lock() {
        mods.extend(loaded);
    }
    println!("[CROW] {count} mods are now active in memory.");
    Ok(())
}

#[unsafe(no_mangle)]
pub extern "C" fn ccrow_version() -> *const c_char {
    CROW_VERSION.as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn crow_main() {
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
        clogger(env, "IT WORKEST");
        crow_manepear(env)
    })
}

pub fn init(_env: &mut Env<'_>) {
    crow_main();
}
