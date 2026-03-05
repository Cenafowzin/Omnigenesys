package operators

import (
	"fmt"
	"procedural_framework/core/pipeline"
)

// PlaceSpawn coloca o tile de spawn no centro inferior do mapa.
// OffsetY permite ajustar quantas células acima da borda inferior o spawn aparece.
type PlaceSpawn struct {
	Layer   string
	Tile    string
	OffsetY int
}

func (p *PlaceSpawn) Execute(ctx *pipeline.Context) error {
	layer := ctx.Grid.GetLayer(p.Layer)
	if layer == nil {
		return fmt.Errorf("place_spawn: layer %q not found", p.Layer)
	}

	x := ctx.Grid.Width / 2
	y := ctx.Grid.Height - 1 - p.OffsetY

	layer.Cells[y][x].Type = p.Tile
	return nil
}
