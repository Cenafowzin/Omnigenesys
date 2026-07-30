# Omnigenesys Rust — Roadmap

> ⚠️ **[PLANNING.md](PLANNING.md) é a fonte de verdade.** Este documento é a lista de tarefas; onde conflitar com o PLANNING, o PLANNING prevalece. Conflitos conhecidos já corrigidos abaixo estão marcados com ⚠️ e a seção do PLANNING que os decide.

## Visão geral

Migração do framework de geração procedural de Go para Rust. O objetivo é um produto de mercado: multi-engine (subprocess/FFI/WASM), 2D+3D, runtime chunk generation, e fácil de criar adapters pela comunidade.

---

## Arquitetura alvo

⚠️ **Corrigido (PLANNING §1.3)**: a árvore de 6 crates é o alvo da **Fase 3**, não do início. Até lá, crate único com módulos internos — workspace prematuro é churn de `Cargo.toml` sem ganho.

Estrutura **atual**:

```
omnigenesys-rs/
├── crates/
│   └── omnigenesys-core/        → coords, tile, layer, grid, spec, rng,
│                                  registry, error, pipeline, output, context
│                                  (+ módulos internos noise, pathfinding, export)
├── bins/
│   └── mapgen/                  → CLI subprocess
└── Cargo.toml                   → Workspace
```

Estrutura **alvo na Fase 3** (quando FFI e WASM exigirem separação de verdade):

```
crates/omnigenesys-core, -noise, -pathfinding, -export, -ffi, -wasm
pipeline.schema.json             → gerado de `spec::pipeline_schema()` (schemars)
```

---

## Fase 1 — Paridade **funcional** com Go (~13-18 dias)

⚠️ **Corrigido (PLANNING §1.2)**: o objetivo é paridade *funcional*, **não** bit-exata. Mesma pipeline produz um mapa equivalente em estrutura e estilo, já no **formato v2** (pipeline e output). Determinismo é garantido *dentro do Rust* por golden snapshots (`insta`), não contra o Go.

Estimativa assume familiaridade com Rust; aprendendo a linguagem, contar 2–3×.

### 1.1 Core (2-3 dias)
- [x] `GridSize` (width, height, depth — depth=1 para 2D)
- [x] `Coord` (x, y, z — z=0 para 2D)
- [x] `TileId = u16` com `EMPTY = 0`
- [x] `TileRegistry` (string ↔ u16, interning no I/O) — ⚠️ string table ordenada na exportação (PLANNING §8.8)
- [ ] `Bounds { origin, size }` + `contains`/`expand`/`iter` em ordem de memória (PLANNING §8.3)
- [ ] `Layer<T>` genérico com `cells: Box<[T]>` sobre `Bounds`, indexação relativa a `origin` (PLANNING §8.4/§8.5)
- [ ] `AnyLayer` enum — só variante `Tiles` por enquanto
- [ ] `Grid` (bounds, layers via `IndexMap<String, AnyLayer>`, seed, acessor tipado)
- [x] `Context` (grid, rng, registry) — ⚠️ `ChaCha8Rng`, **não** `StdRng` (PLANNING §1.1: `StdRng` não é estável entre versões do crate `rand`)
- [x] `trait Operator: Send + Sync` com `fn execute(&self, ctx: &mut Context)`
- [x] `OperatorRegistry` + `Dimensions` — falta `Scope::Local|Global` (PLANNING §8.7)
- [ ] `enum Condition` (LayerIs, LayerNot, LayerEmpty, LayerClear, NearType, NotNearType) + combinadores `Any`/`All`/`Not`
- [x] `Pipeline` (build valida tudo de uma vez, run com `on_fail`)
- [x] `PipelineSpec` v2 + `params`/`$param` + schema via `schemars` — falta `layers` com `kind` (PLANNING §8.8)
- [x] `OpError`/`PipelineError` com `step_id`

### 1.2 Noise (1-2 dias)
- [ ] Perlin 2D (permutation table, fade, lerp, gradient)
- [ ] `Sampler` trait com `fn sample(&self, x: f64, y: f64) -> f64`
- [ ] `NoiseConfig` (type, scale) + `fn build(seed) -> Box<dyn Sampler>`
- [ ] Determinístico por seed (LCG para permutation shuffle)

