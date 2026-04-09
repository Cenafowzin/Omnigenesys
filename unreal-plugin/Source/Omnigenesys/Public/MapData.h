#pragma once

#include "CoreMinimal.h"

/**
 * Estruturas que espelham o JSON exportado pelo mapgen Go.
 *
 * Formato esperado:
 * {
 *   "width": 80, "height": 50, "seed": 512,
 *   "layers": [
 *     { "name": "terrain",
 *       "cells": [[{"type":"floor","metadata":null}, ...], ...] }
 *   ]
 * }
 */

struct FMapCellData
{
	FString Type;      // ex: "floor", "mato_enraizado", "estrutura_loja"
	// Metadata ignorado por ora — extensível no futuro
};

struct FMapLayerData
{
	FString Name;
	TArray<TArray<FMapCellData>> Cells; // Cells[row][col]
};

struct FMapData
{
	int32 Width  = 0;
	int32 Height = 0;
	int64 Seed   = 0;
	TArray<FMapLayerData> Layers;
};
