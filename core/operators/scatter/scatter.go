package scatter

import (
	"fmt"
	"procedural_framework/core/pipeline"
)

// Scatter preenche células aleatoriamente num layer com base numa chance (0.0 a 1.0).
// Conditions determinam quais células são elegíveis.
type Scatter struct {
	Layer      string
	Tile       string
	Chance     float64
	Conditions []pipeline.Condition
}

func (s *Scatter) Execute(ctx *pipeline.Context) error {
	layer := ctx.Grid.GetLayer(s.Layer)
	if layer == nil {
		return fmt.Errorf("scatter: layer %q not found", s.Layer)
	}

	for y := 0; y < ctx.Grid.Height; y++ {
		for x := 0; x < ctx.Grid.Width; x++ {
			if !pipeline.CheckAll(s.Conditions, ctx, x, y) {
				continue
			}
			if ctx.RNG.Float64() < s.Chance {
				layer.Cells[y][x].Type = s.Tile
			}
		}
	}

	return nil
}