### 1.3 Pathfinding (1-2 dias)
- [ ] `NeighborMode` enum (Four, Eight, Six, TwentySix)
- [ ] A* com `BinaryHeap`, `FxHashMap` para gScore/parent
- [ ] Heuristic: distância euclidiana
- [ ] 8 direções 2D (cardinal cost 1.0, diagonal 1.414)
- [ ] Cost function como `&dyn Fn(Coord) -> f64`
- [ ] Condition filtering por vizinho
- [ ] Path reconstruction via parent chain

### 1.4 Operators — Terrain (1 dia)
- [ ] `Fill` (preenche layer inteira com tile)
- [ ] `FillBorder` (preenche borda com espessura configurável)

### 1.5 Operators — Scatter (1 dia)
- [ ] `Scatter` (probabilidade por célula, conditions)
- [ ] `NoiseScatter` (Perlin threshold, conditions)

### 1.6 Operators — Placement (1-2 dias)
- [ ] `PlacePoint` (âncoras, offsets, conditions)
- [ ] `PlaceFixed` (retângulo uniforme com âncoras)
- [ ] `PlaceRoom` (retângulo com wall+floor)
- [ ] `PlaceStructures` (múltiplas estruturas, min_distance, avoid, max_attempts)

### 1.7 Operators — Paths (2-3 dias)
- [ ] `PathRepulsion` (layer, tile, distance, factor)
- [ ] `buildCost` (noise + repulsion → cost closure)
- [ ] `scanStructureBounds` (flood-fill, bounding boxes)
- [ ] `entryPointsFrom` (entry point no edge mais próximo)
- [ ] `PathConnect` (A* direto entre dois pontos)
- [ ] `ConnectToStructures` (scan + A* para cada estrutura)
- [ ] `BranchPaths` (múltiplos caminhos de source tiles)
- [ ] `ConnectPoints` (random walk com direct_chance)

### 1.8 Export + CLI (1-2 dias)
⚠️ **Corrigido (PLANNING §2.4)**: export é **output v2** — string table ordenada + RLE por layer + `objects` + `markers`. **Não** o formato do Go (célula vazia repetida W×H vezes). Os adapters Unity/Unreal são atualizados junto.
- [x] Tipos do output v2 + `rle_encode`/`rle_decode`
- [ ] Serialização a partir do `Grid` (usar iterador do `Layer`, não `&[T]` — PLANNING §8.5)
- [ ] `ToWriter(grid, writer)` compacto
- [ ] CLI `mapgen`: --config (arquivo ou stdin), --out (arquivo ou stdout)
- [ ] Parse de `PipelineConfig` JSON (serde) com dispatch de operators

### 1.9 Testes de paridade (2 dias)
⚠️ **Corrigido (PLANNING §1.2)**: paridade bit-exata Go vs Rust foi descartada (exigiria reimplementar o `math/rand` do Go; custo alto, valor baixo). Substituída por testes estruturais + golden snapshots `insta` + comparação visual PNG lado a lado.
- [ ] Golden snapshots `insta`: seed X + pipeline Y = output exato, congelado
- [ ] Testes estruturais: nº de estruturas, paths conectam, conditions respeitadas
- [ ] Portar mapa example (cornfield) como teste de integração
- [ ] Validar todos os operators individualmente
- [ ] Benchmark comparativo Go vs Rust

---

## Fase 2 — 3D e Voxel (~10-14 dias)

### 2.1 Generators 3D (3-4 dias)
- [ ] Perlin 3D (trilinear interpolation, 8 gradientes)
- [ ] A* 3D com NeighborMode::Six e TwentySix
- [ ] Heightmap → Voxel (noise 2D empilhado)

