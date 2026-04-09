package pipeline

import (
	"math/rand"
	"omnigenesys/core/grid"
)

type Context struct {
	Grid *grid.Grid2D
	RNG  *rand.Rand
}

func NewContext(grid *grid.Grid2D, seed int64) *Context {
	grid.Seed = seed
	return &Context{
		Grid: grid,
		RNG:  rand.New(rand.NewSource(seed)),
	}
}
