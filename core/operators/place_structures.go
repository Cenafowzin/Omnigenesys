package operators

import (
	"fmt"
	"procedural_framework/core/pipeline"
)

type StructureDef struct {
	Type       string
	Width      int
	Height     int
	Conditions []pipeline.Condition // condições avaliadas no centro da estrutura antes de posicionar
}

type rect struct {
	X, Y, W, H int
}

func (r rect) intersects(other rect, margin int) bool {
	return r.X-margin < other.X+other.W &&
		r.X+r.W+margin > other.X &&
		r.Y-margin < other.Y+other.H &&
		r.Y+r.H+margin > other.Y
}

// PlaceStructures posiciona estruturas aleatoriamente no mapa garantindo que
// não se sobreponham (respeitando MinDistance entre elas) e que não caiam
// sobre o tile de borda indicado em AvoidType.
//
// Cada estrutura é marcada pelo seu Type no layer indicado.
// O conteúdo interno das estruturas é responsabilidade da engine (prefabs).
type PlaceStructures struct {
	Layer       string
	Structures  []StructureDef
	MinDistance int
	AvoidLayer  string
	AvoidType   string
	MaxAttempts int
}

func (p *PlaceStructures) Execute(ctx *pipeline.Context) error {
	layer := ctx.Grid.GetLayer(p.Layer)
	if layer == nil {
		return fmt.Errorf("place_structures: layer %q not found", p.Layer)
	}

	maxAttempts := p.MaxAttempts
	if maxAttempts <= 0 {
		maxAttempts = 100
	}

	placed := []rect{}

	for _, def := range p.Structures {
		r, ok := p.findPosition(ctx, def, placed, maxAttempts)
		if !ok {
			return fmt.Errorf("place_structures: could not place %q after %d attempts", def.Type, maxAttempts)
		}

		for dy := 0; dy < def.Height; dy++ {
			for dx := 0; dx < def.Width; dx++ {
				layer.Cells[r.Y+dy][r.X+dx].Type = def.Type
			}
		}

		placed = append(placed, r)
	}

	return nil
}

func (p *PlaceStructures) findPosition(ctx *pipeline.Context, def StructureDef, placed []rect, maxAttempts int) (rect, bool) {
	margin := p.MinDistance

	for attempt := 0; attempt < maxAttempts; attempt++ {
		maxX := ctx.Grid.Width - def.Width - margin
		maxY := ctx.Grid.Height - def.Height - margin
		if maxX <= margin || maxY <= margin {
			return rect{}, false
		}

		x := margin + ctx.RNG.Intn(maxX-margin)
		y := margin + ctx.RNG.Intn(maxY-margin)
		r := rect{x, y, def.Width, def.Height}

		if p.overlapsAny(r, placed, margin) {
			continue
		}

		if p.overlapsAvoid(ctx, r) {
			continue
		}

		// Condições avaliadas no centro da estrutura
		cx, cy := r.X+r.W/2, r.Y+r.H/2
		if !pipeline.CheckAll(def.Conditions, ctx, cx, cy) {
			continue
		}

		return r, true
	}

	return rect{}, false
}

func (p *PlaceStructures) overlapsAny(r rect, placed []rect, margin int) bool {
	for _, other := range placed {
		if r.intersects(other, margin) {
			return true
		}
	}
	return false
}

func (p *PlaceStructures) overlapsAvoid(ctx *pipeline.Context, r rect) bool {
	if p.AvoidLayer == "" || p.AvoidType == "" {
		return false
	}
	avoidLayer := ctx.Grid.GetLayer(p.AvoidLayer)
	if avoidLayer == nil {
		return false
	}
	for dy := 0; dy < r.H; dy++ {
		for dx := 0; dx < r.W; dx++ {
			if avoidLayer.Cells[r.Y+dy][r.X+dx].Type == p.AvoidType {
				return true
			}
		}
	}
	return false
}
