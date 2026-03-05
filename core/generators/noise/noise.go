package noise

import "math"

// Perlin é um gerador de ruído de Perlin 2D determinístico via seed.
type Perlin struct {
	perm [512]int
}

func New(seed int64) *Perlin {
	p := &Perlin{}

	base := make([]int, 256)
	for i := range base {
		base[i] = i
	}

	// Fisher-Yates com LCG simples para não depender de rand.Perm
	s := uint64(seed)
	for i := 255; i > 0; i-- {
		s = s*6364136223846793005 + 1442695040888963407
		j := int(s>>33) % (i + 1)
		base[i], base[j] = base[j], base[i]
	}

	for i := 0; i < 256; i++ {
		p.perm[i] = base[i]
		p.perm[i+256] = base[i]
	}
	return p
}

// Sample retorna um valor de ruído de Perlin em [0, 1] para as coordenadas (x, y).
func (p *Perlin) Sample(x, y float64) float64 {
	xi := int(math.Floor(x)) & 255
	yi := int(math.Floor(y)) & 255

	xf := x - math.Floor(x)
	yf := y - math.Floor(y)

	u := fade(xf)
	v := fade(yf)

	aa := p.perm[p.perm[xi]+yi]
	ab := p.perm[p.perm[xi]+yi+1]
	ba := p.perm[p.perm[xi+1]+yi]
	bb := p.perm[p.perm[xi+1]+yi+1]

	x1 := lerp(grad(aa, xf, yf), grad(ba, xf-1, yf), u)
	x2 := lerp(grad(ab, xf, yf-1), grad(bb, xf-1, yf-1), u)

	return (lerp(x1, x2, v) + 1) / 2
}

func fade(t float64) float64 {
	return t * t * t * (t*(t*6-15) + 10)
}

func lerp(a, b, t float64) float64 {
	return a + t*(b-a)
}

func grad(hash int, x, y float64) float64 {
	switch hash & 3 {
	case 0:
		return x + y
	case 1:
		return -x + y
	case 2:
		return x - y
	default:
		return -x - y
	}
}
