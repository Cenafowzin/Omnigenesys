package pathfinding

import (
	"container/heap"
	"math"
	"procedural_framework/core/generators/noise"
	"procedural_framework/core/pipeline"
)

type Point struct{ X, Y int }

type key struct{ x, y int }

type node struct {
	x, y   int
	g, f   float64
	parent *node
}

type priorityQueue []*node

func (pq priorityQueue) Len() int           { return len(pq) }
func (pq priorityQueue) Less(i, j int) bool { return pq[i].f < pq[j].f }
func (pq priorityQueue) Swap(i, j int)      { pq[i], pq[j] = pq[j], pq[i] }
func (pq *priorityQueue) Push(x any)        { *pq = append(*pq, x.(*node)) }
func (pq *priorityQueue) Pop() any {
	old := *pq
	n := len(old)
	x := old[n-1]
	*pq = old[:n-1]
	return x
}

var dirs = []struct {
	dx, dy int
	cost   float64
}{
	{0, 1, 1}, {0, -1, 1}, {1, 0, 1}, {-1, 0, 1},
	{1, 1, 1.414}, {1, -1, 1.414}, {-1, 1, 1.414}, {-1, -1, 1.414},
}

// FindPath retorna o caminho de (x0,y0) a (x1,y1) usando A*.
// NoiseFactor > 0 adiciona resistência de Perlin noise ao custo de cada célula,
// fazendo o caminho desviar organicamente sem ser aleatório.
func FindPath(
	ctx *pipeline.Context,
	x0, y0, x1, y1 int,
	noiseFactor float64,
	noiseScale float64,
	noiseSeed int64,
	conditions []pipeline.Condition,
) []Point {
	w, h := ctx.Grid.Width, ctx.Grid.Height

	var gen *noise.Perlin
	if noiseFactor > 0 {
		gen = noise.New(noiseSeed)
	}

	cellCost := func(x, y int) float64 {
		if gen == nil {
			return 1.0
		}
		return 1.0 + gen.Sample(float64(x)*noiseScale, float64(y)*noiseScale)*noiseFactor
	}

	heuristic := func(x, y int) float64 {
		return math.Sqrt(float64((x-x1)*(x-x1) + (y-y1)*(y-y1)))
	}

	gScore := make(map[key]float64, w*h)
	parent := make(map[key]key, w*h)

	start := &node{x: x0, y: y0, g: 0, f: heuristic(x0, y0)}
	gScore[key{x0, y0}] = 0

	pq := &priorityQueue{start}
	heap.Init(pq)

	for pq.Len() > 0 {
		cur := heap.Pop(pq).(*node)

		if cur.x == x1 && cur.y == y1 {
			return reconstructPath(parent, key{x0, y0}, key{x1, y1})
		}

		curKey := key{cur.x, cur.y}
		if cur.g > gScore[curKey]+1e-9 {
			continue
		}

		for _, d := range dirs {
			nx, ny := cur.x+d.dx, cur.y+d.dy
			if nx < 0 || nx >= w || ny < 0 || ny >= h {
				continue
			}
			if !pipeline.CheckAll(conditions, ctx, nx, ny) {
				continue
			}

			ng := gScore[curKey] + d.cost*cellCost(nx, ny)
			nk := key{nx, ny}
			if prev, exists := gScore[nk]; !exists || ng < prev-1e-9 {
				gScore[nk] = ng
				parent[nk] = curKey
				heap.Push(pq, &node{x: nx, y: ny, g: ng, f: ng + heuristic(nx, ny)})
			}
		}
	}

	return nil
}

func reconstructPath(parent map[key]key, start, end key) []Point {
	path := []Point{}
	cur := end
	for cur != start {
		path = append([]Point{{cur.x, cur.y}}, path...)
		cur = parent[cur]
	}
	path = append([]Point{{start.x, start.y}}, path...)
	return path
}
