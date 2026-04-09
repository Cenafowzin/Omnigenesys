# Omnigenesys — Instalação e Uso (Unreal Engine 5)

## 1. Instalar o plugin

Copie a pasta `Omnigenesys` (esta pasta inteira) para:

```
<SeuProjeto>/Plugins/Omnigenesys/
```

> Se a pasta `Plugins/` não existir no projeto, crie-a.

## 2. Compilar o plugin

Abra o projeto no Unreal Editor — ele detecta o plugin automaticamente e pergunta se deseja compilar. Clique em **Yes** e aguarde. Após abrir, clique no botão de compilação na toolbar e confirme o status.

> **Se algo der errado** (crash, plugin não aparece): feche o editor, delete as pastas `Plugins/Omnigenesys/Binaries/` e `Plugins/Omnigenesys/Intermediate/`, reabra o projeto e compile novamente.

> Para mudanças em headers C++ do plugin, feche o projeto completamente antes de recompilar — o hot reload não é confiável nesses casos.

## 3. Preparar os binários Go

Os binários são gerados pelo script `build.sh` do repositório Omnigenesys e já ficam na pasta certa automaticamente:

```bash
# No repositório Omnigenesys
./build.sh windows   # gera mapgen.exe e copia para Content/MapGen/
./build.sh all       # gera para Windows, Linux e macOS
```

A estrutura final deve ser:

```
Plugins/Omnigenesys/Content/MapGen/
├── mapgen.exe
└── example_pipeline.json     ← ou sua pipeline customizada
```

> Para usar arquivos do seu próprio projeto em vez dos do plugin, prefixe o caminho com `project:` nos campos do `AMapGeneratorRuntime`:
> - `ExecutablePath` = `project:MapGen/mapgen.exe`
> - `PipelineConfigPath` = `project:MapGen/minha_pipeline.json`

---

## 4. Configurar a cena

### 4.1 TileRegistry (Data Asset)

1. Content Browser → clique direito → **Miscellaneous → Data Asset → TileRegistry**
2. Configure os mapeamentos:

**Mesh Mappings** — tiles renderizados como Static Mesh instanciado (chão, borda, vegetação):

| Campo | Descrição |
|-------|-----------|
| `TileType` | String do tipo definida na pipeline JSON (ex: `"floor"`, `"path"`) |
| `Mesh` | Static Mesh a instanciar |
| `MaterialOverride` | Material opcional |

**Actor Mappings** — tiles que spawnam Actors/Blueprints (estruturas, entidades):

| Campo | Descrição |
|-------|-----------|
| `TileType` | String do tipo definida na pipeline JSON (ex: `"zone_a"`) |
| `Variants` | Um ou mais Blueprints — escolhido aleatoriamente por seed |
| `bSpawnPerCell` | `true` = um actor por célula (vegetação, entidades individuais). `false` = um actor por região contígua (estruturas) |
| `bIsSpawnPoint` | `true` = não spawna actor, apenas registra a posição em `GetSpawnPositions()`. Não precisa de Variants |

### 4.2 Actors na cena

Adicione via **Window → Place Actors** (Shift+1), pesquise pelo nome:

**AMapBuilder**
- `Registry` → arraste o TileRegistry criado
- `TileSize` → tamanho do tile em unidades Unreal (padrão: 100)
- `BaseZ` → altura Z base do mapa

**AMapGeneratorRuntime**
- `Builder` → arraste o AMapBuilder da cena
- `ExecutablePath` → caminho para o `mapgen.exe` (padrão: `MapGen/mapgen.exe`)
- `PipelineConfigPath` → caminho para o JSON da pipeline (padrão: `MapGen/example_pipeline.json`)
- `Seed` → 0 para aleatório, ou valor fixo para mapa determinístico

---

## 5. Gerar o mapa

O `AMapGeneratorRuntime` gera automaticamente no `BeginPlay`. Para controlar manualmente via Blueprint:

```
Event BeginPlay → MapGeneratorRuntime → Generate
```

Para seed específica:
```
MapGeneratorRuntime → GenerateWithSeed (OverrideSeed: 42)
```

---

## 6. Spawn do player

Marque o tile de spawn com `bIsSpawnPoint = true` no TileRegistry (sem Variants). Após a geração, leia as posições no Level Blueprint:

```
Event BeginPlay
  → MapGeneratorRuntime → Generate
  → MapBuilder → GetSpawnPositions → Get [0]
  → PlayerCharacter → SetActorLocation
```

---

## Coordenadas

O mapa é gerado em X+ e Y+ a partir da posição do `AMapBuilder`. O tile (Col=0, Row=0) fica à frente e à direita do actor.

Go usa (0,0) no canto superior esquerdo com Row crescendo para baixo → Unreal: `X = (Col + 0.5) * TileSize`, `Y = (Row + 0.5) * TileSize`.
