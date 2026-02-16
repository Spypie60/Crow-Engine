use jni::{JNIEnv, JavaVM, objects::JValue};
use libloading::Library;
use std::{fs::{self}, io::{Read, Write}, sync::{Mutex, OnceLock}};
use flate2::write::ZlibEncoder;
use flate2::read::ZlibDecoder;
use flate2::Compression;
use lazy_static::lazy_static;
#[allow(unused)]
async unsafe fn scan_n_load_m(path: &str) -> Vec<Library> {
    let mut loaded_libraries = Vec::new();
    let paths = fs::read_dir(path).expect("[CROW ERROR] Could not read mods directory");

    // 1. Prepare the temp cache directory
    let temp_cache = std::env::temp_dir().join("crow_cache");
    let _ = fs::remove_dir_all(&temp_cache); // Clean old session
    fs::create_dir_all(&temp_cache).unwrap();

    for entry in paths {
        if let Ok(entry) = entry {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("crow") {
                println!("[CROW] Extracting and loading: {:?}", path);

                // 2. Load and decompress the .crow file
                // (Using your existing decompressor logic)
                let compressed_data = fs::read(&path).unwrap();
                let decompressed_content = decompressor(compressed_data).await;

                // 3. Write to OS-specific temp file
                let dll_name = format!("{}.{}", path.file_stem().unwrap().to_str().unwrap(), get_os_ext());
                let dll_path = temp_cache.join(dll_name);
                fs::write(&dll_path, decompressed_content).unwrap();

                // 4. Load into memory
                match unsafe { Library::new(&dll_path) } {
                    Ok(lib) => {
                        loaded_libraries.push(lib);
                    }
                    Err(e) => println!("[CROW ERROR] Failed to load {:?}: {}", path, e),
                }
            }
        }
    }
    loaded_libraries
}
pub struct Masoin { //what mod init provides pronounced "maze-ahhn" goofy tone
    pub env: JNIEnv<'static>,
    pub mod_id: String,
    pub engine: Library,
    pub name: String,
    pub crow_id: String,
    pub crow_version: String,
}
#[allow(unused,forgetting_references)]
//injectinatoration 3000
#[unsafe(no_mangle)]
#[allow(improper_ctypes_definitions)]
pub unsafe extern "C" fn init_mod(lib: &Library){
    type Initifn = unsafe extern "C" fn(Masoin)-> std::io::Result<()>;
    let _init_lib: libloading::Symbol<fn()> = unsafe { lib.get("crow_init").unwrap() };
    _init_lib();
    std::mem::forget(lib);
}

#[allow(unused)]
pub async fn compressor(decompressed: String, compression_level: u32)-> Result<Vec<u8>, std::io::Error> {
    let mut c: ZlibEncoder<Vec<u8>> = ZlibEncoder::new(Vec::new(), Compression::new(compression_level));
    c.write_all(decompressed.as_bytes()).unwrap();
    let compressed_bytes = c.finish();
    return compressed_bytes;
}
//decompressinator 3000
#[allow(unused)]
pub async fn decompressor(compressed_data: Vec<u8>)-> String {
    let mut d = ZlibDecoder::new(&compressed_data[..]);
    let mut decompressed = String::new();
    d.read_to_string(&mut decompressed).expect("[CROW ERROR] Failed to decompress data");
    return decompressed;

}
#[allow(unused)]
pub fn get_os_ext() -> &'static str {
    if cfg!(target_os = "windows") { "dll" }
    else if cfg!(target_os = "linux") { "so" }
    else if cfg!(target_os = "macos") { "dylib" }
    else { panic!("[CROW ERROR] Unsupported OS!") }
}

lazy_static! {
    static ref LOADED_MODS: Mutex<Vec<Library>> = Mutex::new(Vec::new()); //well it stores loaded mods
}
static JVM: OnceLock<JavaVM> = OnceLock::new(); //I don't even know what this does "hey chatgpt what does this do?"
#[allow(unused)]
fn cleanup_crow() { //I shouldn't have to write this comment
    println!("[CROW] Closing...");
    if let Ok(mut mods) = LOADED_MODS.lock() {
        mods.clear();
    }
}


