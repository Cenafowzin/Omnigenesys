#!/usr/bin/env sh
# Builds the Godot library (crates/omnigenesys-godot) and copies it into the
# addon, where the .gdextension file finds it. Run from anywhere; needs Rust.
#
#   sh adapters/godot/build-native.sh
#
# The library is committed, so the addon works without Rust. Rebuild and
# commit it whenever crates/omnigenesys-godot or the core changes. The C
# runtime goes inside it (crt-static): a classroom PC may not have the Visual
# C++ redistributable. --target keeps that flag away from build scripts and
# macros, which run on this machine. JOBS=1 for a PC short of memory.
#
# Windows x64 for now. Other platforms come with their targets and a line
# each in omnigenesys.gdextension.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
target=x86_64-pc-windows-msvc
# From the crate's folder: rust-toolchain.toml is looked up from there.
cd "$root/crates/omnigenesys-godot"
RUSTFLAGS="-C target-feature=+crt-static" cargo build --release -j "${JOBS:-2}" --target "$target"
mkdir -p "$here/addons/omnigenesys/bin"
cp "$root/crates/omnigenesys-godot/target/$target/release/omnigenesys_godot.dll" "$here/addons/omnigenesys/bin/"
echo "copied omnigenesys_godot.dll to addons/omnigenesys/bin"
