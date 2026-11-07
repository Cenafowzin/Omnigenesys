# Omnigenesys para Godot

Gera o mapa de uma pipeline do Omnigenesys **enquanto o jogo roda**, com semente aleatória ou escolhida (mesma semente → mesmo mapa da CLI, do app web e do Unity), ou carrega um mapa pronto (`.omap`), e monta numa cena 2D: tiles nos `TileMapLayer`, cenas nas células e nos objetos.

Godot 4.3 ou mais novo. Gerar no jogo: Windows x64 por enquanto. Nas outras plataformas o addon carrega e monta `.omap`.

## Instalar

1. Baixe `omnigenesys-godot-<versão>.zip` da página de [releases](https://github.com/Cenafowzin/Omnigenesys/releases).
2. No Godot: aba **AssetLib** (no topo) → **Import…** → escolha o zip → **Install**.
3. **Project → Project Settings → Plugins** → marque **Omnigenesys**.
4. **Project → Reload Current Project**, para o Godot carregar a biblioteca nativa.

Não precisa de Rust. Sem o zip, copiar a pasta `adapters/godot/addons/omnigenesys` para `addons/` do projeto dá no mesmo.

### Quem mexe no core

A `omnigenesys_godot.dll` é commitada e precisa acompanhar o código: depois de mudar o core ou `crates/omnigenesys-godot`, rode `sh adapters/godot/build-native.sh` (`JOBS=1` em PC com pouca memória). `python adapters/godot/package.py` faz o zip e recusa uma biblioteca mais velha que o código.

## Montar a cena

1. **Pipeline** — o `.json` que o app web salva (**Salvar PIPELINE**), em qualquer lugar do projeto.
2. **Mapeamento** — no FileSystem, botão direito → **Create New → Resource… → OmniMapping**. Em cada item:
   - **Tiles** (`OmniTileRule`): o nome do tile na pipeline →
     - um tile do TileSet: **Source Id**, **Atlas Coords** e **Alternative** (pintado no `TileMapLayer` da layer), e/ou
     - **Scenes**: uma cena por célula, no centro dela.
     - Com mais de uma variante (várias coordenadas de atlas ou várias cenas), **Variant Layer** é a layer de variantes do mapa (a que o passo `PickVariant` escreve): o número da célula escolhe — 0 a primeira, 1 a segunda… `(-1, -1)` ou cena vazia não põe nada.
   - **Objects** (`OmniObjectRule`): o tipo do objeto → **Scenes**, com o **Pivot** (centro da caixa, canto, ou meio da base — o pé de um sprite). Com mais de uma cena, **Variant Prop** é a prop inteira que escolhe (a que o `PickObjectVariant` escreve).
3. **Builder** — um nó **OmniMapBuilder** (Node2D) com **Mapping** = o mapeamento e um `TileMapLayer` filho **com o nome de cada layer de tiles** da pipeline (o nó `terreno` pinta a layer `terreno`), todos com o seu TileSet e na posição (0, 0) do builder.
4. **Gerador** — um nó **OmniGenerator** com **Pipeline** = o `.json`, **Builder** = o nó acima e o **Seed Mode**:
   - **Random**: um mapa novo a cada play (o padrão);
   - **Fixed**: sempre a semente de **Fixed Seed**;
   - **From File**: a semente escrita na pipeline.
5. Dê play.

Os nomes são os da pipeline, exatos. O que o mapa tem e o mapeamento não cobre aparece na saída uma vez por montagem — nada some em silêncio. Variante pedida além das que o mapeamento tem é erro; mapeamento com mais variantes do que o mapa usou vira aviso, quando há células ou objetos suficientes (10 por variante) para não ser só acaso.

### Gerador

- **Parameters**: muda params da pipeline por nome (um nome que a pipeline não declara é erro); o que ficar de fora mantém o valor do arquivo.
- **Generate On Start**: desligado, nada acontece até você chamar `generate()`.
- **Fallback Map**: um `.omap` montado onde a biblioteca nativa não existe (web, por enquanto).

### Mapa pronto, sem gerador

Com o plugin ligado, um `.omap` no projeto é importado como **OmniMapFile**; um mapa que o leitor não entende falha na importação, com o motivo. Ponha no **Map File** do builder, que monta sozinho (**Build On Ready**). No app web, **Exportar MAPA** baixa `mapa-<semente>.omap`; pela CLI, `mapgen generate pipeline.json --seed 512 -o mapa.omap`.

### Builder

- **Tile Layers**: em vez dos filhos, a lista de `TileMapLayer` (cada um ainda pinta a layer com o nome do nó).
- **Spawn Parent**: onde as cenas entram (vazio: o próprio builder). Útil para um nó com **Y Sort** ligado, que desenha na frente o que está mais embaixo.
- **Cell Size**: tamanho da célula em pixels. Zero: o `tile_size` do TileSet.

## Coordenadas

O mapa tem x para a direita e y para baixo, igual ao Godot 2D: a célula (x, y) do mapa é a célula (x, y) do `TileMapLayer`, sem virar nada. As cenas vão para `(x, y) × tamanho da célula` no espaço do builder. Células retangulares; isométrico e mapas 3D (com níveis) ainda não.

## Código

```gdscript
# Um mapa novo, ou o de uma semente (do menu, do save, da run diária).
# Com await, a linha seguinte já roda com o mapa na cena.
var mapa: OmniMap = await $OmniGenerator.generate()
var outro: OmniMap = await $OmniGenerator.generate(512)
if mapa == null:
    return  # o motivo já está na saída
print(mapa.seed_text)  # a semente usada: guarde para repetir o mapa

# Ou reagir a cada mapa:
$OmniGenerator.built.connect(func(m: OmniMap): print(m.objects.size()))

# Guardar a última semente como .omap:
$OmniGenerator.save_last_seed("user://mapa.omap")

# Montar um .omap em tempo de jogo:
$OmniMapBuilder.build(OmniMap.from_omap(FileAccess.get_file_as_string("user://mapa.omap")))
```

- Numa cena de objeto, um método `_omni_spawned(objeto, mapa, criados)` é chamado depois que todos os objetos existem: `objeto` é o `OmniMap.MapObject` (id, tipo, caixa, props), `criados` é id → nó (uma prop do tipo `object` guarda o id de outro objeto).
- Todo nó criado para um objeto tem o objeto na meta `omni_object`.
- `OmniMap`: `width`, `height`, `depth`, `origin`, `seed_text` (texto: a semente vai até 2⁶⁴ − 1, além do `int` do Godot), `tiles`, `layers` (com `ids` ou `values`), `objects`, `layer(nome)`, `object_by_id(id)`, `cell_index(x, y)`.

## Testes

Esta pasta é um projeto Godot de teste. Com o Godot no PATH:

```
godot --headless --path adapters/godot --import
godot --headless --path adapters/godot -s tests/run_tests.gd
```

O primeiro importa o projeto uma vez (para o Godot conhecer as classes do addon); o segundo lê o mapa da Lost Fields em `tests/lost_fields_512.omap`, recusa arquivos quebrados, gera a mesma semente dentro do Godot e confere que o mapa é igual ao da CLI, e monta numa cena, conferindo tile por tile e cena por cena. Termina com `N passed, 0 failed`. Num projeto sem a pasta `.godot`, o comando `--import` pode terminar com *segmentation fault* ao sair, depois de importar tudo: o próximo comando funciona normalmente, e abrir o projeto no editor não tem esse problema. Se a pipeline da Lost Fields mudar, refaça `tests/lost_fields.json` e `tests/lost_fields_512.omap` juntos (o comando está em `tests/run_tests.gd`).
