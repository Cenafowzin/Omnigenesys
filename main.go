package main

import (
	"fmt"
	"log"
	"omnigenesys/core/export"
	"omnigenesys/maps/cornfield"
)

func main() {
	g, err := cornfield.BuildMap(512, 80, 50)
	if err != nil {
		log.Fatalf("pipeline error: %v", err)
	}

	if err := export.ToJSON(g, "map.json"); err != nil {
		log.Fatalf("export error: %v", err)
	}

	fmt.Println("map generated: map.json")
}
