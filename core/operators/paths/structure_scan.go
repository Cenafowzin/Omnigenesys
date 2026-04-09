package paths

import (
	"omnigenesys/core/pipeline"
	"sort"
)

type structBounds struct {
	minX, minY, maxX, maxY int
}

func (b *structBounds) entryPoint(from Point, clearance, gridW, gridH int) Point {
	cx := (b.minX + b.maxX) / 2
	cy := (b.minY + b.maxY) / 2

	dx := from.X - cx
	dy := from.Y - cy

	var ex, ey int
	if abs(dx) >= abs(dy) {
		ey = cy
		if dx < 0 {
			ex = b.minX - clearance
		} else {
			ex = b.maxX + clearance
		}
	} else {
		ex = cx
		if dy < 0 {
			ey = b.minY - clearance
		} else {
			ey = b.maxY + clearance
		}
	}

	if ex < 0 {
		ex = 0
	}
	if ex >= gridW {
		ex = gridW - 1
	}
	if ey < 0 {
		ey = 0
	}
	if ey >= gridH {
		ey = gridH - 1
	}

	return Point{ex, ey}
}

func scanStructureBounds(ctx *pipeline.Context, layerName string) map[string]*structBounds {
	layer := ctx.Grid.GetLayer(layerName)
	if layer == nil {
		return nil
	}

	bounds := map[string]*structBounds{}
	for y := 0; y < ctx.Grid.Height; y++ {
		for x := 0; x < ctx.Grid.Width; x++ {
			t := layer.Cells[y][x].Type
			if t == "" {
				continue
			}
			b := bounds[t]
			if b == nil {
				bounds[t] = &structBounds{x, y, x, y}
			} else {
				if x < b.minX {
					b.minX = x
				}
				if x > b.maxX {
					b.maxX = x
				}
				if y < b.minY {
					b.minY = y
				}
				if y > b.maxY {
					b.maxY = y
				}
			}
		}
	}
	return bounds
}

func entryPointsFrom(ctx *pipeline.Context, layerName string, from Point, clearance int) []Point {
	bounds := scanStructureBounds(ctx, layerName)
	if bounds == nil {
		return nil
	}
	keys := make([]string, 0, len(bounds))
	for k := range bounds {
		keys = append(keys, k)
	}
	sort.Strings(keys)

	points := make([]Point, 0, len(bounds))
	for _, k := range keys {
		points = append(points, bounds[k].entryPoint(from, clearance, ctx.Grid.Width, ctx.Grid.Height))
	}
	return points
}

func abs(v int) int {
	if v < 0 {
		return -v
	}
	return v
}

func randomPoint(ctx *pipeline.Context) Point {
	return Point{
		ctx.RNG.Intn(ctx.Grid.Width),
		ctx.RNG.Intn(ctx.Grid.Height),
	}
}
