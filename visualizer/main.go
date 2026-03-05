package main

import (
	"encoding/json"
	"image"
	"image/color"
	"image/png"
	"log"
	"os"
)

const cellSize = 16

var palette = map[string]color.RGBA{
	// base
	"floor": {R: 180, G: 160, B: 120, A: 255}, // areia/terra
	"wall":  {R: 70, G: 70, B: 90, A: 255},    // cinza escuro
	// milharal
	"mato_enraizado": {R: 30, G: 60, B: 140, A: 255},  // azul escuro — borda
	"mato_alto":      {R: 190, G: 170, B: 40, A: 255},  // amarelo esverdeado
	"path":           {R: 210, G: 190, B: 150, A: 255}, // areia clara
	// entidades
	"spawn": {R: 50, G: 220, B: 80, A: 255}, // verde brilhante
	// estruturas
	"estrutura_loja":    {R: 230, G: 120, B: 20, A: 255},  // laranja
	"estrutura_desafio": {R: 200, G: 60, B: 60, A: 255},   // vermelho
	"estrutura_arena":   {R: 180, G: 80, B: 200, A: 255},  // roxo
	"estrutura_item":    {R: 60, G: 180, B: 220, A: 255},  // ciano
	// misc
	"water": {R: 80, G: 140, B: 200, A: 255}, // azul
	"door":  {R: 180, G: 100, B: 40, A: 255}, // marrom
	"":      {R: 20, G: 20, B: 20, A: 255},   // vazio = preto
}

type cell struct {
	Type string `json:"type"`
}

type layer struct {
	Name  string   `json:"name"`
	Cells [][]cell `json:"cells"`
}

type mapData struct {
	Width  int     `json:"width"`
	Height int     `json:"height"`
	Layers []layer `json:"layers"`
}

func tileColor(t string) color.RGBA {
	if c, ok := palette[t]; ok {
		return c
	}
	return color.RGBA{R: 255, G: 0, B: 200, A: 255} // magenta = tipo desconhecido
}

func main() {
	input := "map.json"
	output := "map.png"
	if len(os.Args) > 1 {
		input = os.Args[1]
	}
	if len(os.Args) > 2 {
		output = os.Args[2]
	}

	data, err := os.ReadFile(input)
	if err != nil {
		log.Fatalf("failed to read %s: %v", input, err)
	}

	var m mapData
	if err := json.Unmarshal(data, &m); err != nil {
		log.Fatalf("failed to parse json: %v", err)
	}

	if len(m.Layers) == 0 {
		log.Fatal("no layers found in map")
	}

	// Renderiza todas as layers em ordem (última por cima)
	img := image.NewRGBA(image.Rect(0, 0, m.Width*cellSize, m.Height*cellSize))

	// Fundo preto
	for y := 0; y < m.Height*cellSize; y++ {
		for x := 0; x < m.Width*cellSize; x++ {
			img.SetRGBA(x, y, color.RGBA{R: 20, G: 20, B: 20, A: 255})
		}
	}

	for _, l := range m.Layers {
		for y, row := range l.Cells {
			for x, c := range row {
				if c.Type == "" {
					continue
				}
				col := tileColor(c.Type)
				drawCell(img, x, y, col)
			}
		}
	}

	f, err := os.Create(output)
	if err != nil {
		log.Fatalf("failed to create %s: %v", output, err)
	}
	defer f.Close()

	if err := png.Encode(f, img); err != nil {
		log.Fatalf("failed to encode png: %v", err)
	}

	log.Printf("saved %s (%dx%d cells)", output, m.Width, m.Height)
}

func drawCell(img *image.RGBA, cellX, cellY int, c color.RGBA) {
	px := cellX * cellSize
	py := cellY * cellSize

	for dy := 0; dy < cellSize; dy++ {
		for dx := 0; dx < cellSize; dx++ {
			// Borda de 1px mais escura para separar as células visualmente
			if dx == 0 || dy == 0 {
				img.SetRGBA(px+dx, py+dy, darken(c, 0.6))
			} else {
				img.SetRGBA(px+dx, py+dy, c)
			}
		}
	}
}

func darken(c color.RGBA, factor float64) color.RGBA {
	return color.RGBA{
		R: uint8(float64(c.R) * factor),
		G: uint8(float64(c.G) * factor),
		B: uint8(float64(c.B) * factor),
		A: c.A,
	}
}
