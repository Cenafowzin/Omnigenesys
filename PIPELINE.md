# Omnigenesys — Criando Pipelines JSON

Uma pipeline define como o mapa é gerado: quais layers existem, quais operators rodam e em que ordem. O `mapgen` lê esse arquivo e exporta o mapa como JSON.

---

## Estrutura raiz

```json
{
  "seed": 512,
  "width": 80,
  "height": 50,
  "layers": ["terrain", "vegetation", "structures", "entities"],
  "steps": [ ... ]
}
```

| Campo | Tipo | Descrição |
|-------|------|-----------|
| `seed` | int | Seed de geração. `0` = aleatório a cada run |
| `width` | int | Largura do mapa em células |
| `height` | int | Altura do mapa em células |
| `layers` | string[] | Layers do mapa em ordem de render (menor index = fundo) |
| `steps` | object[] | Operators executados em sequência |

> **Importante:** layers devem ser declarados aqui antes de serem usados nos steps.

---

## Operators

Cada step tem um campo `"operator"` que define qual operação executar.

---

### `Fill`
Preenche toda a layer com um tipo de tile.

```json
{
  "operator": "Fill",
  "layer": "terrain",
  "tile": "floor"
}
```

---

### `FillBorder`
Preenche a borda da grid com um tile de espessura configurável.

```json
{
  "operator": "FillBorder",
  "layer": "terrain",
  "tile": "border",
  "thickness": 2
}
```

| Campo | Tipo | Descrição |
|-------|------|-----------|
| `thickness` | int | Espessura da borda em células |

---

### `PlacePoint`
Coloca um tile em um ponto específico ou âncora da grid.

```json
{
  "operator": "PlacePoint",
  "layer": "entities",
  "tile": "spawn",
  "anchor_x": "center",
  "anchor_y": "bottom",
  "offset_y": -3
}
```

| Campo | Tipo | Valores | Descrição |
|-------|------|---------|-----------|
| `anchor_x` | string | `"left"`, `"center"`, `"right"` | Ponto base em X |
| `anchor_y` | string | `"top"`, `"center"`, `"bottom"` | Ponto base em Y |
| `x` | int | — | Posição absoluta X (ignorado se anchor_x definido) |
| `y` | int | — | Posição absoluta Y (ignorado se anchor_y definido) |
| `offset_x` | int | — | Deslocamento em X a partir da âncora |
| `offset_y` | int | — | Deslocamento em Y a partir da âncora |

> `anchor_y: "bottom"` + `offset_y: -3` → 3 células acima da borda inferior.

---

### `PlaceStructures`
Distribui estruturas retangulares aleatoriamente no mapa respeitando distância mínima entre elas.

```json
{
  "operator": "PlaceStructures",
  "layer": "structures",
  "min_distance": 5,
  "avoid_layer": "terrain",
  "avoid_type": "border",
  "structures": [
    {
      "type": "zone_a",
      "width": 9,
      "height": 9,
      "conditions": [
        { "condition": "NotNearType", "layer": "entities", "tile": "spawn", "distance": 25 }
      ]
    },
    { "type": "zone_b", "width": 6, "height": 6 }
  ]
}
```

| Campo | Tipo | Descrição |
|-------|------|-----------|
| `min_distance` | int | Distância mínima entre estruturas |
| `avoid_layer` | string | Layer a evitar ao posicionar |
| `avoid_type` | string | Tipo de tile nessa layer a evitar |
| `max_attempts` | int | Tentativas de posicionamento por estrutura (padrão interno) |
| `structures` | object[] | Lista de estruturas a colocar |

Cada estrutura:

| Campo | Tipo | Descrição |
|-------|------|-----------|
| `type` | string | Nome do tile da estrutura |
| `width` | int | Largura em células |
| `height` | int | Altura em células |
| `conditions` | object[] | Conditions extras para posicionamento |

---

### `ConnectToStructures`
Traça caminhos do ponto de origem até cada estrutura usando A* com ruído orgânico.

```json
{
  "operator": "ConnectToStructures",
  "layer": "terrain",
  "tile": "path",
  "from_anchor_x": "center",
  "from_anchor_y": "bottom",
  "from_offset_y": -3,
  "structures_layer": "structures",
  "clearance": 3,
  "noise_factor": 4.0,
  "noise_scale": 0.12,
  "conditions": [
    { "condition": "LayerNot",   "layer": "terrain", "tile": "border" },
    { "condition": "LayerEmpty", "layer": "structures" }
  ]
}
```

| Campo | Tipo | Descrição |
|-------|------|-----------|
| `from_anchor_x/y` | string | Âncora do ponto de origem (mesmos valores do PlacePoint) |
| `from_x/y` | int | Posição absoluta de origem |
| `from_offset_x/y` | int | Offset sobre a âncora |
| `structures_layer` | string | Layer onde as estruturas estão |
| `clearance` | int | Margem em células ao redor das estruturas |
| `noise_factor` | float | Intensidade do ruído no caminho (0 = reto, >3 = orgânico) |
| `noise_scale` | float | Escala do Perlin noise (0.1–0.2 recomendado) |

