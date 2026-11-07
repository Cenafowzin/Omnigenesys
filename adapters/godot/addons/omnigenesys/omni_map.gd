@tool
class_name OmniMap
extends RefCounted
## An Omnigenesys map (output v2, a .omap file), decoded.
##
## Cells are numbered x first, then y, then z: [method cell_index]. The
## map's x grows to the right and its y downwards (row 0 at the top), as in
## Godot 2D; z are the levels of a 3D map. A reader of format 2: it refuses
## another major version, an unknown layer kind or encoding, and ignores
## fields it does not know.

const FORMAT := "omnigenesys-map"
const VERSION := 2

## Why the last [method from_omap] returned null.
static var last_error := ""

var width := 0
var height := 0
var depth := 0
## The map's first cell (its top-left, level 0) in the pipeline's coordinates.
var origin := Vector3i.ZERO
## The seed, as written: a whole number up to 2^64 - 1, beyond Godot's int.
var seed_text := ""
var attempt := 0
## Tile names by id; id 0 is always "empty".
var tiles := PackedStringArray()
## In draw order.
var layers: Array[Layer] = []
var objects: Array[MapObject] = []

var _layers_by_name := {}
var _objects_by_id := {}


## A tile, index or scalar layer.
class Layer:
	var name := ""
	## "tiles", "index" or "scalar".
	var kind := ""
	## Tile ids (tiles) or numbers (index), one per cell.
	var ids := PackedInt64Array()
	## One per cell (scalar).
	var values := PackedFloat32Array()


## An object of the map: its type, its box in cells and its props.
class MapObject:
	var id := 0
	## What it is: what the mapping dispatches on.
	var type := ""
	## The object layer it is in.
	var layer := ""
	## The box's first cell.
	var position := Vector3i.ZERO
	## The box's size in cells (depth 1 in a 2D map).
	var size := Vector3i.ONE
	## Name → value: int, float, bool, String, Vector3i (a cell), int (an
	## object's id), Array (a list).
	var props := {}
	## Name → "int", "float", "bool", "string", "cell", "object" or "list".
	var prop_types := {}

	func prop(prop_name: String, fallback: Variant = null) -> Variant:
		return props.get(prop_name, fallback)


## The index of cell (x, y, z), counted from the map's first cell.
func cell_index(x: int, y: int, z: int = 0) -> int:
	return (z * height + y) * width + x


## The layer called [param layer_name], or null.
func layer(layer_name: String) -> Layer:
	return _layers_by_name.get(layer_name)


## The object with id [param id], or null.
func object_by_id(id: int) -> MapObject:
	return _objects_by_id.get(id)


## Reads a .omap file's text; null if it cannot ([member last_error] says why).
static func from_omap(text: String) -> OmniMap:
	last_error = ""
	var json := JSON.new()
	if json.parse(text) != OK:
		return _fail("JSON line %d: %s" % [json.get_error_line(), json.get_error_message()])
	var root: Variant = json.data
	if not (root is Dictionary):
		return _fail("a .omap file is a JSON object")
	if root.get("format") != FORMAT:
		return _fail('not an Omnigenesys map (format is not "%s")' % FORMAT)
	var version: Variant = root.get("version")
	if not _is_whole(version) or int(version) != VERSION:
		return _fail("map format version %s; this reader knows %d" % [str(version), VERSION])

	var map := OmniMap.new()
	var size: Variant = _ints(root.get("size"), 3, "size")
	var first: Variant = _ints(root.get("origin"), 3, "origin")
	if size == null or first == null:
		return null
	map.width = size[0]
	map.height = size[1]
	map.depth = size[2]
	map.origin = Vector3i(first[0], first[1], first[2])
	var seed_value: Variant = root.get("seed")
	if not (seed_value is String) or seed_value.is_empty() \
			or not seed_value.is_valid_int() or seed_value.begins_with("-") or seed_value.begins_with("+"):
		return _fail("seed '%s' is not a whole number" % str(seed_value))
	map.seed_text = seed_value
	var attempt_value: Variant = root.get("attempt", 0)
	map.attempt = int(attempt_value) if _is_whole(attempt_value) else 0

	var names: Variant = root.get("tiles")
	if not (names is Array):
		return _fail("'tiles' is not a list")
	for t: Variant in names:
		if not (t is String):
			return _fail("a tile name is not a string")
		map.tiles.append(t)

	var cells := map.width * map.height * map.depth
	var layer_list: Variant = root.get("layers")
	if not (layer_list is Array):
		return _fail("'layers' is not a list")
	for item: Variant in layer_list:
		if not (item is Dictionary):
			return _fail("a layer is not an object")
		var l: Layer = _layer(item, cells, map.tiles.size())
		if l == null:
			return null
		map.layers.append(l)
		map._layers_by_name[l.name] = l

	var object_list: Variant = root.get("objects")
	if not (object_list is Array):
		return _fail("'objects' is not a list")
	for item: Variant in object_list:
		var o: MapObject = _object(item)
		if o == null:
			return null
		map.objects.append(o)
		map._objects_by_id[o.id] = o
	return map


