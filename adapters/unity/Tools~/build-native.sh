#!/usr/bin/env sh
# Builds the native library (crates/omnigenesys-ffi), copies it into the
# package, where Unity finds it, and writes any .meta still missing. Run from
# anywhere; needs Rust and Python 3.
#
#   sh adapters/unity/Tools~/build-native.sh
#
# The library is committed (no Git LFS: the Package Manager would fetch the
# pointers), so the package works from a git URL. Rebuild and commit it
# whenever crates/omnigenesys-ffi or the core changes.
#
# Windows x64 for now. Other platforms come with their targets (Linux .so,
# macOS universal .bundle via lipo, Android .so per ABI, iOS .a with
# DllImport "__Internal").
set -eu
tools=$(cd "$(dirname "$0")" && pwd)
package=$(cd "$tools/.." && pwd)
root=$(cd "$package/../.." && pwd)
cargo build --release -p omnigenesys_ffi --manifest-path "$root/Cargo.toml"
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*|Windows_NT)
    mkdir -p "$package/Runtime/Plugins/x86_64"
    cp "$root/target/release/omnigenesys.dll" "$package/Runtime/Plugins/x86_64/omnigenesys.dll"
    echo "copied omnigenesys.dll to Runtime/Plugins/x86_64"
    ;;
  *)
    echo "this platform is not wired up yet: build with cargo and copy the library by hand" >&2
    exit 1
    ;;
esac
python "$tools/make_metas.py"
