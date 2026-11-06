use anyhow::Result;
use jni::{
    jni_str,
    objects::JClass,
    signature::{FieldSignature, RuntimeMethodSignature},
    Env,
};
use libloading::Library;
use std::{
    fs::{self, File},
    io::{Cursor, Read, Seek},
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
use wasmtime::Engine;
use zip::ZipArchive;

use crate::{
    error::jni_err,
    state::{Main, CROW_VERSION, JVM, STARTED},
};

static LOADED_MODS: Mutex<Vec<Mod>> = Mutex::new(Vec::new());
static WASM_ENGINE: OnceLock<Engine> = OnceLock::new();
static TEMP_CACHE: OnceLock<PathBuf> = OnceLock::new();

pub enum WasNat {
    Wasm(wasmtime::Module),
    Native(Library),
}
impl WasNat {
    pub fn new_wasm(bytes: Vec<u8>) -> Self {
        self::WasNat::Wasm(wasmtime::Module::from_binary(wasm_engine(), &bytes).unwrap())
    }
    pub fn new_native(path: &Path) -> Self {
        self::WasNat::Native(unsafe { Library::new(path).unwrap() })
    }
    pub fn new(path: &Path) -> Self {
        if path.extension().unwrap() == "wasm" {
            let mut file = File::open(path).unwrap();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            self::WasNat::new_wasm(bytes)
        } else {
            self::WasNat::new_native(path)
        }
    }
}

fn os_ext() -> &'static str {
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

fn temp_cache() -> &'static Path {
    TEMP_CACHE.get_or_init(|| {
        let dir = std::env::temp_dir().join(format!("crow_cache_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        dir
    })
}
pub fn delete_temp_cache() {
    let _ = fs::remove_dir_all(temp_cache());
}
pub fn byte_native(bytes: Vec<u8>, name: String) -> PathBuf {
    let path = temp_cache().join(format!("{}.{}", name, os_ext()));
    fs::write(&path, bytes).unwrap();
    path
}
pub fn delete_native(name: String) {
    let path = temp_cache().join(format!("{}.{}", name, os_ext()));
    let _ = fs::remove_file(path);
}
fn wasm_engine() -> &'static Engine {
    WASM_ENGINE.get_or_init(Engine::default)
}
//================================================ LOADERS ================================================
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
        if path.extension().and_then(|s| s.to_str()) != Some(os_ext()) {
            continue;
        }
        println!("[CROW] Loading: {:?}", path);

        if let Err(e) = fs::copy(&path, cache.join(path.file_name().unwrap())) {
            println!("[CROW ERROR] Failed to copy {:?}: {e}", path);
            continue;
        }
        let dll_path = cache.join(path.file_name().unwrap());

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

//================================================ MOD DEFINITIONS ================================================

pub struct Mod {
    pub info: Info,
    pub bin: Option<WasNat>,
}
impl Mod {
    pub fn extract(file: File) -> Self {
        let mut reader = ZipArchive::new(file).unwrap();
        let mut info = String::new();
        reader
            .by_name("crowmod.toml")
            .unwrap()
            .read_to_string(&mut info)
            .unwrap();
        let info = Info::from_toml(&info);
        Self { info, bin: None }
    }
    pub fn load_wasm(&mut self, file: File) -> Result<()> {
        let mut reader = ZipArchive::new(file)?;
        let mut bin = Vec::new();

        let mut entry = reader.by_name("mod.wasm")?;
        entry.read_to_end(&mut bin)?;
        self.bin = Some(WasNat::new_wasm(bin));
        Ok(())
    }
    pub fn load_native(&mut self, file: File) -> Result<()> {
        let mut reader = ZipArchive::new(file)?;
        let mut bin = Vec::new();
        let name = format!("mod.{}", os_ext());
        let mut entry = reader.by_name(&name).unwrap();
        entry.read_to_end(&mut bin)?;
        let bin = &byte_native(bin, self.info.name.clone());
        self.bin = Some(WasNat::new_native(bin));
        Ok(())
    }
    pub fn load(&mut self, file: File) -> Result<()> {
        let reader = ZipArchive::new(file.try_clone()?)?;
        let err = reader.index_for_name("mod.wasm").is_none();
        if err {
            self.load_native(file.try_clone()?)?;
        } else {
            self.load_wasm(file)?;
        }
        Ok(())
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Info {
    pub name: String,
    pub desc: Option<String>,
    pub version: String,
    pub mc_version: String,
    pub engine_version: String,
    pub authors: Vec<String>,
    pub ctype: ModType,
}
impl Info {
    pub fn from_toml(s: &str) -> Info {
        toml::from_str(s).unwrap()
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
pub enum ModType {
    Wasm,
    Native,
    Both,
    Limited {
        win: bool,
        mac: bool,
        linux: bool,
        wasm: bool,
    },
}
// ================================================ TICK ================================================

static TICK_LISTENERS: Mutex<Vec<unsafe extern "C" fn(f32)>> = Mutex::new(Vec::new());

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

pub fn crow_broadcast_tick(mut env: Env<'_>, _class: JClass) {
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
