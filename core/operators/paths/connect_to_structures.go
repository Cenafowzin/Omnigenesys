package paths

import (
	"fmt"
	"omnigenesys/core/generators/pathfinding"
	"omnigenesys/core/pipeline"
)

// ConnectToStructures varre o StructuresLayer, calcula o ponto de entrada de cada
// estrutura (célula fora da borda, no lado que enfrenta From) e conecta via A*.
// Clearance define quantas células o ponto de entrada fica fora da borda da estrutura.
// Combine com LayerEmpty{Layer:"structures"} nas Conditions para impedir que o
// caminho atravesse outras estruturas.
// Para caminhos aleatórios sem destino, use BranchPaths com StructuresLayer vazio.
type ConnectToStructures struct {
	Layer           string
	Tile            string
	From            Point
	StructuresLayer string
	Clearance       int
	NoiseFactor     float64
	NoiseScale      float64
	Conditions      []pipeline.Condition
}

func (c *ConnectToStructures) Execute(ctx *pipeline.Context) error {
	layer := ctx.Grid.GetLayer(c.Layer)
	if layer == nil {
		return fmt.Errorf("connect_to_structures: layer %q not found", c.Layer)
	}

	clearance := c.Clearance
	if clearance < 1 {
		clearance = 1
	}

	targets := entryPointsFrom(ctx, c.StructuresLayer, c.From, clearance)

	scale := c.NoiseScale
	if scale <= 0 {
		scale = 0.12
	}

	for _, target := range targets {
		seed := int64(ctx.RNG.Int63())
		path := pathfinding.FindPath(ctx, c.From.X, c.From.Y, target.X, target.Y, c.NoiseFactor, scale, seed, c.Conditions)
		if path == nil {
			continue
		}
		for _, pt := range path {
			layer.Cells[pt.Y][pt.X].Type = c.Tile
		}
	}

	return nil
}
