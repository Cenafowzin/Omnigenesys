# Omnigenesys para Unity

Gera o mapa de uma pipeline do Omnigenesys **enquanto o jogo roda** (biblioteca nativa, mesma semente → mesmo mapa da CLI e do app web) ou carrega um mapa pronto (`.omap`), e monta na cena: tiles em Tilemaps, prefabs nas células e nos objetos.

Unity 6 (6000.0) ou mais novo. Por enquanto, Windows x64 (editor e build).

## Instalar

No Unity: **Window → Package Manager → + → Install package from git URL…** e cole

```
https://github.com/Cenafowzin/Omnigenesys.git?path=/adapters/unity
```

A biblioteca nativa já vem no pacote; não precisa de Rust. Para fixar uma versão, acrescente `#<tag ou commit>` no fim da URL. Para mexer no pacote, clone o repositório e use **Install package from disk…** com `adapters/unity/package.json`.

### Quem mexe no core

A `omnigenesys.dll` é commitada (sem Git LFS, que o Package Manager não segue) e precisa acompanhar o código: depois de mudar o core ou `crates/omnigenesys-ffi`, rode `sh adapters/unity/Tools~/build-native.sh`, que recompila, copia a biblioteca e escreve o `.meta` de arquivo novo do pacote (um pacote vindo do git é só leitura: o Unity ignora asset sem `.meta`).

## Montar a cena

1. **Mapeamento** — *Assets → Create → Omnigenesys → Mapping*. Para cada nome da pipeline:
   - **Tiles**: o nome do tile → `TileBase` (pintado no Tilemap da layer) e/ou prefab (um por célula). Com mais de uma variante, uma **layer de variantes** do mapa (`kind: "index"`, a que o `PickVariant` escreve) escolhe qual, célula a célula.
   - **Objects**: o tipo do objeto → prefab, com o **pivot** (centro, canto, base no meio para sprite 2D, chão no meio para 3D). Com mais de um prefab, uma prop inteira do objeto escolhe.
2. **Builder** — um GameObject com `OmniMapBuilder`: o mapeamento, o `Grid`, e qual Tilemap pinta cada layer de tiles.
3. **Gerador** — `OmniGenerator`: a pipeline (o `.json`), o builder, a semente (do arquivo, fixa ou aleatória), os params. Com *Generate On Start*, o mapa sai no play.

O que o mapa tem e o mapeamento não cobre aparece no Console uma vez por montagem — nada some em silêncio.

## Coordenadas

O mapa tem x para a direita e y para baixo (linha 0 em cima). No Unity, a linha `r` do mapa vira a linha `-1 - r` do Tilemap: o canto de cima à esquerda do mapa fica na origem do Grid, e aumentar o mapa não move o que já estava lá. *Anchor Bottom* põe o canto de baixo na origem. Para um jogo 3D visto de cima, use um Grid com **Cell Swizzle XZY**: o mapa deita no chão e os níveis (z) sobem. Tamanho de célula, espaçamento e rotação vêm do Grid.

## Código

- `OmniNative.Generate(texto, semente, params, salt)` → `OmniMap` (de qualquer thread).
- `OmniMap.FromOmap(texto)` lê um `.omap`.
- Um componente num prefab que implemente `IOmniSpawned` recebe o seu objeto do mapa (`OmniObject.Data`: id, tipo, caixa, props) depois que todos existem — uma prop do tipo `object` aponta para outro.
- *Save last seed as .omap* (menu de contexto do `OmniGenerator`, no editor) guarda o mapa da última semente: para conferir, ou como `fallbackMap` em plataforma sem a biblioteca (WebGL por enquanto).

## Testes fora do Unity

`dotnet run -c Release --project "adapters/unity/Tests~/CoreTests"` compila o C# puro do pacote (`Runtime/Core`, `Runtime/Native`) em C# 9, como o Unity, e confere contra a biblioteca de verdade.