### 2.2 Operators 3D (~0 dias)
⚠️ **Corrigido (PLANNING §8.6)**: esta seção era artefato de portar os tipos 2D-only do Go literalmente. Com `Bounds`/`Coord` sempre 3D e `depth=1` como caso 2D, os operators já servem as duas dimensões — `FillVolume`, `FillShell`, `PlacePoint3D`, `PlaceVolumes`, `VolumeScatter`, `NoiseScatter3D` são **todos redundantes**.
- [ ] `PlaceRoom` em 3D: decidir semântica (piso, teto, 4 paredes) — é ambiguidade de design, não código novo
- [ ] Marcar `Dimensions` corretamente nos genuinamente 2D-only (noise 2D, heightmap)

### 2.3 Spatial Index
⚠️ **Movido para a Fase 4 (PLANNING §2.5/§5)**: **distance field** por (layer, tile) — BFS multi-source O(N) cacheado no `Context`, invalidado quando a layer é escrita — resolve `NotNearType`/`NearType`/`LayerClear` com ganho maior e complexidade menor que spatial hash. Spatial hash fica só para 3D esparso, se medição justificar.

### 2.4 Export 3D (1-2 dias)
- [ ] Sparse format (só células não-vazias)
- [ ] Campo `depth` no JSON config e output
- [ ] MessagePack como formato alternativo

---

## Fase 3 — Integração com engines (~6-7 dias)

### 3.1 FFI / Native Plugin (2-3 dias)
- [ ] Crate `omnigenesys-ffi` (cdylib)
- [ ] `omni_generate(config, len, &out, &out_len) -> i32`
- [ ] `omni_free(ptr, len)`
- [ ] Header C gerado automaticamente (cbindgen)
- [ ] Formato binário direto para zero-copy: `[header | layer0 tiles | layer1 tiles | ...]`

### 3.2 WASM (2 dias)
- [ ] Crate `omnigenesys-wasm` (wasm-bindgen)
- [ ] `generate(config_json: &str) -> String` (JSON in, JSON out)
- [ ] Build com `wasm-pack`
- [ ] Teste em browser (preview de mapa)

### 3.3 Adapters atualizados (2 dias)
- [ ] Atualizar adapter Unreal para usar .dll FFI (opcional, subprocess continua funcionando)
- [ ] Atualizar adapter Unity para usar .dll FFI
- [ ] Documentar: "How to build an adapter" para comunidade

---

## Fase 4 — Performance (~2-3 dias)

### 4.1 Rayon parallelism (2 dias)
- [ ] Scatter paralelo (partition por região, thread-local RNG derivado de seed+index)
- [ ] Pathfinding paralelo (múltiplos paths simultâneos em ConnectToStructures/BranchPaths)
- [ ] Benchmark: single-thread vs rayon em mapas 200×200 e volumes 100³

### 4.2 Otimizações (1 dia)
- [ ] `FxHashMap` no A* (rustc-hash, hashing mais rápido)
- [ ] Verificar autovectorization no noise sampling (flags SIMD no release build)
- [ ] Profile com `cargo flamegraph` e otimizar hotspots reais

---

## Fase 5 — Novos generators (~4-5 dias)
- [ ] 3D Cellular Automata (caves, túneis estilo Minecraft)
- [ ] Marching Cubes (voxel grid → mesh)
- [ ] 3D Voronoi (biomas volumétricos)
- [ ] SDF (Signed Distance Fields — formas orgânicas)

---

## Fase 6 — Produto (~5-7 dias)
Detalhes de produto, IA e monetização: PLANNING §6 e §7.
- [ ] Interface web (editor de pipeline schema-driven, preview via WASM no browser)
- [ ] `pipeline.schema.json` gerado de `spec::pipeline_schema()` — schema nunca dessincroniza do código
- [ ] Documentação completa (PIPELINE.md portado + guia "How to build an adapter")
- [x] ⚠️ Versionamento do formato JSON — já é **`"version": 2`** (PLANNING §2.1), não 1
- [ ] Publicar no crates.io (`cargo add omnigenesys-core`)
- [ ] Landing page mínima do projeto

---

## Crates Rust recomendados

