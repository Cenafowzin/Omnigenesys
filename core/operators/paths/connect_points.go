package paths

import (
	"fmt"
	"omnigenesys/core/pipeline"
)

// ConnectPoints executa um random walk a partir de From.
// Se To não for nil, DirectChance define a probabilidade de cada passo
// se mover em direção ao destino (0.0 = totalmente aleatório, 1.0 = linha reta).
// Se To for nil, o walker anda aleatoriamente por MaxSteps passos.
// Diagonal habilita movimento em 8 direções, deixando os caminhos mais orgânicos.
// Conditions controlam onde o walker pode entrar E onde o tile é escrito.
type ConnectPoints struct {
	Layer        string
	Tile         string
	From         Point
	To           *Point
	DirectChance float64
	MaxSteps     int
	Diagonal     bool
	Conditions   []pipeline.Condition
}

var dirs4 = []Point{{0, -1}, {0, 1}, {-1, 0}, {1, 0}}
var dirs8 = []Point{{0, -1}, {0, 1}, {-1, 0}, {1, 0}, {-1, -1}, {1, -1}, {-1, 1}, {1, 1}}

func (c *ConnectPoints) Execute(ctx *pipeline.Context) error {
	layer := ctx.Grid.GetLayer(c.Layer)
	if layer == nil {
		return fmt.Errorf("connect_points: layer %q not found", c.Layer)
	}

	x, y := c.From.X, c.From.Y
	maxSteps := c.MaxSteps
	if maxSteps <= 0 {
		maxSteps = ctx.Grid.Width * ctx.Grid.Height
	}

	for step := 0; step < maxSteps; step++ {
		layer.Cells[y][x].Type = c.Tile

		if c.To != nil && x == c.To.X && y == c.To.Y {
			break
		}

		next, ok := c.nextStep(ctx, x, y)
		if !ok {
			break
		}
		x, y = next.X, next.Y
	}

	return nil
}

func (c *ConnectPoints) nextStep(ctx *pipeline.Context, x, y int) (Point, bool) {
	if c.To != nil && ctx.RNG.Float64() < c.DirectChance {
		dx, dy := sign(c.To.X-x), sign(c.To.Y-y)
		for _, d := range directionalCandidates(dx, dy) {
			nx, ny := x+d.X, y+d.Y
			if c.canEnter(ctx, nx, ny) {
				return Point{nx, ny}, true
			}
		}
	}

	available := dirs4
	if c.Diagonal {
		available = dirs8
	}
	for _, d := range shuffleDirs(ctx, available) {
		nx, ny := x+d.X, y+d.Y
		if c.canEnter(ctx, nx, ny) {
			return Point{nx, ny}, true
		}
	}

	return Point{}, false
}

func (c *ConnectPoints) canEnter(ctx *pipeline.Context, x, y int) bool {
	if x < 0 || x >= ctx.Grid.Width || y < 0 || y >= ctx.Grid.Height {
		return false
	}
	return pipeline.CheckAll(c.Conditions, ctx, x, y)
}

func sign(v int) int {
	if v > 0 {
		return 1
	}
	if v < 0 {
		return -1
	}
	return 0
}

func directionalCandidates(dx, dy int) []Point {
	candidates := []Point{}
	if dx != 0 {
		candidates = append(candidates, Point{dx, 0})
	}
	if dy != 0 {
		candidates = append(candidates, Point{0, dy})
	}
	if dy != 0 {
		candidates = append(candidates, Point{1, 0}, Point{-1, 0})
	}
	if dx != 0 {
		candidates = append(candidates, Point{0, 1}, Point{0, -1})
	}
	return candidates
}

func shuffleDirs(ctx *pipeline.Context, d []Point) []Point {
	out := make([]Point, len(d))
	copy(out, d)
	ctx.RNG.Shuffle(len(out), func(i, j int) { out[i], out[j] = out[j], out[i] })
	return out
}
