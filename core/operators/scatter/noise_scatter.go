package scatter

import (
	"fmt"
	"omnigenesys/core/generators/noise"
	"omnigenesys/core/pipeline"
)

// NoiseScatter espalha um tile usando Perlin noise.
// Células onde o valor do noise for >= Threshold recebem o tile.
// Scale controla o tamanho das manchas: valores menores = manchas maiores.
// Conditions determinam quais células são elegíveis.
type NoiseScatter struct {
	Layer      string
	Tile       string
	Threshold  float64
	Scale      float64
	Conditions []pipeline.Condition
}

func (n *NoiseScatter) Execute(ctx *pipeline.Context) error {
	layer := ctx.Grid.GetLayer(n.Layer)
	if layer == nil {
		return fmt.Errorf("noise_scatter: layer %q not found", n.Layer)
	}

	scale := n.Scale
	if scale <= 0 {
		scale = 0.1
	}

	seed := int64(ctx.RNG.Uint64())
	gen := noise.New(seed)

	for y := 0; y < ctx.Grid.Height; y++ {
		for x := 0; x < ctx.Grid.Width; x++ {
			if !pipeline.CheckAll(n.Conditions, ctx, x, y) {
				continue
			}
			val := gen.Sample(float64(x)*scale, float64(y)*scale)
			if val >= n.Threshold {
				layer.Cells[y][x].Type = n.Tile
			}
		}
	}

	return nil
}
