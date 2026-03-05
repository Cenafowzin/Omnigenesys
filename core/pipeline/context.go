package pipeline

import (
	"math/rand"
	"procedural_framework/core/grid"
)

type Context struct {
	Grid *grid.Grid2D
	RNG  *rand.Rand
}

func NewContext(grid *grid.Grid2D, seed int64) *Context {
	source := rand.NewSource(seed)

	return &Context{
		Grid: grid,
		RNG:  rand.New(source),
	}
}
