# Omnigenesys para Godot

Monta um mapa do Omnigenesys (`.omap`, gerado no app web ou pela CLI) numa cena 2D: tiles nos `TileMapLayer`, cenas nas células e nos objetos. Só GDScript: roda em qualquer plataforma do Godot (web inclusive) e instala copiando uma pasta.

Godot 4.3 ou mais novo. Gerar o mapa dentro do jogo (como no Unity) vem depois, numa GDExtension; por enquanto o mapa chega pronto.

## Instalar

1. Copie `adapters/godot/addons/omnigenesys` para a pasta `addons/` do seu projeto (fica `res://addons/omnigenesys/`).
2. **Project → Project Settings → Plugins** → marque **Omnigenesys**.

Com o plugin ligado, um arquivo `.omap` no projeto é importado como **OmniMapFile**. Um mapa que o leitor não entende falha na importação, com o motivo no painel de saída — não no meio do jogo. Se o `.omap` já estava no projeto antes de ligar o plugin, clique nele com o botão direito → **Reimport**.

## O mapa

No app web, **Exportar MAPA** baixa `mapa-<semente>.omap`. Pela CLI:

```
mapgen generate pipeline.json --seed 512 -o mapa.omap
```

Coloque o arquivo em qualquer lugar do projeto.

## Montar a cena

1. **Mapeamento** — no FileSystem, botão direito → **New Resource… → OmniMapping**. Em cada item:
   - **Tiles** (`OmniTileRule`): o nome do tile na pipeline →
     - um tile do TileSet: **Source Id**, **Atlas Coords** e **Alternative** (pintado no `TileMapLayer` da layer), e/ou
     - **Scenes**: uma cena por célula, no centro dela.
     - Com mais de uma variante (várias coordenadas de atlas ou várias cenas), **Variant Layer** é a layer de variantes do mapa (a que o passo `PickVariant` escreve): o número da célula escolhe — 0 a primeira, 1 a segunda… `(-1, -1)` ou cena vazia não põe nada.
   - **Objects** (`OmniObjectRule`): o tipo do objeto → **Scenes**, com o **Pivot** (centro da caixa, canto, ou meio da base — o pé de um sprite). Com mais de uma cena, **Variant Prop** é a prop inteira que escolhe (a que o `PickObjectVariant` escreve).
2. **Builder** — um nó **OmniMapBuilder** (Node2D) com:
   - um `TileMapLayer` filho **com o nome de cada layer de tiles** da pipeline (o nó `terreno` pinta a layer `terreno`), todos com o seu TileSet e na posição (0, 0) do builder;
   - **Map File** = o `.omap`, **Mapping** = o mapeamento.
3. Dê play.

Os nomes são os da pipeline, exatos. O que o mapa tem e o mapeamento não cobre aparece na saída uma vez por montagem — nada some em silêncio. Variante pedida além das que o mapeamento tem é erro; mapeamento com mais variantes do que o mapa usou vira aviso.

### Opções do builder

- **Tile Layers**: em vez dos filhos, a lista de `TileMapLayer` (cada um ainda pinta a layer com o nome do nó).
- **Spawn Parent**: onde as cenas entram (vazio: o próprio builder). Útil para um nó com **Y Sort** ligado, que desenha na frente o que está mais embaixo.
- **Cell Size**: tamanho da célula em pixels. Zero: o `tile_size` do TileSet.
- **Build On Ready**: monta o Map File ao entrar na cena. Desligado, chame `build_file(arquivo)` ou `build(mapa)` quando quiser.

## Coordenadas

O mapa tem x para a direita e y para baixo, igual ao Godot 2D: a célula (x, y) do mapa é a célula (x, y) do `TileMapLayer`, sem virar nada. As cenas vão para `(x, y) × tamanho da célula` no espaço do builder. Células retangulares; isométrico e mapas 3D (com níveis) ainda não.

## Código

```gdscript
# Montar outro mapa, por exemplo um .omap escolhido pelo jogador:
var mapa := OmniMap.from_omap(FileAccess.get_file_as_string("user://mapa.omap"))
if mapa == null:
    push_error(OmniMap.last_error)
else:
    $OmniMapBuilder.build(mapa)

# Depois de cada montagem:
$OmniMapBuilder.built.connect(func(mapa: OmniMap): print(mapa.seed_text))
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

O primeiro importa o projeto uma vez (para o Godot conhecer as classes do addon); o segundo lê o mapa da Lost Fields em `tests/lost_fields_512.omap`, recusa arquivos quebrados e monta numa cena, conferindo tile por tile e cena por cena. Termina com `N passed, 0 failed`.
