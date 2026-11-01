//! Writes the C# bindings of `src/lib.rs` into the Unity package, so the
//! two never drift: change a function here and the next build changes the
//! C#. The file is checked in, so the package works without Rust.

fn main() {
    println!("cargo::rerun-if-changed=src/lib.rs");
    csbindgen::Builder::default()
        .input_extern_file("src/lib.rs")
        .csharp_dll_name("omnigenesys")
        // iOS links the static library into the game.
        .csharp_dll_name_if("UNITY_IOS && !UNITY_EDITOR", "__Internal")
        .csharp_namespace("Omnigenesys.Native")
        .csharp_class_name("NativeMethods")
        .csharp_class_accessibility("internal")
        // The status and kind codes, so C# never repeats a number.
        .csharp_generate_const_filter(|name| name.starts_with("OMNI_"))
        .generate_csharp_file("../../adapters/unity/Runtime/Native/NativeMethods.g.cs")
        .expect("C# bindings");
}
