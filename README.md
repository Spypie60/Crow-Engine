# Crow Engine
Crow Engine is a high-performance, native modding SDK and loader for Minecraft 1.21+. Unlike traditional loaders that run inside the Java Virtual Machine (JVM), Crow leverages Rust to provide direct hardware access, SIMD-accelerated physics, and low-level rendering hooks.

# 🚀 Why Crow?
Traditional modding hits a "Java Ceiling" where the Garbage Collector (GC) and JVM overhead limit what’s possible. Crow breaks this ceiling by moving heavy-duty logic to the native layer.

Native Performance: Run complex aerodynamics, cloth physics, and entity logic at C++ speeds.

Zero-Copy Rendering: Access Minecraft’s vertex buffers directly from Rust for ultra-smooth custom shaders and animations.

Safety First: Built-in "Panic Bridges" catch native crashes and report them through the standard Minecraft error screen—no more silent desktop closures.

Hot-Reloading: Modify your Rust code and hot-swap the logic in-game without restarting Minecraft.

# 🛠 Features

Native UI Layer: Immediate-mode GUIs (powered by egui) that render at your monitor's full refresh rate.

The "Handshake" Protocol: A stable JNI bridge that maps Minecraft’s obfuscated code to clean, type-safe Rust structs.

Signature Scanning: Survives minor game updates by auto-locating memory offsets for player data and rendering calls.

# 📦 Getting Started
Prerequisites
Rust (Stable or Nightly)

The Crow CLI: ``cargo install crow-cli`` or Download binaries

Initialize a new project with the physics template
`` crow new my-glider-mod --template=physics ``

Build and inject into a running instance
``cd my-glider-mod``
``crow fly``

or
``crow build``
``crow run``
# ⚖️ License
Crow Engine is licensed under the GNU GPL v3. We believe in keeping the "Native Wings" of Minecraft open and free. Any modifications to the core engine must be shared with the community, and original attribution is required.

