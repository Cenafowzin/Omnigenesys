package main

import (
	"fmt"
	"omnigenesys/core/grid"
	"omnigenesys/core/operators/paths"
	"omnigenesys/core/operators/placement"
	"omnigenesys/core/operators/scatter"
	"omnigenesys/core/operators/terrain"
	"omnigenesys/core/pipeline"
)

// PipelineConfig é o formato JSON que o Unity envia para o mapgen.
// Define a grid, seed e a lista de operators a executar em ordem.
type PipelineConfig struct {
	Seed   int64        `json:"seed"`
	Width  int          `json:"width"`
	Height int          `json:"height"`
	Layers []string     `json:"layers"`
	Steps  []StepConfig `json:"steps"`
}

// StepConfig descreve um operator. Todos os campos possíveis ficam aqui;
// o dispatcher usa apenas os relevantes para cada operator.
type StepConfig struct {
	Operator string `json:"operator"`

	// Campos comuns
	Layer string `json:"layer,omitempty"`
	Tile  string `json:"tile,omitempty"`

	// FillBorder
	Thickness int `json:"thickness,omitempty"`

	// PlacePoint
	AnchorX string `json:"anchor_x,omitempty"`
	AnchorY string `json:"anchor_y,omitempty"`
	X       int    `json:"x,omitempty"`
	Y       int    `json:"y,omitempty"`
	OffsetX int    `json:"offset_x,omitempty"`
	OffsetY int    `json:"offset_y,omitempty"`

	// PlaceStructures
	Structures  []StructureDefConfig `json:"structures,omitempty"`
	MinDistance int                  `json:"min_distance,omitempty"`
	AvoidLayer  string               `json:"avoid_layer,omitempty"`
	AvoidType   string               `json:"avoid_type,omitempty"`
	MaxAttempts int                  `json:"max_attempts,omitempty"`

	// Paths — ponto de origem (suporta âncoras igual ao PlacePoint)
	FromAnchorX string `json:"from_anchor_x,omitempty"`
	FromAnchorY string `json:"from_anchor_y,omitempty"`
	FromX       int    `json:"from_x,omitempty"`
	FromY       int    `json:"from_y,omitempty"`
	FromOffsetX int    `json:"from_offset_x,omitempty"`
	FromOffsetY int    `json:"from_offset_y,omitempty"`

	StructuresLayer string  `json:"structures_layer,omitempty"`
	Clearance       int     `json:"clearance,omitempty"`
	NoiseFactor     float64 `json:"noise_factor,omitempty"`
	NoiseScale      float64 `json:"noise_scale,omitempty"`

	// BranchPaths
	SourceLayer string `json:"source_layer,omitempty"`
	SourceTile  string `json:"source_tile,omitempty"`
	Branches    int    `json:"branches,omitempty"`

	// Scatter
	Chance float64 `json:"chance,omitempty"`

	// NoiseScatter
	Threshold float64 `json:"threshold,omitempty"`
	Scale     float64 `json:"scale,omitempty"`

	Conditions []ConditionConfig `json:"conditions,omitempty"`
}

type StructureDefConfig struct {
	Type       string            `json:"type"`
	Width      int               `json:"width"`
	Height     int               `json:"height"`
	Conditions []ConditionConfig `json:"conditions,omitempty"`
}

// ConditionConfig descreve uma condition. "condition" é o tipo:
// LayerIs, LayerNot, LayerEmpty, NotNearType, LayerClear
type ConditionConfig struct {
	Condition string `json:"condition"`
	Layer     string `json:"layer,omitempty"`
	Tile      string `json:"tile,omitempty"`
	Distance  int    `json:"distance,omitempty"`
}

// ── builders ────────────────────────────────────────────────────────────────

func buildPipeline(cfg PipelineConfig) (*grid.Grid2D, *pipeline.Pipeline, error) {
	g := grid.NewGrid2D(cfg.Width, cfg.Height)
	for _, l := range cfg.Layers {
		g.AddLayer(l)
	}

	pipe := pipeline.NewPipeline()
	for i, step := range cfg.Steps {
		op, err := buildOperator(step, cfg.Width, cfg.Height)
		if err != nil {
			return nil, nil, fmt.Errorf("step %d (%s): %w", i, step.Operator, err)
		}
		pipe.AddStep(op)
	}
	return g, pipe, nil
}