#[allow(unused)]
#[unsafe(no_mangle)]
pub extern "system" fn jni_onload(jvm: JavaVM, _reserved: *mut std::os::raw::c_void) { //on load
    let _ = JVM.set(jvm);
    unsafe { clogger(&mut get_env(), "message".to_string()) };
    crow_main();
    jni::sys::JNI_VERSION_1_8;
}
#[allow(unused)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn crunning(_env: &mut JNIEnv)-> bool { //crow running check
let client_class = _env.find_class("net/minecraft/client/Minecraft").expect("Failed to find Minecraft class");
let instance = _env.get_static_field(client_class,"instance","Lnet/minecraft/client/Minecraft;").expect("Failed to get Minecraft instance").l().expect("Instance is null");
let is_running = _env.get_field(instance,"running","Z").expect("Failed to get running field").z().expect("Failed to read boolean");
    if is_running {
        return false;
    } else if !is_running {
        return true;
        cleanup_crow();
    } else {
        unsafe {clogger_err(_env, "[CROW ERROR] How do you fail on a bool? anyways jni unload error".to_string());}
        return false;
    }
}
#[allow(unused)]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn get_env() -> JNIEnv<'static> { //get_env gets env. good luck troubleshooting this
    JVM.get().expect("[CROW ERROR] JVM NOT ATTACHED").attach_current_thread().expect(panic!("[CROW ERROR] Could not attach"));
    let guard = JVM.get().unwrap().attach_current_thread().unwrap();
    unsafe { JNIEnv::from_raw(guard.get_native_interface()).unwrap()}

}

static TICK_LISTENERS: Mutex<Vec<unsafe extern "C" fn(f32)>> = Mutex::new(Vec::new());
#[unsafe(no_mangle)]
#[allow(improper_ctypes_definitions)]
pub unsafe extern "C" fn get_minecraft_tps(_env: &mut JNIEnv<'_>) -> Result<f32, jni::errors::Error> { //its in the name come on
    let client_class = _env.find_class("net/minecraft/client/MinecraftClient")?;
    let instance = _env.get_static_field(client_class, "instance", "Lnet/minecraft/client/MinecraftClient;")?.l()?;
    let server = _env.get_field(instance, "server", "Lnet/minecraft/server/integrated/IntegratedServer;")?.l()?;
    if server.is_null() {
        return Ok(20.0);
    }
    let tick_time = _env.call_method(server, "getTickTime", "()F", &[])?.f()?;
    if tick_time > 0.0 {
        Ok(1000.0 / tick_time)
    } else {
        Ok(20.0) //default
    }
}
#[unsafe(no_mangle)]
#[allow(improper_ctypes_definitions)]
pub unsafe extern "C" fn clogger(_env: &mut JNIEnv<'_>, message: String) { //crow logger
    if let Ok(log_manager) = _env.find_class("org/apache/logging/log4j/LogManager") {
        let engine_name = _env.new_string("CrowEngine").unwrap();
        if let Ok(logger_obj) = _env.call_static_method(
            log_manager,
            "getLogger",
            "(Ljava/lang/String;)Lorg/apache/logging/log4j/Logger;",
            &[JValue::Object(&engine_name)],
        ).and_then(|v| v.l()) {
            let j_msg = _env.new_string(message).unwrap();
            let _ = _env.call_method(
                logger_obj,
                "info",
                "(Ljava/lang/String;)V",
                &[JValue::Object(&j_msg)],
            );
        }
    }
}
#[unsafe(no_mangle)]
#[allow(improper_ctypes_definitions)]
pub unsafe extern "C" fn clogger_err(_env: &mut JNIEnv<'_>, message: String) { //crow error logger
    if let Ok(log_manager) = _env.find_class("org/apache/logging/log4j/LogManager") {
        let engine_name = _env.new_string("CrowEngine").unwrap();
        if let Ok(logger_obj) = _env.call_static_method(
            log_manager,
            "getLogger",
            "(Ljava/lang/String;)Lorg/apache/logging/log4j/Logger;",
            &[JValue::Object(&engine_name)],
        ).and_then(|v| v.l()) {
            let j_msg = _env.new_string(message).unwrap();
            let _ = _env.call_method(
                logger_obj,
                "error",
                "(Ljava/lang/String;)V",
                &[JValue::Object(&j_msg)],
            );
        }
    }
}
#[unsafe(no_mangle)]
#[allow(improper_ctypes_definitions)]
pub unsafe extern "C" fn clogger_warn(_env: &mut JNIEnv<'_>, message: String) { //self explanatory
    if let Ok(log_manager) = _env.find_class("org/apache/logging/log4j/LogManager") {
        let engine_name = _env.new_string("CrowEngine").unwrap();
        if let Ok(logger_obj) = _env.call_static_method(
            log_manager,
            "getLogger",
            "(Ljava/lang/String;)Lorg/apache/logging/log4j/Logger;",
            &[JValue::Object(&engine_name)],
        ).and_then(|v| v.l()) {
            let j_msg = _env.new_string(message).unwrap();
            let _ = _env.call_method(
                logger_obj,
                "warn",
                "(Ljava/lang/String;)V",
                &[JValue::Object(&j_msg)],
            );
        }
    }
}
#[allow(unused)]
#[allow(improper_ctypes_definitions)]
pub fn crow_broadcast_tick(mut _env: JNIEnv<'_>, _class: jni::objects::JClass){ //I feel like api should manage this
    let tps = match unsafe { get_minecraft_tps(&mut _env) } {
        Ok(tps) => tps,
        Err(_e) => 20.0,
        _ => {unsafe { clogger_err(&mut _env, "[CROW ERROR] Something ain't right.(broadcast tick)".to_string()) }; 20.0},
    };
    let dt = 1.0/tps;
    if let Ok(listeners) = TICK_LISTENERS.lock() {
        for tick_func in listeners.iter() {
            unsafe {
                tick_func(dt);
            }
        }
    }

} //yes and no a reference
pub async fn crow_manepear(_env: &mut JNIEnv<'_>)/*-> Vec<Library>*/ {
    let active_mods = unsafe { scan_n_load_m("./mods").await };
    for lib in active_mods.iter() {
        unsafe {
            init_mod(lib);
        }
    }

    unsafe { clogger(_env,format!("[CROW] {} mods are now active in memory.", active_mods.len())) };
   // return active_mods;
}

