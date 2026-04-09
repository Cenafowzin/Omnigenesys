package export

import (
	"encoding/json"
	"io"
	"os"
	"omnigenesys/core/grid"
)

type exportedCell struct {
	Type     string         `json:"type"`
	Metadata map[string]any `json:"metadata,omitempty"`
}

type exportedLayer struct {
	Name  string           `json:"name"`
	Cells [][]exportedCell `json:"cells"`
}

type exportedGrid struct {
	Width  int             `json:"width"`
	Height int             `json:"height"`
	Seed   int64           `json:"seed"`
	Layers []exportedLayer `json:"layers"`
}

func buildExportedGrid(g *grid.Grid2D) exportedGrid {
	layers := make([]exportedLayer, 0, len(g.Layers))
	for _, layer := range g.OrderedLayers() {
		cells := make([][]exportedCell, len(layer.Cells))
		for y, row := range layer.Cells {
			cells[y] = make([]exportedCell, len(row))
			for x, cell := range row {
				cells[y][x] = exportedCell{
					Type:     cell.Type,
					Metadata: cell.Metadata,
				}
			}
		}
		layers = append(layers, exportedLayer{Name: layer.Name, Cells: cells})
	}
	return exportedGrid{Width: g.Width, Height: g.Height, Seed: g.Seed, Layers: layers}
}

// ToJSON serializa o grid para um arquivo no caminho especificado.
func ToJSON(g *grid.Grid2D, path string) error {
	data, err := json.MarshalIndent(buildExportedGrid(g), "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(path, data, 0644)
}

// ToWriter serializa o grid e escreve no writer fornecido (ex: os.Stdout).
func ToWriter(g *grid.Grid2D, w io.Writer) error {
	data, err := json.Marshal(buildExportedGrid(g))
	if err != nil {
		return err
	}
	_, err = w.Write(data)
	return err
}
