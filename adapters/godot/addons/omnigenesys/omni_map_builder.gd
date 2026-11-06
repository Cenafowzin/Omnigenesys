class_name OmniMapBuilder
extends Node2D
## Builds an Omnigenesys map in the scene: each tile layer into the
## TileMapLayer of the same name, tile scenes at their cells' centres,
## object scenes on their boxes.
##
## The map's x grows to the right and its y downwards, as in Godot 2D, so
## cell (x, y) of the map is cell (x, y) of the TileMapLayer: the map's
## first cell sits at this node's origin (plus the pipeline's origin, if it
## has one). Scenes go to [code](x, y) * cell size[/code] in this node's
## space, so keep the TileMapLayers at this node's position. Rectangular
## cells; a 3D map (depth above 1) is not built here yet.

## After every build.
signal built(map: OmniMap)

## The map built on ready (an imported .omap).
@export var map_file: OmniMapFile
## What each tile name and object type becomes.
@export var mapping: OmniMapping
## The TileMapLayers to paint; each paints the map layer with its node's
## name. Empty: this node's TileMapLayer children.
@export var tile_layers: Array[TileMapLayer] = []
## Parent of the scenes made. Empty: this node.
@export var spawn_parent: Node
## One cell's size in pixels. Zero: the tile size of the first painted
## TileMapLayer's TileSet (16 × 16 without one).
@export var cell_size := Vector2.ZERO
## Build map_file when the node enters the scene (not in the editor).
@export var build_on_ready := true
## Print the names in the map the mapping does not cover, once per build.
@export var report_unmapped := true

## The map built last, or null.
var map: OmniMap

var _made: Array[Node] = []
var _painted: Array[TileMapLayer] = []
var _cell := Vector2(16, 16)


func _ready() -> void:
	if build_on_ready and map_file != null and not Engine.is_editor_hint():
		build_file(map_file)


## Builds an imported .omap file; false if it could not (logged).
func build_file(file: OmniMapFile) -> bool:
	var m := file.load_map()
	return m != null and build(m)


## Clears what the last build made, then builds [param m]; false if it
## could not (logged).
func build(m: OmniMap) -> bool:
	if m == null:
		push_error("%s: no map to build" % name)
		return false
	if mapping == null:
		push_error("%s: OmniMapBuilder has no mapping" % name)
		return false
	if m.depth != 1:
		push_error("%s: the map has %d levels; the 2D builder builds one" % [name, m.depth])
		return false
	clear()
	map = m
	_cell = _cell_size(m)
	var unmapped := {}
	var unbound := {}
	var notes: Array[String] = []
	var tile_rules := mapping.tile_rules()

	for layer in m.layers:
		if layer.kind != "tiles":
			continue
		var target := _target(layer.name)
		if target == null:
			unbound[layer.name] = true
		if not _paint(m, layer, target, tile_rules, unmapped, notes):
			return false

	var spawned: Variant = _spawn_objects(m, mapping.object_rules(), unmapped, notes)
	if spawned == null:
		return false
	for id: int in spawned:
		var node: Node = spawned[id]
		if node.has_method("_omni_spawned"):
			node.call("_omni_spawned", m.object_by_id(id), m, spawned)

	if report_unmapped and (not unmapped.is_empty() or not unbound.is_empty() or not notes.is_empty()):
		var text := "[Omnigenesys] %s:" % name
		if not unmapped.is_empty():
			text += " not in the mapping: %s." % ", ".join(_sorted(unmapped))
		if not unbound.is_empty():
			text += " Tile layers with no TileMapLayer: %s." % ", ".join(_sorted(unbound))
		for n in notes:
			text += " " + n
		print(text)
	built.emit(m)
	return true


## Clears the TileMapLayers the last build painted and frees the scenes it made.
func clear() -> void:
	for t in _painted:
		if is_instance_valid(t):
			t.clear()
	_painted.clear()
	for n in _made:
		if is_instance_valid(n):
			n.queue_free()
	_made.clear()
	map = null


## A point in the map's continuous coordinates (a cell's corner is a whole
## number; y down) in this node's space.
func local_of(x: float, y: float) -> Vector2:
	return Vector2(x, y) * _cell


func _target(layer_name: String) -> TileMapLayer:
	for t in _targets():
		if String(t.name) == layer_name:
			return t
	return null


func _targets() -> Array[TileMapLayer]:
	if not tile_layers.is_empty():
		return tile_layers
	var out: Array[TileMapLayer] = []
	for c in get_children():
		if c is TileMapLayer:
			out.append(c)
	return out


func _cell_size(m: OmniMap) -> Vector2:
	if cell_size != Vector2.ZERO:
		return cell_size
	for layer in m.layers:
		if layer.kind != "tiles":
			continue
		var t := _target(layer.name)
		if t != null and t.tile_set != null:
			return Vector2(t.tile_set.tile_size)
	return Vector2(16, 16)


