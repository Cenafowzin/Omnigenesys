@tool
extends EditorImportPlugin
## Imports a .omap file as an OmniMapFile, checking it on the way: a map
## this reader cannot read fails the import, with the reason, instead of
## failing in the game. The imported resource is what an exported game
## ships (a raw .omap would be left out of the export).

const MapFile := preload("omni_map_file.gd")
const Map := preload("omni_map.gd")


func _get_importer_name() -> String:
	return "omnigenesys.omap"


func _get_visible_name() -> String:
	return "Omnigenesys map"


func _get_recognized_extensions() -> PackedStringArray:
	return PackedStringArray(["omap"])


func _get_save_extension() -> String:
	return "res"


func _get_resource_type() -> String:
	return "Resource"


func _get_preset_count() -> int:
	return 1


func _get_preset_name(_preset_index: int) -> String:
	return "Default"


func _get_import_options(_path: String, _preset_index: int) -> Array[Dictionary]:
	return []


func _get_option_visibility(_path: String, _option_name: StringName, _options: Dictionary) -> bool:
	return true


func _get_priority() -> float:
	return 1.0


func _get_import_order() -> int:
	return 0


func _import(source_file: String, save_path: String, _options: Dictionary,
		_platform_variants: Array[String], _gen_files: Array[String]) -> Error:
	var text := FileAccess.get_file_as_string(source_file)
	if text.is_empty():
		var err := FileAccess.get_open_error()
		if err != OK:
			return err
	if Map.from_omap(text) == null:
		push_error("[Omnigenesys] %s: %s" % [source_file, Map.last_error])
		return ERR_PARSE_ERROR
	var res := MapFile.new()
	res.text = text
	return ResourceSaver.save(res, "%s.%s" % [save_path, _get_save_extension()])
