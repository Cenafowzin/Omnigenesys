extends SceneTree
## The addon's tests, headless:
##
##   godot --headless --path adapters/godot --import
##   godot --headless --path adapters/godot -s tests/run_tests.gd
##
## (the first imports the project once, so Godot knows the addon's class
## names). Exits with 1 if a check fails.
##
## lost_fields_512.omap is the Lost Fields recipe at seed 512, written by
## the CLI: mapgen generate crates/omnigenesys-core/tests/pipelines/lost_fields.json
## --seed 512 -o adapters/godot/tests/lost_fields_512.omap

const FIXTURE := "res://tests/lost_fields_512.omap"

var _failed := 0
var _passed := 0


func _initialize() -> void:
	var text := FileAccess.get_file_as_string(FIXTURE)
	_check(not text.is_empty(), "the fixture is there")
	_reads_the_fixture(text)
	_ignores_unknown_fields(text)
	_refuses_what_it_cannot_read(text)
	_props_keep_their_types()
	_imports_as_a_map_file()
	await _builds_tiles_scenes_and_objects(text)
	await _refuses_a_missing_variant(text)
	print("%d passed, %d failed" % [_passed, _failed])
	quit(1 if _failed > 0 else 0)


func _check(ok: bool, what: String) -> void:
	if ok:
		_passed += 1
	else:
		_failed += 1
		printerr("FAILED: " + what)


func _reads_the_fixture(text: String) -> void:
	var m := OmniMap.from_omap(text)
	_check(m != null, "reads the fixture: " + OmniMap.last_error)
	if m == null:
		return
	_check(m.width == 80 and m.height == 80 and m.depth == 1, "size 80 × 80 × 1")
	_check(m.origin == Vector3i.ZERO, "origin")
	_check(m.seed_text == "512", "seed")
	_check(m.tiles[0] == "empty" and m.tiles.has("mato_alto"), "tile names, 0 = empty")
	var terreno := m.layer("terreno")
	_check(terreno != null and terreno.kind == "tiles" and terreno.ids.size() == 6400, "rle tile layer, every cell")
	var max_id := 0
	for id in terreno.ids:
		max_id = maxi(max_id, id)
	_check(max_id > 0 and max_id < m.tiles.size(), "tile ids name a tile")
	var mato := m.layer("mato")
	_check(mato != null and mato.kind == "scalar" and mato.values.size() == 6400, "scalar layer")
	var variants := m.layer("variante_mato")
	_check(variants != null and variants.kind == "index", "index layer")
	var vegetacao := m.layer("vegetacao")
	var mato_alto := m.tiles.find("mato_alto")
	var seen := {}
	for i in 6400:
		if vegetacao.ids[i] == mato_alto:
			seen[variants.ids[i]] = true
	_check(seen.size() == 7, "the grass draws its 7 variants (%d)" % seen.size())
	_check(m.objects.size() == 28, "28 objects (%d)" % m.objects.size())
	var posts := 0
	for o in m.objects:
		if o.type == "poste_luz":
			posts += 1
			_check(o.prop_types.get("variante") == "int" and o.prop("variante") >= 0 and o.prop("variante") < 4,
					"post %d has a variant from 0 to 3" % o.id)
			_check(o.size == Vector3i(1, 1, 1), "a post is one cell")
	_check(posts == 10, "10 posts")
	_check(m.object_by_id(m.objects[0].id) == m.objects[0], "object by id")


func _ignores_unknown_fields(text: String) -> void:
	var data: Dictionary = JSON.parse_string(text)
	data["from_the_future"] = {"x": 1}
	data["layers"][0]["also_new"] = true
	data["objects"][0]["new_too"] = [1, 2]
	var m := OmniMap.from_omap(JSON.stringify(data))
	_check(m != null, "unknown fields are ignored: " + OmniMap.last_error)


func _refuses_what_it_cannot_read(text: String) -> void:
	var cases := [
		["not json {", "JSON line"],
		["[1, 2]", "a JSON object"],
	]
	for c in cases:
		_check(OmniMap.from_omap(c[0]) == null and OmniMap.last_error.contains(c[1]),
				"refuses %s: %s" % [c[0], OmniMap.last_error])
	# [what, path to the field, its new value, what the error says]
	var edits := [
		["version 3", ["version"], 3, "version 3"],
		["another format", ["format"], "tiled", "not an Omnigenesys map"],
		["an unknown kind", ["layers", 0, "kind"], "voxels", "kind 'voxels' is unknown"],
		["an unknown encoding", ["layers", 0, "encoding"], "zstd", "encoding 'zstd' is unknown"],
		["short runs", ["layers", 0, "runs"], [1, 10], "the runs hold 10 cells"],
		["odd runs", ["layers", 0, "runs"], [1, 6400, 2], "pairs"],
		["a tile id past the names", ["layers", 0, "runs"], [9, 6400], "tile id 9"],
		["a negative seed", ["seed"], "-1", "seed"],
		["an unknown prop type", ["objects", 0, "props"], {"p": {"type": "color", "value": 1}}, "type 'color' is unknown"],
		["a bad bbox", ["objects", 0, "bbox"], [0, 0, 0, 1], "bbox"],
	]
	for e in edits:
		var data: Variant = JSON.parse_string(text)
		var at: Variant = data
		var path: Array = e[1]
		for k in path.slice(0, path.size() - 1):
			at = at[k]
		at[path[path.size() - 1]] = e[2]
		var m := OmniMap.from_omap(JSON.stringify(data))
		_check(m == null and OmniMap.last_error.contains(e[3]), "refuses %s: '%s'" % [e[0], OmniMap.last_error])