## A map from the native library (OmniGeneration.result): the same map a
## .omap file holds, in packed arrays, no number through JSON. Null if it
## carries an error ([member last_error] says it).
static func from_native(data: Dictionary) -> OmniMap:
	last_error = ""
	if data.has("error"):
		return _fail(data["error"])
	var map := OmniMap.new()
	map.width = data["width"]
	map.height = data["height"]
	map.depth = data["depth"]
	map.origin = data["origin"]
	map.seed_text = data["seed"]
	map.attempt = data["attempt"]
	map.tiles = data["tiles"]
	for item: Dictionary in data["layers"]:
		var l := Layer.new()
		l.name = item["name"]
		l.kind = item["kind"]
		if l.kind == "scalar":
			l.values = item["values"]
		else:
			l.ids = item["ids"]
		map.layers.append(l)
		map._layers_by_name[l.name] = l
	var json := JSON.new()
	if json.parse(data["objects_json"]) != OK:
		return _fail("objects: JSON line %d: %s" % [json.get_error_line(), json.get_error_message()])
	for item: Variant in json.data:
		var o: MapObject = _object(item)
		if o == null:
			return null
		map.objects.append(o)
		map._objects_by_id[o.id] = o
	return map


static func _layer(l: Dictionary, cells: int, tile_count: int) -> Layer:
	var layer_name: Variant = l.get("name")
	var kind: Variant = l.get("kind")
	var encoding: Variant = l.get("encoding")
	if not (layer_name is String and kind is String and encoding is String):
		return _fail("a layer needs a name, a kind and an encoding")
	var out := Layer.new()
	out.name = layer_name
	out.kind = kind
	if kind == "scalar":
		if encoding != "dense":
			return _fail("layer '%s': encoding '%s' is unknown for a scalar layer" % [layer_name, encoding])
		var values: Variant = l.get("values")
		if not (values is Array) or values.size() != cells:
			return _fail("layer '%s': 'values' does not hold %d numbers" % [layer_name, cells])
		out.values.resize(cells)
		for i in cells:
			var v: Variant = values[i]
			if not (v is float or v is int):
				return _fail("layer '%s': value %d is not a number" % [layer_name, i])
			out.values[i] = v
		return out
	if kind != "tiles" and kind != "index":
		return _fail("layer '%s': kind '%s' is unknown (a newer format?)" % [layer_name, kind])

	if encoding == "rle":
		var runs: Variant = l.get("runs")
		if not (runs is Array) or runs.size() % 2 != 0:
			return _fail("layer '%s': runs come in pairs (value, count)" % layer_name)
		out.ids.resize(cells)
		var at := 0
		for i in range(0, runs.size(), 2):
			var v: Variant = runs[i]
			var n: Variant = runs[i + 1]
			if not _is_whole(v) or not _is_whole(n) or v < 0 or n < 0:
				return _fail("layer '%s': run %d is not two whole numbers" % [layer_name, i >> 1])
			if at + int(n) > cells:
				return _fail("layer '%s': the runs hold more than %d cells" % [layer_name, cells])
			for k in int(n):
				out.ids[at + k] = int(v)
			at += int(n)
		if at != cells:
			return _fail("layer '%s': the runs hold %d cells; the map has %d" % [layer_name, at, cells])
	elif encoding == "dense":
		var list: Variant = l.get("cells")
		if not (list is Array) or list.size() != cells:
			return _fail("layer '%s': 'cells' does not hold %d numbers" % [layer_name, cells])
		out.ids.resize(cells)
		for i in cells:
			var v: Variant = list[i]
			if not _is_whole(v) or v < 0:
				return _fail("layer '%s': cell %d is not a whole number" % [layer_name, i])
			out.ids[i] = int(v)
	else:
		return _fail("layer '%s': encoding '%s' is unknown (a newer format?)" % [layer_name, encoding])

	if kind == "tiles":
		for i in cells:
			if out.ids[i] >= tile_count:
				return _fail("layer '%s': tile id %d, but the map names %d tiles" % [layer_name, out.ids[i], tile_count])
	return out


