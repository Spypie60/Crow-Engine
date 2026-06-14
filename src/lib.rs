use jni::{JNIEnv, JavaVM, objects::{JString, JValue}, sys::jint};
use libloading::Library;
use std::{fs::{self}, io::{Read,Write}, ptr, sync::{Mutex, OnceLock}, thread::{self, sleep}, time::Duration};
use flate2::write::ZlibEncoder;
use flate2::read::ZlibDecoder;
use flate2::Compression;
use lazy_static::lazy_static;
use winapi::{ um::winuser::{MB_OK, MessageBoxA}};

#[allow(unused)]
#[tokio::main]
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
#[repr(C)]
pub struct Main { //what mod init provides pronounced "maze-ahhn" goofy tone
    pub env: JNIEnv<'static>,
    pub mod_id: String,
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
#[unsafe(no_mangle)]
pub extern "system" fn DllMain(_: *mut (), reason: u32, _: *mut ()) -> i32 {
    if reason == 1 { // DLL_PROCESS_ATTACH
        // Immediately spawn a thread to escape the Loader Lock
        thread::spawn(|| {
            unsafe {
                MessageBoxA(
                    ptr::null_mut(), 
                    "Crow Engine: Escaped Loader Lock!\0".as_ptr() as *const i8, 
                    "Crow\0".as_ptr() as *const i8, 
                    MB_OK
                );
            }
            
        });
        return 1;
    } else if reason == 0 { // DLL_PROCESS_DETACH {
        cleanup_crow();
        return 1;
    }
    1
}
#[unsafe(no_mangle)]
pub extern "system" fn JNI_OnLoad(vm: JavaVM, _reserved: *mut std::ffi::c_void) -> jint {
    let _ = JVM.set(vm);
    crow_main();
    return jni::sys::JNI_VERSION_1_8;
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
#[allow(non_snake_case)]
pub unsafe fn get_env() -> Option<JNIEnv<'static>> {
    // 1. Get the JVM global
    let jvm = match JVM.get() {
        Some(v) => v,
        None => {
            println!("JVM GLOBAL NOT FOUND");
            return None; 
        }
    };

    // 2. Attach the current thread as a daemon
    match jvm.attach_current_thread_as_daemon() {
        Ok(guard) => {
            let raw_ptr = guard.get_native_interface();
            println!("Found Thread Finishing");
            unsafe { Some(JNIEnv::from_raw(raw_ptr).ok()?) }

        },
        Err(_) => {
            println!("NO THREAD");
            None
        }
    }
}

static TICK_LISTENERS: Mutex<Vec<unsafe extern "C" fn(f32)>> = Mutex::new(Vec::new());
#[unsafe(no_mangle)]
#[allow(improper_ctypes_definitions)]
pub unsafe extern "C" fn get_minecraft_tps(_env: &mut JNIEnv<'_>) -> Result<f32, jni::errors::Error> { //its in the name come on
    crow_api::binding::net::minecraft::server::players
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

#[allow(improper_ctypes_definitions)]
pub unsafe extern "C" fn clogger(_env: &mut JNIEnv<'_>, message: impl ToString) { //crow logger
    if let Ok(log_manager) = _env.find_class("org/apache/logging/log4j/LogManager") {
        let engine_name = _env.new_string("CrowEngine").unwrap();
        if let Ok(logger_obj) = _env.call_static_method(
            log_manager,
            "getLogger",
            "(Ljava/lang/String;)Lorg/apache/logging/log4j/Logger;",
            &[JValue::Object(&engine_name)],
        ).and_then(|v| v.l()) {
            let j_msg = _env.new_string(message.to_string()).unwrap();
            let _ = _env.call_method(
                logger_obj,
                "info",
                "(Ljava/lang/String;)V",
                &[JValue::Object(&j_msg)],
            );
        }
    }
}
#[allow(improper_ctypes_definitions)]
pub unsafe extern "C" fn clogger_err(_env: &mut JNIEnv<'_>, message: impl ToString) { //crow error logger
    if let Ok(log_manager) = _env.find_class("org/apache/logging/log4j/LogManager") {
        let engine_name = _env.new_string("CrowEngine").unwrap();
        if let Ok(logger_obj) = _env.call_static_method(
            log_manager,
            "getLogger",
            "(Ljava/lang/String;)Lorg/apache/logging/log4j/Logger;",
            &[JValue::Object(&engine_name)],
        ).and_then(|v| v.l()) {
            let j_msg = _env.new_string(message.to_string()).unwrap();
            let _ = _env.call_method(
                logger_obj,
                "error",
                "(Ljava/lang/String;)V",
                &[JValue::Object(&j_msg)],
            );
        }
    }
}
#[allow(improper_ctypes_definitions)]
pub unsafe extern "C" fn clogger_warn(_env: &mut JNIEnv<'_>, message: impl ToString) { //self explanatory
    if let Ok(log_manager) = _env.find_class("org/apache/logging/log4j/LogManager") {
        let engine_name = _env.new_string("CrowEngine").unwrap();
        if let Ok(logger_obj) = _env.call_static_method(
            log_manager,
            "getLogger",
            "(Ljava/lang/String;)Lorg/apache/logging/log4j/Logger;",
            &[JValue::Object(&engine_name)],
        ).and_then(|v| v.l()) {
            let j_msg = _env.new_string(message.to_string()).unwrap();
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
    let active_mods = unsafe { scan_n_load_m("./mods") };
    for lib in active_mods.iter() {
        unsafe {
            init_mod(lib);
        }
    }

    unsafe { clogger(_env,format!("[CROW] {} mods are now active in memory.", active_mods.len())) };
   // return active_mods;
}
pub fn wait_load<'a>(env: &'a mut JNIEnv<'a>)-> Result<(), ()>{
    println!("[CROW] Waiting for Splash Screen to finish...");

    let mc_class = unsafe { env.find_class("net/minecraft/client/Minecraft").unwrap_unchecked() };

    // Get the static instance: Minecraft.getInstance()
    let mc_inst = env.call_static_method(
        mc_class,
        "getInstance",
        "()Lnet/minecraft/client/Minecraft;",
        &[]
    ).unwrap().l().unwrap();

    loop {
        // Field 'overlay' holds the Splash Screen (LoadingOverlay)
        // If overlay == null, the loading screen is GONE.
        let overlay = unsafe { env.get_field(
            &mc_inst,
            "overlay", // Obfuscated name might be 'bd' or similar in some builds
            "Lnet/minecraft/client/gui/screens/Overlay;"
        ).unwrap_unchecked().l().unwrap() };

        if overlay.is_null() {
            println!("[CROW] Splash Screen Finished! Window is ready.");
            break;
        }

        // Don't toast the CPU while waiting
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    Ok(())
}
pub fn wait_world<'a>(env: &'a mut JNIEnv<'a>)-> Result<(), ()>{
    println!("[CROW] Waiting for World to Load...");
    unsafe{clogger_warn(env, "Waiting for World to Load...");}

    let mc_class = unsafe { env.find_class("net/minecraft/client/Minecraft").unwrap_unchecked() };

    // Get the static instance: Minecraft.getInstance()
    let mc_inst = env.call_static_method(
        mc_class,
        "getInstance",
        "()Lnet/minecraft/client/Minecraft;",
        &[]
    ).unwrap().l().unwrap();

    loop {
        // Field 'level' holds the current world (ClientLevel)
        // If level != null, the world is loaded.
        let level = env.get_field(
            &mc_inst,
            "player", //
            "Lnet/minecraft/client/Minecraft;"
        );

        if !level.is_err() {
            println!("[CROW] World Loaded! Player can now join.");
            break;
        }

        // Don't toast the CPU while waiting
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    unsafe{clogger_err(env, "Waiting for World to Load...");}
    Ok(())

}


#[unsafe(no_mangle)]
#[allow(improper_ctypes_definitions)]
pub unsafe extern "C" fn ccrow_version()-> String{ // core crow version
    std::env::var("CARGO_PKG_VERSION").unwrap_or("0.0.0".to_string()).to_string()
}

#[allow(unused)]
#[unsafe(no_mangle)]
pub extern "system" fn crow_main() {
    let mut _env = unsafe { get_env().ok_or("JVM NOT ATTACHED") };
    //let mut e  = Box::new(_env);
    unsafe { clogger(&mut _env, "IT WORKEST".to_string()) };
    init(&mut _env);}
// In your main init function
pub fn init(env: &mut JNIEnv) {
    // Get the global VM handle
    let jvm = env.get_java_vm().expect("Failed to get JavaVM");
    crow_api::binding::jni_init(jni::Env, LoaderC).unwrap();

    std::thread::spawn(move || {

        let mut thread_env = jvm.attach_current_thread().expect("Failed to attach thread");
        let mut _env = unsafe { get_env().ok_or("JVM NOT ATTACHED") };
        let _ = wait_world(&mut _env);
        println!("[CROW] Logic started after world load.");



    println!("[CROW] DLL Attached. Background watcher is active.");
    });
}