package paths

import (
	"fmt"
	"omnigenesys/core/generators/pathfinding"
	"omnigenesys/core/pipeline"
)

// BranchPaths escolhe N células de SourceTile no SourceLayer como origens e
// conecta cada uma a um destino via A*, criando ramificações da rede de caminhos.
// Isso quebra o padrão "star" onde todos os caminhos saem do mesmo ponto,
// criando uma rede mais orgânica e distribuída.
type BranchPaths struct {
	SourceLayer     string
	SourceTile      string
	Layer           string
	Tile            string
	Branches        int
	StructuresLayer string
	Clearance       int
	NoiseFactor     float64
	NoiseScale      float64
	Conditions      []pipeline.Condition
}

func (b *BranchPaths) Execute(ctx *pipeline.Context) error {
	sourceLayer := ctx.Grid.GetLayer(b.SourceLayer)
	if sourceLayer == nil {
		return fmt.Errorf("branch_paths: source layer %q not found", b.SourceLayer)
	}
	layer := ctx.Grid.GetLayer(b.Layer)
	if layer == nil {
		return fmt.Errorf("branch_paths: layer %q not found", b.Layer)
	}

	origins := []Point{}
	for y := 0; y < ctx.Grid.Height; y++ {
		for x := 0; x < ctx.Grid.Width; x++ {
			if sourceLayer.Cells[y][x].Type == b.SourceTile {
				origins = append(origins, Point{x, y})
			}
		}
	}
	if len(origins) == 0 {
		return fmt.Errorf("branch_paths: no %q cells found in layer %q", b.SourceTile, b.SourceLayer)
	}

	clearance := b.Clearance
	if clearance < 1 {
		clearance = 1
	}

	scale := b.NoiseScale
	if scale <= 0 {
		scale = 0.12
	}

	for i := 0; i < b.Branches; i++ {
		origin := origins[ctx.RNG.Intn(len(origins))]

		var target Point
		if b.StructuresLayer != "" {
			structTargets := entryPointsFrom(ctx, b.StructuresLayer, origin, clearance)
			if len(structTargets) > 0 {
				target = structTargets[ctx.RNG.Intn(len(structTargets))]
			} else {
				target = randomPoint(ctx)
			}
		} else {
			target = randomPoint(ctx)
		}

		seed := int64(ctx.RNG.Int63())
		path := pathfinding.FindPath(ctx, origin.X, origin.Y, target.X, target.Y, b.NoiseFactor, scale, seed, b.Conditions)
		if path == nil {
			continue
		}
		for _, pt := range path {
			layer.Cells[pt.Y][pt.X].Type = b.Tile
		}
	}

	return nil
}
