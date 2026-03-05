package operators

import (
	"fmt"
	"procedural_framework/core/pipeline"
)

// PlaceRoom coloca um retângulo preenchido com Floor e borda com Wall num layer.
// Se X e Y forem -1, a posição é escolhida aleatoriamente.
type PlaceRoom struct {
	Layer  string
	X, Y   int
	Width  int
	Height int
	Floor  string
	Wall   string
}

func (p *PlaceRoom) Execute(ctx *pipeline.Context) error {
	layer := ctx.Grid.GetLayer(p.Layer)
	if layer == nil {
		return fmt.Errorf("place_room: layer %q not found", p.Layer)
	}

	x, y := p.X, p.Y

	if x < 0 || y < 0 {
		maxX := ctx.Grid.Width - p.Width
		maxY := ctx.Grid.Height - p.Height
		if maxX <= 0 || maxY <= 0 {
			return fmt.Errorf("place_room: room (%dx%d) does not fit in grid (%dx%d)",
				p.Width, p.Height, ctx.Grid.Width, ctx.Grid.Height)
		}
		x = ctx.RNG.Intn(maxX)
		y = ctx.RNG.Intn(maxY)
	}

	for dy := 0; dy < p.Height; dy++ {
		for dx := 0; dx < p.Width; dx++ {
			px, py := x+dx, y+dy
			if px < 0 || px >= ctx.Grid.Width || py < 0 || py >= ctx.Grid.Height {
				continue
			}

			isBorder := dx == 0 || dy == 0 || dx == p.Width-1 || dy == p.Height-1
			if isBorder {
				layer.Cells[py][px].Type = p.Wall
			} else {
				layer.Cells[py][px].Type = p.Floor
			}
		}
	}

	return nil
}
