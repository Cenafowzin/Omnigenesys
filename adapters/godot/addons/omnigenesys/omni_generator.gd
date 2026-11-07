class_name OmniGenerator
extends Node
## Makes a map from a pipeline while the game runs and hands it to an
## OmniMapBuilder. The generation runs on its own thread: the game keeps
## drawing, and generate() returns when the map is built.
##
## Where the addon's native library does not exist (for now, everywhere but
## Windows x64) it builds the fallback map instead.

## How generate() with no seed picks one.
enum SeedMode {
	## A new map every time.
	RANDOM,
	## Always the seed below: to look at or test one map.
	FIXED,
	## The seed written in the pipeline file.
	FROM_FILE,
}

## After every map built.
signal built(map: OmniMap)

## The pipeline file (the .json saved by the web app).
@export var pipeline: JSON
## Where the map is built. Empty: only the built signal gets it.
@export var builder: OmniMapBuilder
@export var seed_mode := SeedMode.RANDOM
## The seed when Seed Mode is Fixed: a whole number from 0 to 2^64 - 1.
@export var fixed_seed := "0"
## Param overrides, name → value; a param left out keeps the file's value.
@export var parameters: Dictionary = {}
## Make a map when the scene starts.
@export var generate_on_start := true
## Built where the native library does not exist (an imported .omap).
@export var fallback_map: OmniMapFile

## The seed of the last map made, as text (it can pass Godot's int).
var last_seed := ""
## True while a map is being made.
var busy := false


func _ready() -> void:
	if generate_on_start:
		generate()


## True where maps can be made in the game; otherwise only .omap files.
static func can_generate() -> bool:
	return ClassDB.class_exists(&"OmniGeneration")


## Makes and builds a map, and returns it; null if it failed (logged).
## [param with_seed] is a whole number or its text; null follows Seed Mode.
## With await, the line after it runs with the map in the scene.
func generate(with_seed: Variant = null) -> OmniMap:
	if busy:
		push_error("%s: already making a map" % name)
		return null
	busy = true
	# One frame first, so a builder later in the scene is ready too.
	await get_tree().process_frame
	var map := await _make(with_seed)
	busy = false
	if map == null:
		return null
	last_seed = map.seed_text
	if builder != null and not builder.build(map):
		return null
	built.emit(map)
	return map


## Saves the map of the last seed as a .omap file (it is made again: the same
## seed makes the same map). To keep a seed worth keeping. True if saved.
func save_last_seed(path: String) -> bool:
	if last_seed.is_empty() or not can_generate():
		push_error("%s: no seed to save, or no native library here" % name)
		return false
	var generation: Variant = _start(last_seed)
	if generation == null:
		return false
	var text: String = generation.omap_text()
	if text.is_empty():
		push_error("%s: %s" % [name, generation.result().get("error", "the map could not be written")])
		return false
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		push_error("%s: cannot write %s: %s" % [name, path, error_string(FileAccess.get_open_error())])
		return false
	file.store_string(text)
	return true


func _make(with_seed: Variant) -> OmniMap:
	if not can_generate():
		if fallback_map == null:
			push_error("%s: this platform has no native library; give OmniGenerator a fallback map (.omap)" % name)
			return null
		return fallback_map.load_map()
	var generation: Variant = _start(_seed_to_use(with_seed))
	if generation == null:
		return null
	while not generation.is_done():
		await get_tree().process_frame
	var map := OmniMap.from_native(generation.result())
	if map == null:
		push_error("[Omnigenesys] %s: %s" % [pipeline.resource_path, OmniMap.last_error])
	return map


## Starts a generation. The class is looked up by name, not written here,
## so this script still loads where the native library does not exist.
func _start(seed_text: String) -> Variant:
	if pipeline == null:
		push_error("%s: OmniGenerator has no pipeline" % name)
		return null
	# The file itself, not the parsed JSON: Godot reads every number as a
	# float, which would change them. A JSON resource ships as written.
	var text := FileAccess.get_file_as_string(pipeline.resource_path)
	if text.is_empty():
		push_error("%s: cannot read %s: %s" % [name, pipeline.resource_path, error_string(FileAccess.get_open_error())])
		return null
	var params := "" if parameters.is_empty() else JSON.stringify(parameters)
	var generation: Variant = ClassDB.instantiate(&"OmniGeneration")
	generation.start(text, params, seed_text, false, "")
	return generation


func _seed_to_use(with_seed: Variant) -> String:
	if with_seed != null:
		return str(with_seed)
	match seed_mode:
		SeedMode.FIXED:
			return fixed_seed
		SeedMode.FROM_FILE:
			return ""
		_:
			# Any of the 2^64 seeds; a game's RNG state stays untouched.
			var bytes := Crypto.new().generate_random_bytes(8)
			return String.num_uint64(bytes.decode_s64(0))
