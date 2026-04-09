package placement

import (
	"fmt"
	"omnigenesys/core/pipeline"
)

// PlacePoint coloca um único tile numa posição calculada por âncoras.
//
// AnchorX: "center", "left", "right", "random" — ou vazio para usar X diretamente.
// AnchorY: "center", "top", "bottom", "random" — ou vazio para usar Y diretamente.
// OffsetX/OffsetY: deslocamento em células a partir da âncora.
// Conditions: verificadas na célula resolvida antes de colocar o tile.
//
// Exemplos:
//
//	Spawn (centro inferior):  AnchorX:"center" AnchorY:"bottom" OffsetY:-3
//	Exit (centro superior):   AnchorX:"center" AnchorY:"top"    OffsetY:1
//	Exit (aleatório na borda): AnchorX:"random" AnchorY:"top"   OffsetY:1
type PlacePoint struct {
	Layer      string
	Tile       string
	AnchorX    string
	AnchorY    string
	X, Y       int
	OffsetX    int
	OffsetY    int
	Conditions []pipeline.Condition
}

func (p *PlacePoint) Execute(ctx *pipeline.Context) error {
	layer := ctx.Grid.GetLayer(p.Layer)
	if layer == nil {
		return fmt.Errorf("place_point: layer %q not found", p.Layer)
	}

	x := p.resolveX(ctx)
	y := p.resolveY(ctx)

	x += p.OffsetX
	y += p.OffsetY

	if x < 0 || x >= ctx.Grid.Width || y < 0 || y >= ctx.Grid.Height {
		return fmt.Errorf("place_point: resolved position (%d, %d) is out of bounds", x, y)
	}

	if !pipeline.CheckAll(p.Conditions, ctx, x, y) {
		return fmt.Errorf("place_point: conditions not met at (%d, %d)", x, y)
	}

	layer.Cells[y][x].Type = p.Tile
	return nil
}

func (p *PlacePoint) resolveX(ctx *pipeline.Context) int {
	switch p.AnchorX {
	case "center":
		return ctx.Grid.Width / 2
	case "left":
		return 0
	case "right":
		return ctx.Grid.Width - 1
	case "random":
		return ctx.RNG.Intn(ctx.Grid.Width)
	default:
		return p.X
	}
}

func (p *PlacePoint) resolveY(ctx *pipeline.Context) int {
	switch p.AnchorY {
	case "center":
		return ctx.Grid.Height / 2
	case "top":
		return 0
	case "bottom":
		return ctx.Grid.Height - 1
	case "random":
		return ctx.RNG.Intn(ctx.Grid.Height)
	default:
		return p.Y
	}
}