#[unsafe(no_mangle)]
#[allow(improper_ctypes_definitions)]
pub unsafe extern "C" fn ccrow_version()-> String{ // core crow version
    std::env::var("ARGO_PKG_VERSION").unwrap_or("0.0.0".to_string()).to_string()
}
pub fn test_make_item(env: &mut JNIEnv) {
    // 1. Create Item.Properties
    // Java: new Item.Properties()
    let props_class = env.find_class("net/minecraft/world/item/Item$Properties").unwrap();
    let props_obj = env.new_object(props_class, "()V", &[]).unwrap();

    // 2. Create the Item instance
    // Java: new Item(props)
    let item_class = env.find_class("net/minecraft/world/item/Item").unwrap();
    let item_instance = env.new_object(
        item_class, 
        "(Lnet/minecraft/world/item/Item$Properties;)V", 
        &[(&props_obj).into()]
    ).unwrap();

    // 3. Create the ID (ResourceLocation)
    // Java: ResourceLocation.parse("crow:test_ruby")
    let rl_class = env.find_class("net/minecraft/resources/ResourceLocation").unwrap();
    let id_str = env.new_string("crow:test_ruby").unwrap();
    let resource_location = env.call_static_method(
        rl_class, 
        "parse", 
        "(Ljava/lang/String;)Lnet/minecraft/resources/ResourceLocation;", 
        &[(&id_str).into()]
    ).unwrap().l().unwrap();

    // 4. Register it to the Built-In Registry
    // Java: Registry.register(BuiltInRegistries.ITEM, id, item)
    let registries_class = env.find_class("net/minecraft/core/registries/BuiltInRegistries").unwrap();
    let item_registry = env.get_static_field(
        registries_class, 
        "ITEM", 
        "Lnet/minecraft/core/DefaultedRegistry;"
    ).unwrap().l().unwrap();

    let registry_class = env.find_class("net/minecraft/core/Registry").unwrap();
    env.call_static_method(
        registry_class, 
        "register", 
        "(Lnet/minecraft/core/Registry;Lnet/minecraft/resources/ResourceLocation;Ljava/lang/Object;)Ljava/lang/Object;", 
        &[(&item_registry).into(), (&resource_location).into(), (&item_instance).into()]
    ).expect("CRITICAL: Failed to register item!");

    println!("[Crow] Test Ruby successfully injected into Minecraft!");
}

#[allow(unused)]
#[unsafe(no_mangle)]
pub extern "system" fn crow_main() {
    let mut env = unsafe { get_env() };
    //crow_manepear(&mut env);
    test_make_item(&mut env);
}