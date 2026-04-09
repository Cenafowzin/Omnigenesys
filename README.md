# Omnigenesys

Framework procedural genérico para geração de mapas em Go, agnóstico de engine. Exporta mapas via JSON e integra com qualquer engine através de adapters oficiais.

## Como funciona

O Omnigenesys roda como um subprocess chamado pela engine. Recebe uma pipeline JSON via stdin, gera o mapa e retorna o resultado via stdout.

```
Pipeline JSON → mapgen → Mapa JSON → Engine (Unity / Unreal / Godot)
```

## Integração com Engines

### Unity
Instale via **Package Manager → Add package from disk** selecionando `unity-package/package.json`.
O `mapgen.exe` é copiado para `StreamingAssets/` automaticamente.

### Unreal Engine
Copie a pasta `unreal-plugin/` para `<Projeto>/Plugins/Omnigenesys/` e compile.
Consulte `unreal-plugin/INSTALL.md` para o setup completo.

## Build

```bash
bash build.sh           # Windows (padrão)
bash build.sh all       # Windows + Linux + Mac
```

Gera o `mapgen.exe` e distribui para os adapters automaticamente.

## Licença

Uso pessoal e educacional gratuito. Uso comercial requer licença — veja [LICENSE](LICENSE).