func _paint(m: OmniMap, layer: OmniMap.Layer, target: TileMapLayer, rules: Dictionary,
		unmapped: Dictionary, notes: Array[String]) -> bool:
	var count := m.tiles.size()
	# Each tile id's rule and variant layer, once.
	var rule_of: Array = []
	rule_of.resize(count)
	var variants_of: Array = []
	variants_of.resize(count)
	for t in range(1, count):
		var rule: OmniTileRule = rules.get(m.tiles[t])
		rule_of[t] = rule
		if rule != null and not rule.variant_layer.is_empty():
			var v := m.layer(rule.variant_layer)
			if v == null or v.kind != "index":
				push_error("the mapping of tile '%s' picks variants by '%s', which is not an index layer of the map"
						% [m.tiles[t], rule.variant_layer])
				return false
			variants_of[t] = v.ids
	# The highest variant each tile drew, and in how many cells.
	var highest := PackedInt64Array()
	highest.resize(count)
	var cells := PackedInt64Array()
	cells.resize(count)

	if target != null:
		target.clear()
		_painted.append(target)
	var ids := layer.ids
	for y in m.height:
		for x in m.width:
			var i := y * m.width + x
			var t: int = ids[i]
			if t == 0:
				continue
			var rule: OmniTileRule = rule_of[t]
			if rule == null:
				unmapped["tile '%s'" % m.tiles[t]] = true
				continue
			var variant := 0
			if variants_of[t] != null:
				variant = variants_of[t][i]
			highest[t] = maxi(highest[t], variant)
			cells[t] += 1
			var cx := m.origin.x + x
			var cy := m.origin.y + y
			if target != null and not rule.atlas_coords.is_empty():
				if not _has(rule.atlas_coords.size(), variant, m.tiles[t]):
					return false
				target.set_cell(Vector2i(cx, cy), rule.source_id, rule.atlas_coords[variant], rule.alternative)
			if not rule.scenes.is_empty():
				if not _has(rule.scenes.size(), variant, m.tiles[t]):
					return false
				var scene := rule.scenes[variant]
				if scene != null:
					_make(scene, local_of(cx + 0.5, cy + 0.5))

	for t in range(1, count):
		var rule: OmniTileRule = rule_of[t]
		if variants_of[t] == null or cells[t] == 0:
			continue
		var has := maxi(rule.atlas_coords.size(), rule.scenes.size())
		if highest[t] + 1 < has:
			notes.append("Tile '%s' has %d variants in the mapping, but its %d cells in '%s' only used %d: if the pipeline has fewer variants, raise its count."
					% [m.tiles[t], has, cells[t], layer.name, highest[t] + 1])
	return true


## Object id → the node made for it; null if a variant is missing (logged).
func _spawn_objects(m: OmniMap, rules: Dictionary, unmapped: Dictionary, notes: Array[String]) -> Variant:
	var spawned := {}
	# Per type that picks by a prop: [objects, highest variant, scenes].
	var drawn := {}
	for o in m.objects:
		var rule: OmniObjectRule = rules.get(o.type)
		if rule == null or rule.scenes.is_empty():
			unmapped["object '%s'" % o.type] = true
			continue
		var variant := 0
		if not rule.variant_prop.is_empty():
			var p: Variant = o.props.get(rule.variant_prop)
			if o.prop_types.get(rule.variant_prop) != "int" or p < 0:
				push_error("object %d ('%s') has no whole prop '%s' to pick its scene" % [o.id, o.type, rule.variant_prop])
				return null
			variant = p
			var d: Array = drawn.get(o.type, [0, 0, rule.scenes.size()])
			drawn[o.type] = [d[0] + 1, maxi(d[1], variant), d[2]]
		if not _has(rule.scenes.size(), variant, o.type):
			return null
		var scene := rule.scenes[variant]
		if scene == null:
			continue
		var node := _make(scene, _pivot(o, rule.pivot))
		node.set_meta("omni_object", o)
		spawned[o.id] = node
	for type: String in _sorted(drawn):
		var d: Array = drawn[type]
		if d[1] + 1 < d[2]:
			notes.append("Object '%s' has %d scenes in the mapping, but its %d objects only used %d: if the pipeline has fewer variants, raise its count."
					% [type, d[2], d[0], d[1] + 1])
	return spawned


func _pivot(o: OmniMap.MapObject, pivot: OmniObjectRule.Pivot) -> Vector2:
	var x := float(o.position.x)
	var y := float(o.position.y)
	var w := float(o.size.x)
	var h := float(o.size.y)
	match pivot:
		OmniObjectRule.Pivot.CORNER:
			return local_of(x, y)
		OmniObjectRule.Pivot.BOTTOM_CENTER:
			return local_of(x + w / 2.0, y + h)
		_:
			return local_of(x + w / 2.0, y + h / 2.0)


## Variant [param i] of [param n]; past the end is an error, not a wrap.
func _has(n: int, i: int, what: String) -> bool:
	if i < n:
		return true
	push_error("'%s' asks for variant %d, but the mapping has %d: add them, or lower the pipeline's variant count" % [what, i, n])
	return false


func _make(scene: PackedScene, at: Vector2) -> Node:
	var node := scene.instantiate()
	var parent: Node = spawn_parent if spawn_parent != null else self
	parent.add_child(node, true)
	if node is Node2D:
		(node as Node2D).global_position = to_global(at)
	elif node is Control:
		(node as Control).global_position = to_global(at)
	_made.append(node)
	return node


static func _sorted(names: Dictionary) -> PackedStringArray:
	var keys := PackedStringArray(names.keys())
	keys.sort()
	return keys
