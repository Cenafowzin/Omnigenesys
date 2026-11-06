@tool
class_name OmniObjectRule
extends Resource
## What one object type of the map becomes: a scene on the object's box.

## Where on the box the scene's origin goes.
enum Pivot {
	## The middle of the box.
	CENTER,
	## The box's top-left corner.
	CORNER,
	## The middle of the box's bottom edge: a sprite's feet.
	BOTTOM_CENTER,
}

## The object's type in the pipeline, exactly.
@export var type := ""
## More than one: an int prop picks (variant prop); an empty slot places
## nothing.
@export var scenes: Array[PackedScene] = []
@export var pivot := Pivot.CENTER
## An int prop of the object (the one a PickObjectVariant step writes)
## that picks the scene. Empty: the first.
@export var variant_prop := ""
