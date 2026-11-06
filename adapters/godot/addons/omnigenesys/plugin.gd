@tool
extends EditorPlugin
## Registers the .omap importer while the plugin is enabled
## (Project → Project Settings → Plugins).

const Importer := preload("omni_import_plugin.gd")

var _importer: EditorImportPlugin


func _enter_tree() -> void:
	_importer = Importer.new()
	add_import_plugin(_importer)


func _exit_tree() -> void:
	remove_import_plugin(_importer)
	_importer = null
