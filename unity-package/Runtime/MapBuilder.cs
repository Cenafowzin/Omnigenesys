using System;
using System.Collections.Generic;
using Newtonsoft.Json;
using UnityEngine;
using UnityEngine.Tilemaps;

namespace Omnigenesys
{
    /// <summary>
    /// Constrói o mapa na cena Unity a partir do JSON gerado pelo framework Go.
    /// Tiles mapeados em TileRegistry.tiles   → Tilemap.SetTile()
    /// Tiles mapeados em TileRegistry.prefabs → Instantiate() em prefabRoot (com variantes aleatórias)
    ///
    /// Coordenadas: Go usa (0,0) top-left com Y crescendo para baixo.
    /// Unity Tilemap usa Y crescendo para cima → conversão: unityY = -goY
    /// </summary>
    public class MapBuilder : MonoBehaviour
    {
        [Serializable]
        public class LayerMapping
        {
            [Tooltip("Nome do layer como definido no pipeline JSON (ex: 'terrain')")]
            public string layerName;
            [Tooltip("Tilemap filho do Grid na cena")]
            public Tilemap tilemap;
        }

        [SerializeField] private TileRegistry registry;
        [SerializeField] private LayerMapping[] layers;
        [SerializeField] private Transform prefabRoot;

        // ── DTOs ─────────────────────────────────────────────────────────────

        private class MapData
        {
            public int width;
            public int height;
            public long seed;
            public List<LayerData> layers;
        }

        private class LayerData
        {
            public string name;
            public List<List<CellData>> cells;
        }

        private class CellData
        {
            public string type;
            public Dictionary<string, object> metadata;
        }

        // ── API pública ───────────────────────────────────────────────────────

        /// <param name="deterministicVariants">
        /// true  → variantes determinísticas (mesma seed = mesmo visual sempre) <br/>
        /// false → variantes aleatórias a cada geração independente da seed
        /// </param>
        public void Build(string mapJson, bool deterministicVariants = true)
        {
            var map = JsonConvert.DeserializeObject<MapData>(mapJson);
            var rng = deterministicVariants
                ? new System.Random((int)(map.seed ^ (map.seed >> 32)))
                : new System.Random();
            Clear();

            var layerMap = new Dictionary<string, Tilemap>();
            foreach (var m in layers)
                if (!string.IsNullOrEmpty(m.layerName) && m.tilemap != null)
                    layerMap[m.layerName] = m.tilemap;

            foreach (var layer in map.layers)
            {
                layerMap.TryGetValue(layer.name, out var tilemap);

                for (int y = 0; y < map.height; y++)
                for (int x = 0; x < map.width; x++)
                {
                    var cell = layer.cells[y][x];
                    if (string.IsNullOrEmpty(cell.type)) continue;

                    if (tilemap != null)
                    {
                        var tile = registry.GetTile(cell.type);
                        if (tile != null)
                            tilemap.SetTile(new Vector3Int(x, -y, 0), tile);
                    }

                    var prefab = registry.GetPrefab(cell.type, rng);
                    if (prefab != null)
                    {
                        var pos = new Vector3(x + 0.5f, -y + 0.5f, 0f);
                        Instantiate(prefab, pos, Quaternion.identity, prefabRoot);
                    }
                }
            }
        }

        public void Clear()
        {
            foreach (var m in layers)
                m.tilemap?.ClearAllTiles();

            if (prefabRoot == null) return;
            for (int i = prefabRoot.childCount - 1; i >= 0; i--)
                Destroy(prefabRoot.GetChild(i).gameObject);
        }
    }
}
