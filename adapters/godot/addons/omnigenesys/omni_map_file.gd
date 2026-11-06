@tool
class_name OmniMapFile
extends Resource
## An imported .omap file: drag it into an OmniMapBuilder's Map File.

## The file as written (output v2 JSON); read with load_map().
@export_storage var text := ""


## The map, or null (the reason is logged).
func load_map() -> OmniMap:
	var map := OmniMap.from_omap(text)
	if map == null:
		push_error("[Omnigenesys] %s: %s" % [resource_path, OmniMap.last_error])
	return map
