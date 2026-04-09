using System;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.Tilemaps;

namespace Omnigenesys
{
    /// <summary>
    /// Mapeia os tile types do framework (strings) para assets Unity.
    /// Crie um asset via: Assets → Create → Omnigenesys → TileRegistry
    ///
    /// Tile types que mapeiam para TileBase são colocados em Tilemaps.
    /// Tile types que mapeiam para Prefab são instanciados como GameObjects.
    /// </summary>
    [CreateAssetMenu(fileName = "TileRegistry", menuName = "Omnigenesys/TileRegistry")]
    public class TileRegistry : ScriptableObject
    {
        [Serializable]
        public class TileMapping
        {
            [Tooltip("Tile type exatamente como definido no pipeline JSON (ex: 'mato_enraizado')")]
            public string tileType;
            public TileBase tile;
        }

        [Serializable]
        public class PrefabMapping
        {
            [Tooltip("Tile type exatamente como definido no pipeline JSON (ex: 'estrutura_loja')")]
            public string tileType;
            [Tooltip("Um prefab único, ou múltiplas variantes — o MapBuilder sorteia aleatoriamente")]
            public GameObject[] variants;
        }

        [Header("Tiles → TileBase assets")]
        public TileMapping[] tiles;

        [Header("Tipos → Prefabs (suporta variantes)")]
        public PrefabMapping[] prefabs;

        // ── lookups (lazy) ────────────────────────────────────────────────────

        private Dictionary<string, TileBase> _tiles;
        private Dictionary<string, GameObject[]> _prefabs;

        public TileBase GetTile(string type)
        {
            BuildIfNeeded();
            return _tiles.TryGetValue(type, out var t) ? t : null;
        }

        /// <summary>Retorna uma variante aleatória para o tipo, ou null se não mapeado.</summary>
        public GameObject GetPrefab(string type, System.Random rng)
        {
            BuildIfNeeded();
            if (!_prefabs.TryGetValue(type, out var variants) || variants.Length == 0) return null;
            return variants[rng.Next(variants.Length)];
        }

        private void BuildIfNeeded()
        {
            if (_tiles != null) return;

            _tiles = new Dictionary<string, TileBase>();
            foreach (var m in tiles)
                if (!string.IsNullOrEmpty(m.tileType) && m.tile != null)
                    _tiles[m.tileType] = m.tile;

            _prefabs = new Dictionary<string, GameObject[]>();
            foreach (var m in prefabs)
                if (!string.IsNullOrEmpty(m.tileType) && m.variants != null && m.variants.Length > 0)
                    _prefabs[m.tileType] = m.variants;
        }

        private void OnValidate() => _tiles = null;
    }
}
