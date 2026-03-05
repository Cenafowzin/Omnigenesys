package paths

import (
	"fmt"
	"procedural_framework/core/generators/pathfinding"
	"procedural_framework/core/pipeline"
)

// PathConnect conecta dois pontos explícitos usando A* com ruído no custo de travessia.
// NoiseFactor controla o quanto o caminho desvia da linha reta:
//
//	0.0 = caminho mais curto puro
//	2-5 = desvios orgânicos (recomendado para estradas de terra)
//	>8  = desvios muito pronunciados
type PathConnect struct {
	Layer       string
	Tile        string
	From        Point
	To          Point
	NoiseFactor float64
	NoiseScale  float64
	Conditions  []pipeline.Condition
}

func (p *PathConnect) Execute(ctx *pipeline.Context) error {
	layer := ctx.Grid.GetLayer(p.Layer)
	if layer == nil {
		return fmt.Errorf("path_connect: layer %q not found", p.Layer)
	}

	scale := p.NoiseScale
	if scale <= 0 {
		scale = 0.12
	}

	seed := int64(ctx.RNG.Int63())
	path := pathfinding.FindPath(ctx, p.From.X, p.From.Y, p.To.X, p.To.Y, p.NoiseFactor, scale, seed, p.Conditions)
	if path == nil {
		return fmt.Errorf("path_connect: no path found from (%d,%d) to (%d,%d)", p.From.X, p.From.Y, p.To.X, p.To.Y)
	}

	for _, pt := range path {
		layer.Cells[pt.Y][pt.X].Type = p.Tile
	}

	return nil
}
