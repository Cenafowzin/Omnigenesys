using System;
using System.Collections.Generic;
using System.Text;
using UnityEngine;
using UnityEngine.Tilemaps;

namespace Omnigenesys
{
    /// <summary>
    /// Builds a map in the scene: tile layers into Tilemaps (one
    /// SetTilesBlock per layer), tile prefabs at their cells, object prefabs
    /// on their boxes. Positions go through the Grid, so its cell size,
    /// gap and swizzle apply: a Grid with Cell Swizzle XZY lays the map on
    /// the ground of a 3D scene, the map's levels (z) going up.
    ///
    /// The map's y grows downwards (row 0 at the top) and Unity's upwards,
    /// so map row r becomes cell row -1 - r: the map's top-left corner sits
    /// at the Grid's origin and the map grows down and right, and growing
    /// the map never moves what was already there.
    /// </summary>
    public sealed class OmniMapBuilder : MonoBehaviour
    {
        [Serializable]
        public sealed class LayerBinding
        {
            [Tooltip("A tile layer's name in the pipeline.")]
            public string layer;
            public Tilemap tilemap;
        }

        [Tooltip("What each tile name and object type becomes.")]
        public OmniMapping mapping;
        [Tooltip("The Grid the Tilemaps are children of. Empty: one cell is one unit, from this object.")]
        public Grid grid;
        [Tooltip("Which Tilemap paints each tile layer. A layer left out is not painted (its tile prefabs still come).")]
        public LayerBinding[] layers = Array.Empty<LayerBinding>();
        [Tooltip("Parent of the prefabs made. Empty: this object.")]
        public Transform prefabParent;
        [Tooltip("Put the map's bottom-left corner at the origin (map row r at cell row height-1-r) instead of its top-left.")]
        public bool anchorBottom;
        [Tooltip("Log the names in the map that the mapping does not cover, once per build.")]
        public bool reportUnmapped = true;

        readonly List<GameObject> _made = new List<GameObject>();
        OmniMap _map;

        /// <summary>The map built last, or null.</summary>
        public OmniMap Map => _map;

        /// <summary>Clears what the last build made, then builds <paramref name="map"/>.</summary>
        public void Build(OmniMap map)
        {
            if (map == null)
                throw new ArgumentNullException(nameof(map));
            if (mapping == null)
                throw new InvalidOperationException($"{name}: OmniMapBuilder has no mapping");
            Clear();
            _map = map;
            var unmapped = new SortedSet<string>();
            var unbound = new SortedSet<string>();

            foreach (var layer in map.Layers)
            {
                if (layer.Kind != OmniLayerKind.Tiles)
                    continue;
                var tilemap = TilemapOf(layer.Name);
                if (tilemap == null)
                    unbound.Add(layer.Name);
                PaintAndPlace(map, layer, tilemap, unmapped);
            }
            var spawned = SpawnObjects(map, unmapped);
            foreach (var o in spawned.Values)
                foreach (var hook in o.GetComponents<IOmniSpawned>())
                    hook.OnOmniSpawned(o, map, spawned);

            if (reportUnmapped && (unmapped.Count > 0 || unbound.Count > 0))
            {
                var sb = new StringBuilder($"[Omnigenesys] {name}:");
                if (unmapped.Count > 0)
                    sb.Append(" not in the mapping: ").Append(string.Join(", ", unmapped)).Append('.');
                if (unbound.Count > 0)
                    sb.Append(" Tile layers with no Tilemap: ").Append(string.Join(", ", unbound)).Append('.');
                Debug.Log(sb.ToString(), this);
            }
        }

        /// <summary>Clears the Tilemaps bound here and destroys the prefabs the last build made.</summary>
        public void Clear()
        {
            foreach (var b in layers)
                if (b.tilemap != null)
                    b.tilemap.ClearAllTiles();
            foreach (var go in _made)
            {
                if (go == null)
                    continue;
                if (Application.isPlaying)
                    Destroy(go);
                else
                    DestroyImmediate(go);
            }
            _made.Clear();
            _map = null;
        }

        Tilemap TilemapOf(string layer)
        {
            foreach (var b in layers)
                if (b.layer == layer)
                    return b.tilemap;
            return null;
        }

        /// <summary>The Unity cell row of map row <paramref name="y"/> (absolute).</summary>
        int Row(OmniMap map, int y) => anchorBottom ? map.OriginY + map.Height - 1 - (y - map.OriginY) : -1 - y;

        /// <summary>
        /// A point in the map's continuous coordinates (a cell's corner is a
        /// whole number; y down) as a position in the world.
        /// </summary>
        public Vector3 WorldOf(float x, float y, float z)
        {
            var map = _map ?? throw new InvalidOperationException("no map built yet");
            // A cell's corner at map y is the bottom of Unity row Row(y), i.e. its top is Row(y) + 1.
            float uy = anchorBottom ? map.OriginY + map.Height - (y - map.OriginY) : -y;
            var cell = new Vector3(x, uy, z);
            if (grid == null)
                return transform.TransformPoint(cell);
            return grid.transform.TransformPoint(grid.CellToLocalInterpolated(cell));
        }

