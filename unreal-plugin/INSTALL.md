# Omnigenesys — Instalação do Plugin

## 1. Instalar o plugin

Copie a pasta `Omnigenesys` (esta pasta inteira) para:

```
<SeuProjeto>/Plugins/Omnigenesys/
```

> Se a pasta `Plugins/` não existir no projeto, crie-a.

## 2. Compilar

Feche o Unreal Editor (se aberto), clique direito no `.uproject` → **Generate Visual Studio Project Files**, depois compile via Visual Studio ou clique no martelo no Editor.

O Unreal detecta o plugin automaticamente — não é necessário editar nenhum arquivo do projeto.

## 3. Preparar os binários

Compile o `mapgen.exe` a partir do framework Go e coloque **dentro do plugin**:

```bash
cd cmd/mapgen
GOOS=windows GOARCH=amd64 go build -o mapgen.exe .
```

```
Plugins/Omnigenesys/Content/MapGen/
├── mapgen.exe
└── cornfield_pipeline.json   ← ou sua pipeline customizada
```

> Para usar arquivos do seu próprio projeto em vez dos do plugin, prefixe o caminho com `project:` nos campos do `AMapGeneratorRuntime`:
> - `ExecutablePath` = `project:MapGen/mapgen.exe`
> - `PipelineConfigPath` = `project:MapGen/minha_pipeline.json`

## 4. Configurar a cena

### TileRegistry (Data Asset)
1. Content Browser → clique direito → **Miscellaneous → Data Asset → TileRegistry**
2. Mapeie tipos de tile para Static Meshes e/ou Actor Blueprints

### Actors na cena
1. Adicione um `AMapBuilder` → atribua o `TileRegistry` e defina `TileSize`
2. Adicione um `AMapGeneratorRuntime` → arraste o `AMapBuilder`, defina `PipelineConfigPath` e `ExecutablePath`

### Disparar a geração
Blueprint: `Event BeginPlay → MapGeneratorRuntime → Generate`

C++:
```cpp
MapGenRuntime->Generate();
// ou com seed específica:
MapGenRuntime->GenerateWithSeed(42);
```