static func _object(item: Variant) -> MapObject:
	if not (item is Dictionary):
		return _fail("an object is not a JSON object")
	var id: Variant = item.get("id")
	var type: Variant = item.get("type")
	var layer_name: Variant = item.get("layer")
	if not _is_whole(id) or not (type is String) or not (layer_name is String):
		return _fail("an object needs an id, a type and a layer")
	var box: Variant = _ints(item.get("bbox"), 6, "bbox of object %d" % int(id))
	if box == null:
		return null
	var props: Variant = item.get("props", {})
	if not (props is Dictionary):
		return _fail("object %d: 'props' is not an object" % int(id))
	var o := MapObject.new()
	o.id = int(id)
	o.type = type
	o.layer = layer_name
	o.position = Vector3i(box[0], box[1], box[2])
	o.size = Vector3i(box[3], box[4], box[5])
	for k: Variant in props:
		var p: Variant = props[k]
		if not (p is Dictionary) or not (p.get("type") is String):
			return _fail("object %d: prop '%s' is not {type, value}" % [o.id, k])
		var ok := [true]
		var value: Variant = _prop(p, "object %d: prop '%s'" % [o.id, k], ok)
		if not ok[0]:
			return null
		o.props[k] = value
		o.prop_types[k] = p["type"]
	return o


## The value of a {type, value} prop; on a bad one, ok[0] is false.
static func _prop(p: Dictionary, what: String, ok: Array) -> Variant:
	var type: Variant = p.get("type")
	var v: Variant = p.get("value")
	match type:
		"int", "object":
			if _is_whole(v):
				return int(v)
		"float":
			if v is float or v is int:
				return float(v)
		"bool":
			if v is bool:
				return v
		"string":
			if v is String:
				return v
		"cell":
			var c: Variant = _ints(v, 3, what)
			if c != null:
				return Vector3i(c[0], c[1], c[2])
			ok[0] = false
			return null
		"list":
			if v is Array:
				var items := []
				for item: Variant in v:
					if not (item is Dictionary) or not (item.get("type") is String):
						break
					items.append(_prop(item, what, ok))
					if not ok[0]:
						return null
				if items.size() == v.size():
					return items
		_:
			ok[0] = false
			_fail("%s: type '%s' is unknown (a newer format?)" % [what, str(type)])
			return null
	ok[0] = false
	_fail("%s: its value is not a %s" % [what, str(type)])
	return null


## A list of [param n] whole numbers, or null.
static func _ints(v: Variant, n: int, what: String) -> Variant:
	if not (v is Array) or v.size() != n:
		return _fail("%s is not a list of %d numbers" % [what, n])
	var out := PackedInt32Array()
	for i in n:
		if not _is_whole(v[i]):
			return _fail("%s[%d] is not a whole number" % [what, i])
		out.append(int(v[i]))
	return out


## Godot's JSON reads every number as a float.
static func _is_whole(v: Variant) -> bool:
	if v is int:
		return true
	return v is float and is_finite(v) and v == floorf(v) and absf(v) <= 9007199254740992.0


static func _fail(message: String) -> Variant:
	last_error = message
	return null
