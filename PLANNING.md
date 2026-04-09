# Omnigenesys — Planning

## Visão geral

Framework de geração procedural de mapas em Go, agnóstico de engine.
Exporta para JSON e é consumido por adapters de engine (Unreal, Unity, Godot, etc.).

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
│   └── example/       → Pipeline do mapa example (roguelike horror)
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
| Layer      | Conteúdo                                        |
|------------|-------------------------------------------------|
| terrain    | floor, mato_enraizado (borda), path             |
| vegetation | mato_alto (ruído Perlin)                        |
| structures | estrutura_loja, _desafio, _arena, _item         |
| entities   | spawn (jogador)                                 |

Regras expressas como Conditions:
- Estruturas não spawnnam sobre mato_enraizado → `LayerNot`
- Arena/desafio ficam longe do spawn → `NotNearType{Distance:25}`
- Caminhos não atravessam estruturas → `LayerEmpty{Layer:"structures"}`
- Vegetação não cresce sobre caminhos ou estruturas → `LayerNot + LayerEmpty`

---

## Adapter Unreal Engine 5

### Arquitetura

```
AMapGeneratorRuntime
  └─ Executa mapgen.exe como subprocess
  └─ Passa pipeline JSON via arquivo temporário
  └─ Lê mapa JSON do stdout
  └─ Chama AMapBuilder.BuildFromJson()

AMapBuilder
  └─ Parseia o JSON do mapa
  └─ Mesh tiles → UInstancedStaticMeshComponent (1 draw call por tipo)
  └─ Actor tiles → SpawnActor por região contígua (bSpawnPerCell=false)
                 → SpawnActor por célula (bSpawnPerCell=true)
                 → Registra posição (bIsSpawnPoint=true)

UTileRegistry (Data Asset)
  └─ MeshMappings: TileType → StaticMesh + Material
  └─ ActorMappings: TileType → Actor Blueprints (variantes) + flags
```

### Sistema de coordenadas

Go (0,0) = canto superior esquerdo, Row cresce para baixo.
Unreal: `X = (Col + 0.5) * TileSize`, `Y = (Row + 0.5) * TileSize`, `Z = BaseZ`.
Mapa gerado em X+ e Y+ a partir do AMapBuilder.

### Spawn do player

Tiles com `bIsSpawnPoint=true` não spawnam actors — registram posições em `AMapBuilder.GetSpawnPositions()`. O game lê essas posições e posiciona o player.

---

## Adapter Unity

### Estratégia de integração: Subprocess

```
MapGeneratorRuntime.cs
  └─ Process.Start("mapgen.exe --seed 512 --width 80 --height 50")
  └─ stdout → JSON string → JsonConvert.DeserializeObject<MapData>()

TileRegistry.asset (ScriptableObject)
  └─ TileType → TileBase (Tilemap) ou GameObject prefab

MapBuilder.cs
  └─ Itera layers em ordem
  └─ TileBase → Tilemap.SetTile()
  └─ Prefab → Instantiate()
```

### Coordenadas Go → Unity

Go usa Y crescendo para baixo, Unity Tilemap usa Y crescendo para cima.
Conversão: `unityY = -(goY)` | Centragem: `worldPos = new Vector3(x + 0.5f, -y + 0.5f, 0)`

---

## Status de implementação

### Framework Go
- [x] Grid2D com layers e layer order
- [x] Pipeline com operators e conditions
- [x] Generators: Perlin noise, A* pathfinding
- [x] Operators: terrain, scatter, placement, paths
- [x] Export: ToJSON, ToWriter (stdout)
- [x] CLI: cmd/mapgen com flags --seed, --width, --height
- [x] Mapa example como pacote reutilizável

### Adapter Unreal Engine 5
- [x] TileRegistry Data Asset (mesh + actor mappings)
- [x] MapBuilder (ISM por tipo + SpawnActor por região/célula)
- [x] MapGeneratorRuntime (subprocess + pipeline JSON)
- [x] bSpawnPerCell (actor por célula vs por região)
- [x] bIsSpawnPoint + GetSpawnPositions()
- [x] Alinhamento de tiles na grid (TileSize centrado)

### Adapter Unity
- [x] TileRegistry ScriptableObject
- [x] MapBuilder (Tilemap + Prefab placement)
- [x] MapGeneratorRuntime (subprocess)

---

## Roadmap

- [ ] Native Plugin (CGo → .dll) para eliminar overhead de subprocess e suportar console
- [ ] Port C# do core para suporte WebGL no Unity
- [ ] Adapter Godot
- [ ] Novos generators: cellular automata, voronoi, BSP, WFC
- [ ] Mais mapas de exemplo em `maps/`

---

## Notas de build

```bash
# Windows
go build -o mapgen.exe ./cmd/mapgen

# Cross-compile
GOOS=linux   GOARCH=amd64 go build -o mapgen-linux   ./cmd/mapgen
GOOS=darwin  GOARCH=amd64 go build -o mapgen-mac     ./cmd/mapgen
GOOS=windows GOARCH=amd64 go build -o mapgen.exe     ./cmd/mapgen
```