func _props_keep_their_types() -> void:
	var text := JSON.stringify({
		"format": "omnigenesys-map", "version": 2, "minor": 0, "seed": "18446744073709551615",
		"origin": [-3, 4, 0], "size": [2, 1, 1], "tiles": ["empty"],
		"layers": [{"name": "i", "kind": "index", "encoding": "dense", "cells": [0, 5]}],
		"objects": [{"id": 7, "type": "t", "layer": "l", "bbox": [-3, 4, 0, 2, 1, 1], "props": {
			"n": {"type": "int", "value": -2},
			"f": {"type": "float", "value": 1.5},
			"b": {"type": "bool", "value": true},
			"s": {"type": "string", "value": "oi"},
			"c": {"type": "cell", "value": [1, 2, 0]},
			"o": {"type": "object", "value": 7},
			"l": {"type": "list", "value": [{"type": "int", "value": 1}, {"type": "string", "value": "x"}]},
		}}],
	})
	var m := OmniMap.from_omap(text)
	_check(m != null, "reads typed props: " + OmniMap.last_error)
	if m == null:
		return
	_check(m.seed_text == "18446744073709551615", "a seed past Godot's int stays a string")
	_check(m.origin == Vector3i(-3, 4, 0), "a negative origin")
	var o := m.objects[0]
	_check(typeof(o.prop("n")) == TYPE_INT and o.prop("n") == -2, "int")
	_check(typeof(o.prop("f")) == TYPE_FLOAT and o.prop("f") == 1.5, "float")
	_check(o.prop("b") == true and o.prop("s") == "oi", "bool, string")
	_check(o.prop("c") == Vector3i(1, 2, 0), "cell")
	_check(o.prop("o") == 7 and o.prop_types["o"] == "object", "object reference")
	_check(o.prop("l") == [1, "x"], "list")
	_check(o.prop("missing", 3) == 3, "a missing prop gives the fallback")
	_check(m.layer("i").ids == PackedInt64Array([0, 5]), "dense index layer")


func _imports_as_a_map_file() -> void:
	if not ResourceLoader.exists(FIXTURE):
		printerr("note: the fixture was not imported (run with --import first); import not checked")
		return
	var res := load(FIXTURE)
	_check(res is OmniMapFile, "a .omap imports as an OmniMapFile")
	if res is OmniMapFile:
		_check((res as OmniMapFile).load_map() != null, "the imported file reads")


## A TileSet of four 16 × 16 tiles in one atlas.
func _tile_set() -> TileSet:
	var image := Image.create(64, 16, false, Image.FORMAT_RGBA8)
	image.fill(Color.WHITE)
	var atlas := TileSetAtlasSource.new()
	atlas.texture = ImageTexture.create_from_image(image)
	atlas.texture_region_size = Vector2i(16, 16)
	for x in 4:
		atlas.create_tile(Vector2i(x, 0))
	var tiles := TileSet.new()
	tiles.tile_size = Vector2i(16, 16)
	tiles.add_source(atlas, 0)
	return tiles


## A scene: a Node2D with meta "variant" = [param i], and a script that
## counts the _omni_spawned calls in meta "hooked".
func _scene(i: int, hook: bool = false) -> PackedScene:
	var n := Node2D.new()
	n.set_meta("variant", i)
	if hook:
		var script := GDScript.new()
		script.source_code = "extends Node2D\nfunc _omni_spawned(o, m, spawned):\n\tset_meta(\"hooked\", [o.id, spawned.size()])\n"
		script.reload()
		n.set_script(script)
	var packed := PackedScene.new()
	packed.pack(n)
	n.free()
	return packed


