package terrain

import (
	"fmt"
	"omnigenesys/core/pipeline"
)

// FillBorder preenche as bordas do mapa com um tile até a espessura indicada.
type FillBorder struct {
	Layer     string
	Tile      string
	Thickness int
}

func (f *FillBorder) Execute(ctx *pipeline.Context) error {
	layer := ctx.Grid.GetLayer(f.Layer)
	if layer == nil {
		return fmt.Errorf("fill_border: layer %q not found", f.Layer)
	}

	t := f.Thickness
	w := ctx.Grid.Width
	h := ctx.Grid.Height

	for y := 0; y < h; y++ {
		for x := 0; x < w; x++ {
			if x < t || x >= w-t || y < t || y >= h-t {
				layer.Cells[y][x].Type = f.Tile
			}
		}
	}

	return nil
}
