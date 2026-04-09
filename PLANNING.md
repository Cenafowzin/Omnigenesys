# Omnigenesys — Planning

## Visão geral

Framework de geração procedural de mapas em Go, agnóstico de engine.
Exporta para JSON e é consumido por adapters de engine (Unity, Godot, etc.).

---

## Arquitetura do Framework (Go)

```
omnigenesys/
├── core/
│   ├── grid/            → Grid2D, Layer, Cell
│   ├── pipeline/        → Pipeline, Operator, Context, Condition
│   ├── generators/
│   │   ├── noise/       → Perlin noise (determinístico por seed)
│   │   └── pathfinding/ → A* com custo de ruído orgânico
│   ├── operators/
│   │   ├── terrain/     → Fill, FillBorder
│   │   ├── scatter/     → Scatter, NoiseScatter
│   │   ├── placement/   → PlacePoint, PlaceRoom, PlaceStructures
│   │   └── paths/       → ConnectToStructures, BranchPaths, PathConnect, ConnectPoints
│   └── export/          → ToJSON (arquivo), ToWriter (stdout)
├── maps/
│   └── cornfield/       → Pipeline do mapa cornfield (roguelike horror)
├── cmd/
│   └── mapgen/          → Entrypoint CLI para uso como subprocess
├── visualizer/          → Gerador de PNG para debug
└── main.go              → Entrypoint de desenvolvimento
```

### Princípios do framework

- **Operators** executam transformações na grid (sem lógica de jogo hardcoded)
- **Conditions** expressam regras como predicados composáveis por célula
- **Pipeline** é uma sequência de operators, configurável inteiramente do lado externo
- **Seed determinística**: mesmo seed → mesmo mapa sempre
- **Layer order**: preservada via `LayerOrder []string` no Grid2D

---

## Mapa Cornfield (roguelike horror)

Layers (em ordem de render):
| Layer      | Conteúdo                              |
|------------|---------------------------------------|
| terrain    | floor, mato_enraizado (borda), path   |
| vegetation | mato_alto (ruído Perlin)              |
| structures | estrutura_loja, _desafio, _arena, _item |
| entities   | spawn (jogador)                       |

Regras expressas como Conditions (não hardcoded nos operators):
- Estruturas não spawnnam sobre mato_enraizado → `LayerNot`
- Arena/desafio ficam longe do spawn → `NotNearType{Distance:25}`
- Caminhos não atravessam estruturas → `LayerEmpty{Layer:"structures"}`
- Vegetação não cresce sobre caminhos ou estruturas → `LayerNot + LayerEmpty`

---

## Unity Adapter — Arquitetura

### Estratégia de integração runtime: Subprocess

O Unity executa o binário Go compilado como processo filho.
O Go lê flags (seed, width, height), roda a pipeline e imprime JSON no stdout.
O Unity lê o stdout e deserializa.

```
Unity Runtime
    │
    ├─ MapGeneratorRuntime.cs
    │   └─ Process.Start("mapgen.exe --seed 512 --width 80 --height 50")
    │       └─ stdout → JSON string
    │
    ├─ MapData.cs (DTOs)
    │   └─ JsonConvert.DeserializeObject<MapData>(json)
    │
    ├─ TileRegistry.asset (ScriptableObject)
    │   ├─ "mato_enraizado" → TileBase (wall)
    │   ├─ "floor"          → TileBase
    │   ├─ "path"           → TileBase
    │   ├─ "mato_alto"      → TileBase (vegetation)
    │   ├─ "estrutura_loja" → GameObject prefab
    │   ├─ "estrutura_arena"→ GameObject prefab
    │   └─ "spawn"          → GameObject prefab (spawn marker)
    │
    └─ MapBuilder.cs
        ├─ Itera layers em ordem
        ├─ Células mapeadas para TileBase → Tilemap.SetTile()
        └─ Células mapeadas para Prefab  → Instantiate() em Transform root
```

### Estrutura de pastas Unity

```
Assets/
├── StreamingAssets/
│   └── MapGen/
│       └── mapgen.exe          ← binário Go compilado
├── Scripts/
│   └── MapGeneration/
│       ├── Data/
│       │   ├── MapData.cs
│       │   └── TileRegistry.cs
│       ├── MapBuilder.cs
│       └── MapGeneratorRuntime.cs
├── ScriptableObjects/
│   └── TileRegistry.asset
└── Prefabs/
    └── Structures/
        ├── Loja.prefab
        ├── Arena.prefab
        ├── Desafio.prefab
        └── Item.prefab
```

### Coordenadas Go → Unity

Go usa (0,0) no canto superior esquerdo com Y crescendo para baixo.
Unity Tilemap usa Y crescendo para cima.

Conversão: `unityY = -(goY)`

Centragem de tile: `worldPos = new Vector3(x + 0.5f, -y + 0.5f, 0)`

### Dependência Unity necessária

Newtonsoft JSON (suporte a arrays 2D e Dictionary):
- Package Manager → `com.unity.nuget.newtonsoft-json`

### Mapeamento de layers → Tilemaps

Cada layer do JSON mapeia para uma Tilemap separada na cena.
Isso preserva a separação de render e permite ligar/desligar layers.

```
terrain    → Tilemap "Terrain"    (sorting order 0)
vegetation → Tilemap "Vegetation" (sorting order 1)
structures → Tilemap "Structures" (sorting order 2) + prefabs
entities   → sem Tilemap (só prefabs: spawn marker)
```

---

## Fases de implementação

### Fase 1 — Adapter JSON → Unity (atual)
- [x] DTOs em C# (MapData, LayerData, CellData)
- [x] TileRegistry ScriptableObject
- [x] MapBuilder (Tilemap + Prefab placement)
- [x] MapGeneratorRuntime (subprocess)
- [ ] Testar com mapa gerado manualmente

### Fase 2 — Runtime Generation (desktop)
- [x] `export.ToWriter` no Go (stdout)
- [x] `cmd/mapgen` CLI com flags --seed, --width, --height
- [x] `maps/cornfield` como pacote reutilizável
- [ ] Compilar mapgen.exe e colocar em StreamingAssets
- [ ] Testar geração runtime no editor Unity

### Fase 3 — Futuro
- [ ] Native Plugin (CGo → .dll) para eliminar overhead de processo
  - Necessário se latência do subprocess for inaceitável (>200ms)
  - Libera suporte a console
- [ ] Port C# do framework para suporte WebGL
- [ ] Mais tipos de mapa via novos pacotes em `maps/`
- [ ] Novos generators: cellular automata, voronoi, BSP, WFC

---

## Notas de build

Compilar o binário para Windows (desenvolvimento):
```bash
go build -o cmd/mapgen/mapgen.exe ./cmd/mapgen
```

Compilar para outras plataformas (cross-compile):
```bash
GOOS=linux   GOARCH=amd64 go build -o mapgen-linux   ./cmd/mapgen
GOOS=darwin  GOARCH=amd64 go build -o mapgen-mac     ./cmd/mapgen
GOOS=windows GOARCH=amd64 go build -o mapgen.exe     ./cmd/mapgen
```
