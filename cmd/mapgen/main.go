package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"math/rand"
	"os"
	"omnigenesys/core/export"
	"omnigenesys/core/pipeline"
	"time"
)

func main() {
	configFile := flag.String("config", "", "pipeline config JSON file (padrão: stdin)")
	flag.Parse()

	var data []byte
	var err error

	if *configFile != "" {
		data, err = os.ReadFile(*configFile)
	} else {
		data, err = io.ReadAll(os.Stdin)
	}
	if err != nil {
		fmt.Fprintf(os.Stderr, "read error: %v\n", err)
		os.Exit(1)
	}

	var cfg PipelineConfig
	if err := json.Unmarshal(data, &cfg); err != nil {
		fmt.Fprintf(os.Stderr, "config parse error: %v\n", err)
		os.Exit(1)
	}

	if cfg.Seed == 0 {
		cfg.Seed = rand.New(rand.NewSource(time.Now().UnixNano())).Int63()
	}

	g, pipe, err := buildPipeline(cfg)
	if err != nil {
		fmt.Fprintf(os.Stderr, "pipeline build error: %v\n", err)
		os.Exit(1)
	}

	ctx := pipeline.NewContext(g, cfg.Seed)
	if err := pipe.Run(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "pipeline error: %v\n", err)
		os.Exit(1)
	}

	if err := export.ToWriter(g, os.Stdout); err != nil {
		fmt.Fprintf(os.Stderr, "export error: %v\n", err)
		os.Exit(1)
	}
}
