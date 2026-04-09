# Omnigenesys

Framework procedural genérico para geração de mapas em Go, agnóstico de engine. Define mapas via pipeline JSON e integra com qualquer engine através de adapters oficiais.

## Como funciona

```
Pipeline JSON → mapgen (Go) → Mapa JSON → Engine (Unreal / Unity / Godot)
```

O `mapgen` roda como subprocess chamado pela engine. Recebe uma configuração de pipeline, gera o mapa deterministicamente por seed e retorna o resultado como JSON.

## Adapters disponíveis

| Engine | Pasta | Status |
|--------|-------|--------|
| Unreal Engine 5 | `unreal-plugin/Omnigenesys/` | Funcional |
| Unity | `unity-package/` | Funcional |

## Build

Use o script `build.sh` para compilar e distribuir o `mapgen` para os adapters automaticamente:

```bash
./build.sh            # Windows (padrão)
./build.sh linux      # Linux
./build.sh mac        # macOS
./build.sh all        # todas as plataformas
```

O binário é copiado automaticamente para:
- `unreal-plugin/Omnigenesys/Content/MapGen/`
- `unity-package/StreamingAssets~/MapGen/`

## Integração Unreal Engine

Copie `unreal-plugin/Omnigenesys/` para `<Projeto>/Plugins/Omnigenesys/` e consulte `INSTALL.md` dentro da pasta.

## Integração Unity

Instale via **Package Manager → Add package from disk** selecionando `unity-package/package.json`.

## Pipelines

Mapas são definidos por pipelines JSON. Veja [PIPELINE.md](PIPELINE.md) para a referência completa de operators, conditions e exemplos.

## Licença

Uso pessoal e educacional gratuito. Uso comercial requer licença — veja [LICENSE](LICENSE).
