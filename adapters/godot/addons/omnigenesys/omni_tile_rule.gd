@tool
class_name OmniTileRule
extends Resource
## What one tile name of the map becomes: a tile of the TileSet (painted
## into the layer's TileMapLayer) and/or a scene (one per cell, at its
## centre). With more than one, a variant layer of the map picks.

## The tile's name in the pipeline, exactly.
@export var tile := ""

@export_group("TileMapLayer")
## The TileSet source the tiles come from.
@export var source_id := 0
## The tile in that source. More than one: the variant layer picks;
## (-1, -1) paints nothing.
@export var atlas_coords: Array[Vector2i] = []
## The alternative tile (0: the tile itself).
@export var alternative := 0

@export_group("Scenes")
## Placed at the centre of every cell with this tile. More than one: the
## variant layer picks; an empty slot places nothing.
@export var scenes: Array[PackedScene] = []

@export_group("")
## An index layer of the map (the one a PickVariant step writes) whose
## number at the cell picks the variant. Empty: the first.
@export var variant_layer := ""
