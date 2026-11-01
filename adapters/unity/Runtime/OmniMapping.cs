using System;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.Tilemaps;

namespace Omnigenesys
{
    /// <summary>Where on an object's box its prefab sits.</summary>
    public enum OmniPivot
    {
        /// <summary>The middle of the box.</summary>
        Center,
        /// <summary>The outer corner of the box's first cell (its top-left in the map).</summary>
        Corner,
        /// <summary>The middle of the box's bottom edge on screen: a 2D sprite's feet.</summary>
        BottomCenter,
        /// <summary>The middle of the box's floor (its lowest level): a 3D model's feet.</summary>
        BaseCenter,
    }

    /// <summary>
    /// What each name in a map becomes in the scene: a tile name becomes a
    /// TileBase (painted into the layer's Tilemap) and/or a prefab (one per
    /// cell); an object type becomes a prefab on its box. Names are the
    /// pipeline's, exactly. Create one with Assets → Create → Omnigenesys →
    /// Mapping.
    /// </summary>
    [CreateAssetMenu(fileName = "OmniMapping", menuName = "Omnigenesys/Mapping")]
    public sealed class OmniMapping : ScriptableObject
    {
        [Serializable]
        public sealed class TileRule
        {
            [Tooltip("The tile's name in the pipeline.")]
            public string tile;
            [Tooltip("Painted into the Tilemap of the layer the tile is in. More than one: the variant layer picks. An empty slot paints nothing.")]
            public TileBase[] tiles = Array.Empty<TileBase>();
            [Tooltip("Placed at the centre of every cell with this tile. More than one: the variant layer picks. An empty slot places nothing.")]
            public GameObject[] prefabs = Array.Empty<GameObject>();
            [Tooltip("An index layer of the map (e.g. one PickVariant writes) whose number at the cell picks the variant. Empty: the first.")]
            public string variantLayer;
        }

        [Serializable]
        public sealed class ObjectRule
        {
            [Tooltip("The object's type in the pipeline.")]
            public string type;
            [Tooltip("More than one: an int prop picks (variant prop). An empty slot places nothing.")]
            public GameObject[] prefabs = Array.Empty<GameObject>();
            public OmniPivot pivot = OmniPivot.Center;
            [Tooltip("An int prop of the object that picks the prefab. Empty: the first.")]
            public string variantProp;
        }

        public TileRule[] tiles = Array.Empty<TileRule>();
        public ObjectRule[] objects = Array.Empty<ObjectRule>();

        Dictionary<string, TileRule> _tiles;
        Dictionary<string, ObjectRule> _objects;

        public TileRule Tile(string name)
        {
            Index();
            return _tiles.TryGetValue(name, out var r) ? r : null;
        }

        public ObjectRule Object(string type)
        {
            Index();
            return _objects.TryGetValue(type, out var r) ? r : null;
        }

        void Index()
        {
            if (_tiles != null)
                return;
            _tiles = new Dictionary<string, TileRule>();
            foreach (var r in tiles)
                if (!string.IsNullOrEmpty(r.tile))
                    _tiles[r.tile] = r;
            _objects = new Dictionary<string, ObjectRule>();
            foreach (var r in objects)
                if (!string.IsNullOrEmpty(r.type))
                    _objects[r.type] = r;
        }

        void OnValidate()
        {
            _tiles = null;
            _objects = null;
        }
    }
}
