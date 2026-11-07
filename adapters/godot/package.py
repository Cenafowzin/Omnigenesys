"""Makes the zip people install: addons/omnigenesys and nothing else, what the
editor's AssetLib tab → Import expects and what a GitHub release can carry.

    sh adapters/godot/build-native.sh    (when the core or the crate changed)
    python adapters/godot/package.py

Writes adapters/godot/dist/omnigenesys-godot-<version>.zip. Stops if the
native library is missing or older than the code it is built from: a zip
that cannot generate must not go out by accident.
"""

import configparser
import os
import sys
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
ADDON = os.path.join(HERE, "addons", "omnigenesys")
LIBRARY = os.path.join(ADDON, "bin", "omnigenesys_godot.dll")
SOURCES = [
    os.path.join(ROOT, "crates", "omnigenesys-godot", "src"),
    os.path.join(ROOT, "crates", "omnigenesys-core", "src"),
]


def newest_source():
    times = []
    for top in SOURCES:
        for folder, _, files in os.walk(top):
            times += [os.path.getmtime(os.path.join(folder, f)) for f in files]
    return max(times)


def main():
    if not os.path.exists(LIBRARY):
        sys.exit("no native library: run sh adapters/godot/build-native.sh")
    if os.path.getmtime(LIBRARY) < newest_source():
        sys.exit("the native library is older than the code: run sh adapters/godot/build-native.sh")
    plugin = configparser.ConfigParser()
    plugin.read(os.path.join(ADDON, "plugin.cfg"), encoding="utf-8")
    version = plugin["plugin"]["version"].strip('"')

    os.makedirs(os.path.join(HERE, "dist"), exist_ok=True)
    out = os.path.join(HERE, "dist", f"omnigenesys-godot-{version}.zip")
    with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
        for folder, _, files in os.walk(ADDON):
            for f in sorted(files):
                # ~name.dll is the editor's working copy of the library, left
                # behind when Godot does not exit cleanly.
                if f.startswith("~"):
                    continue
                path = os.path.join(folder, f)
                z.write(path, os.path.relpath(path, HERE).replace(os.sep, "/"))
    print(f"{out}: {os.path.getsize(out) / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