func buildOperator(s StepConfig, width, height int) (pipeline.Operator, error) {
	conds := buildConditions(s.Conditions)

	switch s.Operator {
	case "Fill":
		return &terrain.Fill{Layer: s.Layer, Tile: s.Tile}, nil

	case "FillBorder":
		return &terrain.FillBorder{Layer: s.Layer, Tile: s.Tile, Thickness: s.Thickness}, nil

	case "PlacePoint":
		return &placement.PlacePoint{
			Layer:   s.Layer,
			Tile:    s.Tile,
			AnchorX: s.AnchorX, AnchorY: s.AnchorY,
			X: s.X, Y: s.Y,
			OffsetX: s.OffsetX, OffsetY: s.OffsetY,
			Conditions: conds,
		}, nil

	case "PlaceStructures":
		defs := make([]placement.StructureDef, len(s.Structures))
		for i, sd := range s.Structures {
			defs[i] = placement.StructureDef{
				Type: sd.Type, Width: sd.Width, Height: sd.Height,
				Conditions: buildConditions(sd.Conditions),
			}
		}
		return &placement.PlaceStructures{
			Layer: s.Layer, Structures: defs,
			MinDistance: s.MinDistance,
			AvoidLayer:  s.AvoidLayer, AvoidType: s.AvoidType,
			MaxAttempts: s.MaxAttempts,
		}, nil

	case "ConnectToStructures":
		from := resolvePoint(s.FromAnchorX, s.FromAnchorY, s.FromX, s.FromY, s.FromOffsetX, s.FromOffsetY, width, height)
		return &paths.ConnectToStructures{
			Layer: s.Layer, Tile: s.Tile,
			From:            paths.Point{X: from[0], Y: from[1]},
			StructuresLayer: s.StructuresLayer,
			Clearance:       s.Clearance,
			NoiseFactor:     s.NoiseFactor, NoiseScale: s.NoiseScale,
			Conditions: conds,
		}, nil

	case "BranchPaths":
		return &paths.BranchPaths{
			SourceLayer: s.SourceLayer, SourceTile: s.SourceTile,
			Layer: s.Layer, Tile: s.Tile,
			Branches:        s.Branches,
			StructuresLayer: s.StructuresLayer,
			Clearance:       s.Clearance,
			NoiseFactor:     s.NoiseFactor, NoiseScale: s.NoiseScale,
			Conditions: conds,
		}, nil

	case "Scatter":
		return &scatter.Scatter{
			Layer: s.Layer, Tile: s.Tile,
			Chance: s.Chance, Conditions: conds,
		}, nil

	case "NoiseScatter":
		return &scatter.NoiseScatter{
			Layer: s.Layer, Tile: s.Tile,
			Threshold: s.Threshold, Scale: s.Scale,
			Conditions: conds,
		}, nil

	default:
		return nil, fmt.Errorf("unknown operator %q", s.Operator)
	}
}

func buildConditions(cfgs []ConditionConfig) []pipeline.Condition {
	conds := make([]pipeline.Condition, 0, len(cfgs))
	for _, c := range cfgs {
		switch c.Condition {
		case "LayerIs":
			conds = append(conds, pipeline.LayerIs{Layer: c.Layer, Type: c.Tile})
		case "LayerNot":
			conds = append(conds, pipeline.LayerNot{Layer: c.Layer, Type: c.Tile})
		case "LayerEmpty":
			conds = append(conds, pipeline.LayerEmpty{Layer: c.Layer})
		case "NotNearType":
			conds = append(conds, pipeline.NotNearType{Layer: c.Layer, Type: c.Tile, Distance: c.Distance})
		case "LayerClear":
			conds = append(conds, pipeline.LayerClear{Layer: c.Layer, Distance: c.Distance})
		}
	}
	return conds
}

// resolvePoint resolve âncoras para coordenadas absolutas, igual ao PlacePoint.
func resolvePoint(anchorX, anchorY string, x, y, offsetX, offsetY, width, height int) [2]int {
	rx := x
	switch anchorX {
	case "center":
		rx = width / 2
	case "left":
		rx = 0
	case "right":
		rx = width - 1
	}

	ry := y
	switch anchorY {
	case "center":
		ry = height / 2
	case "top":
		ry = 0
	case "bottom":
		ry = height - 1
	}

	return [2]int{rx + offsetX, ry + offsetY}
}
