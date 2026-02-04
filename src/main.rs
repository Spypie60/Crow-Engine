#[allow(unused)]
use jni::{JNIVersion, JavaVM};
use libloading::{Library, Symbol};
use std::{env, fs::{self}, io::{Cursor, Read, Write}, sync::Mutex};
use flate2::write::ZlibEncoder;
use flate2::read::ZlibDecoder;
use lazy_static::lazy_static;
use flate2::Compression;
use tar::Archive;

#[allow(unused)]
async unsafe fn scan_and_load_mods(path: &str) -> Vec<Library> {
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
#[allow(unused)]
async unsafe fn load_crow_mod(jvm: &JavaVM, mod_path: &str) -> anyhow::Result<()> {
    // 1. Read and Decompress
    let compressed_bytes = fs::read(mod_path)?;
    let decompressed_data = decompressor(compressed_bytes).await;
    // 2. Wrap the decompressed data in a Tar Archive
    let mut archive = Archive::new(Cursor::new(decompressed_data.as_bytes()));
    let mut manifest_data = String::new();
    let mut binary_bytes: Vec<u8> = Vec::new();

    // 3. Iterate through files in the "Crow Nest"
    let target_bin: String = format!("bin/mod.{}", get_os_ext());
    for file in archive.entries()? {
        let mut file = file?;
        let path = file.path()?.to_owned();

        if path.to_str() == Some("mod.toml") {
            file.read_to_string(&mut manifest_data)?;
        } else if path.to_str() == Some(&target_bin) {
            file.read_to_end(&mut binary_bytes)?;
        }
    }

    // 4. Save the binary to a temp location for libloading
    let temp_dll = env::temp_dir().join(format!("crow_cache/{}", target_bin.replace("/", "_")));
    fs::create_dir_all(temp_dll.parent().unwrap())?;
    fs::write(&temp_dll, binary_bytes)?;

    // 5. Native Load
    let lib = unsafe { Library::new(&temp_dll) }?;
    type CrowInit = unsafe extern "C" fn(env_ptr: *mut jni::sys::JNIEnv);

    let init: Symbol<CrowInit> = unsafe { lib.get(b"crow_init") }?;

    // Get the pointer from your current JVM thread
    let env = jvm.attach_current_thread()?;
    let env_ptr = env.get_native_interface();

    // The Handshake!
    unsafe {init(env_ptr);}

    std::mem::forget(lib);
    Ok(())
}
lazy_static! {
    static ref LOADED_MODS: Mutex<Vec<Library>> = Mutex::new(Vec::new());
}
// In your main.rs
fn cleanup_crow() {
    println!("[CROW] Closing...");
    if let Ok(mut mods) = unsafe { LOADED_MODS.lock() } {
        mods.clear();
    }
}

pub fn broadcast_tick() {
    if let Ok(mods) = LOADED_MODS.lock() {
        for lib in mods.iter() {
            unsafe {
                if let Ok(tick_fn) = lib.get::<unsafe extern "C" fn()>(b"crow_tick") {
                    tick_fn();
                }
            }
        }
    }
}
async fn manepear()-> Vec<Library> {
    let active_mods = unsafe { scan_and_load_mods("./mods").await };
    println!("[CROW] {} mods are now active in memory.", active_mods.len());
    return active_mods;
}
fn main() {
    manepear();
}