---

### `BranchPaths`
Cria ramificações a partir de tiles de caminho existentes, conectando a estruturas próximas.

```json
{
  "operator": "BranchPaths",
  "source_layer": "terrain",
  "source_tile": "path",
  "layer": "terrain",
  "tile": "path",
  "branches": 3,
  "structures_layer": "structures",
  "clearance": 3,
  "noise_factor": 3.5,
  "noise_scale": 0.12,
  "conditions": [
    { "condition": "LayerNot",   "layer": "terrain", "tile": "border" },
    { "condition": "LayerEmpty", "layer": "structures" }
  ]
}
```

| Campo | Tipo | Descrição |
|-------|------|-----------|
| `source_layer` | string | Layer onde os caminhos de origem estão |
| `source_tile` | string | Tipo do tile de origem |
| `branches` | int | Número de ramificações a criar |

---

### `Scatter`
Espalha tiles aleatoriamente por toda a layer com uma probabilidade por célula.

```json
{
  "operator": "Scatter",
  "layer": "vegetation",
  "tile": "arbusto",
  "chance": 0.15,
  "conditions": [
    { "condition": "LayerIs", "layer": "terrain", "tile": "floor" }
  ]
}
```

| Campo | Tipo | Descrição |
|-------|------|-----------|
| `chance` | float | Probabilidade de colocar o tile em cada célula (0.0–1.0) |

---

### `NoiseScatter`
Espalha tiles usando Perlin noise — cria agrupamentos naturais em vez de distribuição aleatória pura.

```json
{
  "operator": "NoiseScatter",
  "layer": "vegetation",
  "tile": "foliage",
  "threshold": 0.52,
  "scale": 0.18,
  "conditions": [
    { "condition": "LayerIs",    "layer": "terrain", "tile": "floor" },
    { "condition": "LayerNot",   "layer": "terrain", "tile": "path" },
    { "condition": "LayerEmpty", "layer": "structures" }
  ]
}
```

| Campo | Tipo | Descrição |
|-------|------|-----------|
| `threshold` | float | Células com noise > threshold recebem o tile (0.0–1.0). Maior = menos tiles |
| `scale` | float | Escala do noise (0.1 = grandes manchas, 0.3 = granulado) |

---

## Conditions

Conditions filtram onde um operator pode agir. Todas devem ser verdadeiras para a célula ser afetada. Usadas em `"conditions"` de qualquer operator ou de estruturas individuais.

| Condition | Campos extras | Descrição |
|-----------|---------------|-----------|
| `LayerIs` | `layer`, `tile` | Célula deve ter esse tile nessa layer |
| `LayerNot` | `layer`, `tile` | Célula não pode ter esse tile nessa layer |
| `LayerEmpty` | `layer` | Layer deve estar vazia nessa célula |
| `NotNearType` | `layer`, `tile`, `distance` | Célula deve estar a mais de `distance` células de qualquer tile desse tipo |
| `LayerClear` | `layer`, `distance` | Layer deve estar vazia num raio de `distance` células |

```json
{ "condition": "LayerIs",      "layer": "terrain",  "tile": "floor" }
{ "condition": "LayerNot",     "layer": "terrain",  "tile": "border" }
{ "condition": "LayerEmpty",   "layer": "structures" }
{ "condition": "NotNearType",  "layer": "entities", "tile": "spawn", "distance": 25 }
{ "condition": "LayerClear",   "layer": "structures", "distance": 3 }
```

---

## Exemplo completo mínimo

Mapa simples com chão, borda, vegetação e um ponto de spawn:

```json
{
  "seed": 0,
  "width": 40,
  "height": 30,
  "layers": ["terrain", "vegetation", "entities"],
  "steps": [
    {
      "operator": "Fill",
      "layer": "terrain",
      "tile": "floor"
    },
    {
      "operator": "FillBorder",
      "layer": "terrain",
      "tile": "wall",
      "thickness": 1
    },
    {
      "operator": "PlacePoint",
      "layer": "entities",
      "tile": "spawn",
      "anchor_x": "center",
      "anchor_y": "center"
    },
    {
      "operator": "NoiseScatter",
      "layer": "vegetation",
      "tile": "tree",
      "threshold": 0.55,
      "scale": 0.15,
      "conditions": [
        { "condition": "LayerIs",    "layer": "terrain", "tile": "floor" },
        { "condition": "LayerNot",   "layer": "terrain", "tile": "wall" },
        { "condition": "LayerEmpty", "layer": "entities" }
      ]
    }
  ]
}
```