func _mapping(grass: int) -> OmniMapping:
	var mapping := OmniMapping.new()
	for t in [["floor", 0], ["mato_enraizado", 1], ["path", 2]]:
		var r := OmniTileRule.new()
		r.tile = t[0]
		r.atlas_coords = [Vector2i(t[1], 0)]
		mapping.tiles.append(r)
	var g := OmniTileRule.new()
	g.tile = "mato_alto"
	for i in grass:
		g.scenes.append(_scene(i))
	g.variant_layer = "variante_mato"
	mapping.tiles.append(g)
	var post := OmniObjectRule.new()
	post.type = "poste_luz"
	for i in 4:
		post.scenes.append(_scene(i, true))
	post.pivot = OmniObjectRule.Pivot.BOTTOM_CENTER
	post.variant_prop = "variante"
	mapping.objects.append(post)
	var spawn := OmniObjectRule.new()
	spawn.type = "spawn"
	spawn.scenes = [_scene(0)]
	spawn.pivot = OmniObjectRule.Pivot.CORNER
	mapping.objects.append(spawn)
	return mapping


func _builder(grass: int) -> OmniMapBuilder:
	var b := OmniMapBuilder.new()
	b.build_on_ready = false
	b.mapping = _mapping(grass)
	b.position = Vector2(100, 50)
	var tiles := _tile_set()
	for layer_name in ["terreno", "vegetacao"]:
		var t := TileMapLayer.new()
		t.name = layer_name
		t.tile_set = tiles
		b.add_child(t)
	root.add_child(b)
	return b


func _live(parent: Node, meta: String) -> Array:
	var out := []
	for c in parent.get_children():
		if c.has_meta(meta) and not c.is_queued_for_deletion():
			out.append(c)
	return out


func _builds_tiles_scenes_and_objects(text: String) -> void:
	var m := OmniMap.from_omap(text)
	var b := _builder(7)
	var got := [null]
	b.built.connect(func(map: OmniMap) -> void: got[0] = map)
	_check(b.build(m), "builds")
	_check(got[0] == m, "emits built")
	var terreno: TileMapLayer = b.get_node("terreno")
	var layer := m.layer("terreno")
	var path := m.tiles.find("path")
	var painted := 0
	var right := true
	for y in m.height:
		for x in m.width:
			var id: int = layer.ids[m.cell_index(x, y)]
			if id == 0:
				continue
			painted += 1
			var want := Vector2i(2, 0) if id == path else Vector2i(1 if m.tiles[id] == "mato_enraizado" else 0, 0)
			right = right and terreno.get_cell_atlas_coords(Vector2i(x, y)) == want
	_check(painted > 0 and terreno.get_used_cells().size() == painted, "every terrain cell painted (%d)" % painted)
	_check(right, "each tile name with its atlas tile, at the same cell")

	var vegetacao := m.layer("vegetacao")
	var variants := m.layer("variante_mato")
	var mato_alto := m.tiles.find("mato_alto")
	var grass := {}
	for i in vegetacao.ids.size():
		if vegetacao.ids[i] == mato_alto:
			grass[Vector2(i % m.width + 0.5, i / m.width + 0.5) * 16.0] = variants.ids[i]
	var objects := {}
	for o in m.objects:
		if o.type == "poste_luz" or o.type == "spawn":
			objects[o.id] = o
	var scenes := _live(b, "variant")
	_check(scenes.size() == grass.size() + objects.size(),
			"one scene per grass cell and mapped object (%d, %d + %d)" % [scenes.size(), grass.size(), objects.size()])
	var grass_right := true
	for n: Node2D in scenes:
		if n.has_meta("omni_object"):
			continue
		var at := n.position
		grass_right = grass_right and grass.has(at) and grass[at] == n.get_meta("variant")
	_check(grass_right, "each grass scene at its cell's centre, the variant its cell drew")

	for n: Node2D in scenes:
		if not n.has_meta("omni_object"):
			continue
		var o: OmniMap.MapObject = n.get_meta("omni_object")
		if o.type == "poste_luz":
			_check(n.get_meta("variant") == o.prop("variante"), "post %d: the scene its prop picks" % o.id)
			_check(n.position == Vector2(o.position.x + 0.5, o.position.y + 1) * 16.0, "post %d: bottom centre" % o.id)
			_check(n.get_meta("hooked", []) == [o.id, objects.size()], "post %d: _omni_spawned with every object" % o.id)
		else:
			_check(n.position == Vector2(o.position.x, o.position.y) * 16.0, "spawn: corner")
			_check(n.global_position == b.global_position + n.position, "scenes in the builder's space")

	_check(b.build(m), "builds again")
	await process_frame
	_check(_live(b, "variant").size() == scenes.size(), "a new build replaces the last one")
	_check(terreno.get_used_cells().size() == painted, "the tiles too")
	b.clear()
	await process_frame
	_check(_live(b, "variant").is_empty() and terreno.get_used_cells().is_empty(), "clear leaves nothing")
	b.queue_free()


func _refuses_a_missing_variant(text: String) -> void:
	var m := OmniMap.from_omap(text)
	var b := _builder(3)
	print("(an error about variant 3 or above is expected next)")
	_check(not b.build(m), "3 grass scenes for 7 variants is refused")
	b.queue_free()
	await process_frame
