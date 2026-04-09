package example

import (
	"omnigenesys/core/grid"
	"omnigenesys/core/operators/paths"
	"omnigenesys/core/operators/placement"
	"omnigenesys/core/operators/scatter"
	"omnigenesys/core/operators/terrain"
	"omnigenesys/core/pipeline"
)

// BuildMap cria e executa a pipeline de exemplo do Omnigenesys.
// Retorna o Grid2D pronto para exportação.
func BuildMap(seed int64, width, height int) (*grid.Grid2D, error) {
	g := grid.NewGrid2D(width, height)
	g.AddLayer("terrain")
	g.AddLayer("vegetation")
	g.AddLayer("structures")
	g.AddLayer("entities")

	ctx := pipeline.NewContext(g, seed)

	spawnX := g.Width / 2
	spawnY := g.Height - 4

	pipe := pipeline.NewPipeline().
		AddStep(&terrain.Fill{Layer: "terrain", Tile: "floor"}).
		AddStep(&terrain.FillBorder{Layer: "terrain", Tile: "border", Thickness: 2}).
		AddStep(&placement.PlacePoint{
			Layer:   "entities",
			Tile:    "spawn",
			AnchorX: "center",
			AnchorY: "bottom",
			OffsetY: -3,
		}).
		AddStep(&placement.PlaceStructures{
			Layer: "structures",
			Structures: []placement.StructureDef{
				{
					Type: "zone_a", Width: 9, Height: 9,
					Conditions: []pipeline.Condition{
						pipeline.NotNearType{Layer: "entities", Type: "spawn", Distance: 25},
					},
				},
				{
					Type: "zone_b", Width: 7, Height: 7,
					Conditions: []pipeline.Condition{
						pipeline.NotNearType{Layer: "entities", Type: "spawn", Distance: 25},
					},
				},
				{Type: "zone_c", Width: 6, Height: 6},
				{Type: "zone_d", Width: 4, Height: 4},
			},
			MinDistance: 5,
			AvoidLayer:  "terrain",
			AvoidType:   "border",
		}).
		AddStep(&paths.ConnectToStructures{
			Layer:           "terrain",
			Tile:            "path",
			From:            paths.Point{X: spawnX, Y: spawnY},
			StructuresLayer: "structures",
			Clearance:       3,
			NoiseFactor:     4.0,
			NoiseScale:      0.12,
			Conditions: []pipeline.Condition{
				pipeline.LayerNot{Layer: "terrain", Type: "border"},
				pipeline.LayerEmpty{Layer: "structures"},
			},
		}).
		AddStep(&paths.BranchPaths{
			SourceLayer:     "terrain",
			SourceTile:      "path",
			Layer:           "terrain",
			Tile:            "path",
			Branches:        3,
			StructuresLayer: "structures",
			Clearance:       3,
			NoiseFactor:     3.5,
			NoiseScale:      0.12,
			Conditions: []pipeline.Condition{
				pipeline.LayerNot{Layer: "terrain", Type: "border"},
				pipeline.LayerEmpty{Layer: "structures"},
			},
		}).
		AddStep(&paths.BranchPaths{
			SourceLayer: "terrain",
			SourceTile:  "path",
			Layer:       "terrain",
			Tile:        "path",
			Branches:    4,
			NoiseFactor: 2.0,
			NoiseScale:  0.12,
			Conditions: []pipeline.Condition{
				pipeline.LayerNot{Layer: "terrain", Type: "border"},
				pipeline.LayerEmpty{Layer: "structures"},
			},
		}).
		AddStep(&scatter.NoiseScatter{
			Layer:     "vegetation",
			Tile:      "foliage",
			Threshold: 0.52,
			Scale:     0.18,
			Conditions: []pipeline.Condition{
				pipeline.LayerIs{Layer: "terrain", Type: "floor"},
				pipeline.LayerNot{Layer: "terrain", Type: "path"},
				pipeline.LayerEmpty{Layer: "structures"},
				pipeline.LayerEmpty{Layer: "entities"},
			},
		})

	if err := pipe.Run(ctx); err != nil {
		return nil, err
	}

	return g, nil
}
