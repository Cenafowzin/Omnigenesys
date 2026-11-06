@tool
class_name OmniMapping
extends Resource
## What each name in a map becomes in the scene. Names are the pipeline's,
## exactly. Create one with right click in the FileSystem → New Resource →
## OmniMapping.

@export var tiles: Array[OmniTileRule] = []
@export var objects: Array[OmniObjectRule] = []


## Tile name → its rule (the last one, if a name repeats).
func tile_rules() -> Dictionary:
	var out := {}
	for r in tiles:
		if r != null and not r.tile.is_empty():
			out[r.tile] = r
	return out


## Object type → its rule (the last one, if a type repeats).
func object_rules() -> Dictionary:
	var out := {}
	for r in objects:
		if r != null and not r.type.is_empty():
			out[r.type] = r
	return out
