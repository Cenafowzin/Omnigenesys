package terrain

import (
	"fmt"
	"omnigenesys/core/pipeline"
)

type Fill struct {
	Layer string
	Tile  string
}

func (f *Fill) Execute(ctx *pipeline.Context) error {
	layer := ctx.Grid.GetLayer(f.Layer)
	if layer == nil {
		return fmt.Errorf("fill: layer %q not found", f.Layer)
	}

	for y := 0; y < ctx.Grid.Height; y++ {
		for x := 0; x < ctx.Grid.Width; x++ {
			layer.Cells[y][x].Type = f.Tile
		}
	}

	return nil
}
