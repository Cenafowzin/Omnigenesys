package export

import (
	"encoding/json"
	"os"
	"procedural_framework/core/grid"
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
	Layers []exportedLayer `json:"layers"`
}

func ToJSON(g *grid.Grid2D, path string) error {
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

	out := exportedGrid{
		Width:  g.Width,
		Height: g.Height,
		Layers: layers,
	}

	data, err := json.MarshalIndent(out, "", "  ")
	if err != nil {
		return err
	}

	return os.WriteFile(path, data, 0644)
}
