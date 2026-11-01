"""Writes the .meta of every file and folder of the package that has none.

A package installed from a git URL is read-only: Unity cannot make the
.meta files there, and it ignores every asset without one. So they are
committed. Each GUID comes from the asset's path in the package (an MD5),
so running this again gives the same ones; a .meta that exists is never
touched (Unity may have refined it).

    python adapters/unity/Tools~/make_metas.py
"""

import hashlib
import os
import sys

PACKAGE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
NAMESPACE = "com.omnigenesys.unity/"

FOLDER = """fileFormatVersion: 2
guid: {guid}
folderAsset: yes
DefaultImporter:
  externalObjects: {{}}
  userData:
  assetBundleName:
  assetBundleVariant:
"""

SCRIPT = """fileFormatVersion: 2
guid: {guid}
MonoImporter:
  externalObjects: {{}}
  serializedVersion: 2
  defaultReferences: []
  executionOrder: 0
  icon: {{instanceID: 0}}
  userData:
  assetBundleName:
  assetBundleVariant:
"""

ASMDEF = """fileFormatVersion: 2
guid: {guid}
AssemblyDefinitionImporter:
  externalObjects: {{}}
  userData:
  assetBundleName:
  assetBundleVariant:
"""

TEXT = """fileFormatVersion: 2
guid: {guid}
TextScriptImporter:
  externalObjects: {{}}
  userData:
  assetBundleName:
  assetBundleVariant:
"""

DEFAULT = """fileFormatVersion: 2
guid: {guid}
DefaultImporter:
  externalObjects: {{}}
  userData:
  assetBundleName:
  assetBundleVariant:
"""

# The Windows x64 library: the Windows editor and Windows x64 builds only.
# Left to Unity's defaults ("Any Platform"), an Android or macOS build would
# try to ship a Windows DLL.
PLUGIN_WIN64 = """fileFormatVersion: 2
guid: {guid}
PluginImporter:
  externalObjects: {{}}
  serializedVersion: 2
  iconMap: {{}}
  executionOrder: {{}}
  defineConstraints: []
  isPreloaded: 0
  isOverridable: 1
  isExplicitlyReferenced: 0
  validateReferences: 1
  platformData:
  - first:
      : Any
    second:
      enabled: 0
      settings:
        Exclude Editor: 0
        Exclude Linux64: 1
        Exclude OSXUniversal: 1
        Exclude Win: 1
        Exclude Win64: 0
  - first:
      Any:
    second:
      enabled: 0
      settings: {{}}
  - first:
      Editor: Editor
    second:
      enabled: 1
      settings:
        CPU: x86_64
        DefaultValueInitialized: true
        OS: Windows
  - first:
      Standalone: Linux64
    second:
      enabled: 0
      settings:
        CPU: None
  - first:
      Standalone: OSXUniversal
    second:
      enabled: 0
      settings:
        CPU: None
  - first:
      Standalone: Win
    second:
      enabled: 0
      settings:
        CPU: None
  - first:
      Standalone: Win64
    second:
      enabled: 1
      settings:
        CPU: x86_64
  userData:
  assetBundleName:
  assetBundleVariant:
"""


def template(rel, is_dir):
    if is_dir:
        return FOLDER
    name = os.path.basename(rel)
    ext = os.path.splitext(name)[1].lower()
    if ext == ".cs":
        return SCRIPT
    if ext == ".asmdef":
        return ASMDEF
    if ext in (".json", ".md", ".txt"):
        return TEXT
    if rel.replace("\\", "/") == "Runtime/Plugins/x86_64/omnigenesys.dll":
        return PLUGIN_WIN64
    if ext in (".dll", ".so", ".dylib", ".bundle", ".a"):
        sys.exit(f"{rel}: a native library with no platform settings here; add them before shipping it")
    return DEFAULT


def hidden(name):
    # Unity skips these, and so does this.
    return name.startswith(".") or name.endswith("~") or name.endswith(".meta")


def main():
    made = 0
    for top, dirs, files in os.walk(PACKAGE):
        dirs[:] = sorted(d for d in dirs if not hidden(d))
        here = os.path.relpath(top, PACKAGE)
        entries = [(d, True) for d in dirs] + [(f, False) for f in sorted(files) if not hidden(f)]
        for name, is_dir in entries:
            rel = os.path.normpath(os.path.join(here, name)) if here != "." else name
            meta = os.path.join(PACKAGE, rel) + ".meta"
            if os.path.exists(meta):
                continue
            guid = hashlib.md5((NAMESPACE + rel.replace("\\", "/")).encode()).hexdigest()
            with open(meta, "w", encoding="utf-8", newline="\n") as f:
                f.write(template(rel, is_dir).format(guid=guid))
            made += 1
            print("wrote", rel + ".meta")
    print(f"{made} .meta written")


if __name__ == "__main__":
    main()