        void PaintAndPlace(OmniMap map, OmniLayer layer, Tilemap tilemap, SortedSet<string> unmapped)
        {
            int w = map.Width, h = map.Height, d = map.Depth;
            // Each tile id's rule, once.
            var rules = new OmniMapping.TileRule[map.Tiles.Count];
            var variantsOf = new uint[map.Tiles.Count][];
            for (int t = 1; t < rules.Length; t++)
            {
                rules[t] = mapping.Tile(map.Tiles[t]);
                if (rules[t] == null)
                    continue;
                if (!string.IsNullOrEmpty(rules[t].variantLayer))
                {
                    var v = map.Layer(rules[t].variantLayer);
                    if (v == null || v.Kind != OmniLayerKind.Index)
                        throw new InvalidOperationException(
                            $"the mapping of tile '{map.Tiles[t]}' picks variants by '{rules[t].variantLayer}', which is not an index layer of the map");
                    variantsOf[t] = v.Indices;
                }
            }

            var ids = layer.TileIds;
            TileBase[] block = tilemap != null ? new TileBase[ids.Length] : null;
            var seen = new bool[rules.Length];
            for (int z = 0; z < d; z++)
                for (int r = 0; r < h; r++)
                {
                    // Unity rows grow upwards: either anchoring puts map row r
                    // at block row h-1-r (counted from the block's lowest row).
                    int j = h - 1 - r;
                    for (int x = 0; x < w; x++)
                    {
                        int i = (z * h + r) * w + x;
                        int t = ids[i];
                        if (t == 0)
                            continue;
                        var rule = rules[t];
                        if (rule == null)
                        {
                            if (!seen[t])
                                unmapped.Add($"tile '{map.Tiles[t]}'");
                            seen[t] = true;
                            continue;
                        }
                        uint variant = variantsOf[t]?[i] ?? 0;
                        if (block != null && rule.tiles.Length > 0)
                            block[(z * h + j) * w + x] = Pick(rule.tiles, variant, map.Tiles[t]);
                        if (rule.prefabs.Length > 0)
                        {
                            var prefab = Pick(rule.prefabs, variant, map.Tiles[t]);
                            if (prefab != null)
                                Make(prefab, WorldOf(map.OriginX + x + 0.5f, map.OriginY + r + 0.5f, map.OriginZ + z));
                        }
                    }
                }
            if (block == null)
                return;
            int top = Row(map, map.OriginY);
            int bottom = Row(map, map.OriginY + h - 1);
            var bounds = new BoundsInt(map.OriginX, Math.Min(top, bottom), map.OriginZ, w, h, d);
            tilemap.SetTilesBlock(bounds, block);
            tilemap.CompressBounds();
        }

        /// <summary>Variant <paramref name="i"/> of <paramref name="items"/>; past the end is an error, not a wrap.</summary>
        static T Pick<T>(T[] items, uint i, string what)
        {
            if (i >= items.Length)
                throw new InvalidOperationException(
                    $"'{what}' asks for variant {i}, but the mapping has {items.Length}: add them, or lower the pipeline's variant count");
            return items[i];
        }

        Dictionary<long, OmniObject> SpawnObjects(OmniMap map, SortedSet<string> unmapped)
        {
            var spawned = new Dictionary<long, OmniObject>();
            foreach (var o in map.Objects)
            {
                var rule = mapping.Object(o.Type);
                if (rule == null || rule.prefabs.Length == 0)
                {
                    unmapped.Add($"object '{o.Type}'");
                    continue;
                }
                uint variant = 0;
                if (!string.IsNullOrEmpty(rule.variantProp))
                {
                    if (!o.Props.TryGetValue(rule.variantProp, out var p) || p.Type != OmniValueType.Int || p.AsInt() < 0)
                        throw new InvalidOperationException(
                            $"object {o.Id} ('{o.Type}') has no whole prop '{rule.variantProp}' to pick its prefab");
                    variant = (uint)p.AsInt();
                }
                var prefab = Pick(rule.prefabs, variant, o.Type);
                if (prefab == null)
                    continue;
                var go = Make(prefab, PivotOf(o, rule.pivot));
                go.name = $"{prefab.name} #{o.Id}";
                // Not `??`: Unity's destroyed or missing objects are not C# null.
                if (!go.TryGetComponent(out OmniObject omni))
                    omni = go.AddComponent<OmniObject>();
                omni.Data = o;
                spawned[o.Id] = omni;
            }
            return spawned;
        }

        Vector3 PivotOf(OmniObjectData o, OmniPivot pivot)
        {
            float cx = o.X + o.Width / 2f, cy = o.Y + o.Height / 2f;
            switch (pivot)
            {
                case OmniPivot.Corner: return WorldOf(o.X, o.Y, o.Z);
                case OmniPivot.BottomCenter: return WorldOf(cx, o.Y + o.Height, o.Z);
                case OmniPivot.BaseCenter: return WorldOf(cx, cy, o.Z);
                default: return WorldOf(cx, cy, o.Z + (o.Depth > 1 ? o.Depth / 2f : 0f));
            }
        }

        GameObject Make(GameObject prefab, Vector3 at)
        {
            var go = Instantiate(prefab, at, prefab.transform.rotation, prefabParent != null ? prefabParent : transform);
            _made.Add(go);
            return go;
        }
    }
}