| Funcionalidade | Crate |
|---|---|
| RNG determinístico | `rand` + `rand_chacha` (ChaCha8Rng) |
| Serialização JSON | `serde` + `serde_json` |
| CLI args | `clap` |
| Paralelismo | `rayon` |
| Priority queue (A*) | `std::collections::BinaryHeap` |
| HashMap rápido | `rustc-hash` (FxHashMap) |
| Ordered map (layers) | `indexmap` |
| WASM bindings | `wasm-bindgen` + `wasm-pack` |
| Formato binário | `rmp-serde` (MessagePack) ou `flatbuffers` |
| C header gen | `cbindgen` |
| Profiling | `cargo-flamegraph` |

---

## Mapeamento Go → Rust

### Tipos

⚠️ Atualizado conforme PLANNING §8.

| Go | Rust |
|---|---|
| `grid.Cell { Type string, Metadata map[string]any }` | `TileId (u16)` — **metadata por célula removida** (§8.8); `objects`/`markers` cobrem o caso |
| `grid.Layer { Name, Cells [][]Cell }` | `Layer<T> { bounds: Bounds, cells: Box<[T]> }` dentro de `AnyLayer::Tiles` |
| `grid.Grid2D { Width, Height, Seed, Layers, LayerOrder }` | `Grid { bounds: Bounds, layers: IndexMap<String, AnyLayer>, seed }` |
| `pipeline.Context { Grid, RNG }` | `Context { grid, tiles: TileRegistry, rng: ChaCha8Rng }` — rng **derivado por step** (§2.1) |
| `pipeline.Operator (interface)` | `trait Operator: Send + Sync` |
| `pipeline.Condition (structs com switch)` | `enum Condition` |
| `noise.Perlin { perm [512]int }` | `Perlin { perm: [u8; 512] }` |
| `pathfinding.Point { X, Y }` | `Coord { x: i32, y: i32, z: i32 }` |

### Operators — 1:1

| Go | Rust | Campos |
|---|---|---|
| `terrain.Fill` | `Fill` | layer, tile |
| `terrain.FillBorder` | `FillBorder` | layer, tile, thickness |
| `placement.PlacePoint` | `PlacePoint` | layer, tile, anchor_x/y/z, offset_x/y/z |
| `placement.PlaceRoom` | `PlaceRoom` | layer, x, y, width, height, floor, wall |
| `placement.PlaceFixed` | `PlaceFixed` | layer, tile, width, height, anchor, offset |
| `placement.PlaceStructures` | `PlaceStructures` | layer, structures[], min_distance, avoid_layer, avoid_type |
| `scatter.Scatter` | `Scatter` | layer, tile, chance, conditions |
| `scatter.NoiseScatter` | `NoiseScatter` | layer, tile, threshold, noise config, conditions |
| `paths.PathConnect` | `PathConnect` | layer, tile, from, to, noise_factor, noise config |
| `paths.ConnectToStructures` | `ConnectToStructures` | layer, tile, from, structures_layer, clearance, noise, repulsion |
| `paths.BranchPaths` | `BranchPaths` | source_layer, source_tile, layer, tile, branches, noise, repulsion |
| `paths.ConnectPoints` | `ConnectPoints` | layer, tile, from, to, direct_chance, max_steps, diagonal |

---

## Visão de longo prazo

```
Onirika (ecossistema)
├── Omnigenesys (módulos de procgen separados)
│   ├── omni-mapgen      ← mapas/terreno (este projeto)
│   ├── omni-dialog      ← diálogos procedurais
│   ├── omni-npc         ← comportamento/spawning de NPCs
│   ├── omni-loot        ← economia/drops procedurais
│   └── omni-quest       ← missões procedurais
│
├── Dream Box (core mínimo de engine — futuro)
│   ├── ECS + Game loop + Event bus + Plugin API
│   └── cada módulo Omni vira plugin nativo
│
└── Lost Fields (jogo — prova tudo em produção)
```

Cada módulo funciona standalone com qualquer engine (subprocess/FFI/WASM) hoje, e vira plugin nativo do Dream Box amanhã. O Dream Box não é projetado top-down — emerge bottom-up dos padrões reais descobertos construindo os módulos Omni.
