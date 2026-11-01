// The map an engine receives, the same whether it came from the native
// library (OmniNative.Generate) or from a .omap file (OmniMap.FromOmap).
// Plain C#, no UnityEngine: it is tested outside Unity.
using System;
using System.Collections.Generic;

namespace Omnigenesys
{
    public enum OmniLayerKind
    {
        /// <summary>A tile id per cell; <see cref="OmniMap.Tiles"/> names them.</summary>
        Tiles = 0,
        /// <summary>A number per cell: height, density.</summary>
        Scalar = 1,
        /// <summary>A whole number per cell: a variant, a region.</summary>
        Index = 2,
    }

    /// <summary>One layer of cells. Cells go x fastest, then y, then z.</summary>
    public sealed class OmniLayer
    {
        public string Name { get; }
        public OmniLayerKind Kind { get; }
        /// <summary>Tile ids (Kind Tiles), else null.</summary>
        public ushort[] TileIds { get; }
        /// <summary>Whole numbers (Kind Index), else null.</summary>
        public uint[] Indices { get; }
        /// <summary>Numbers (Kind Scalar), else null.</summary>
        public float[] Values { get; }

        public OmniLayer(string name, ushort[] tileIds)
        {
            Name = name; Kind = OmniLayerKind.Tiles; TileIds = tileIds;
        }

        public OmniLayer(string name, uint[] indices)
        {
            Name = name; Kind = OmniLayerKind.Index; Indices = indices;
        }

        public OmniLayer(string name, float[] values)
        {
            Name = name; Kind = OmniLayerKind.Scalar; Values = values;
        }
    }

    public enum OmniValueType { Int, Float, Bool, String, Cell, Object, List }

    /// <summary>An object's property, with its type as the file says it.</summary>
    public sealed class OmniValue
    {
        public OmniValueType Type { get; }
        readonly long _int;
        readonly double _float;
        readonly bool _bool;
        readonly string _string;
        readonly int[] _cell;
        readonly IReadOnlyList<OmniValue> _list;

        OmniValue(OmniValueType type, long i = 0, double f = 0, bool b = false, string s = null,
            int[] cell = null, IReadOnlyList<OmniValue> list = null)
        {
            Type = type; _int = i; _float = f; _bool = b; _string = s; _cell = cell; _list = list;
        }

        public static OmniValue Int(long v) => new OmniValue(OmniValueType.Int, i: v);
        public static OmniValue Float(double v) => new OmniValue(OmniValueType.Float, f: v);
        public static OmniValue Bool(bool v) => new OmniValue(OmniValueType.Bool, b: v);
        public static OmniValue String(string v) => new OmniValue(OmniValueType.String, s: v);
        public static OmniValue Cell(int x, int y, int z) => new OmniValue(OmniValueType.Cell, cell: new[] { x, y, z });
        public static OmniValue ObjectRef(long id) => new OmniValue(OmniValueType.Object, i: id);
        public static OmniValue List(IReadOnlyList<OmniValue> v) => new OmniValue(OmniValueType.List, list: v);

        public long AsInt() => Type == OmniValueType.Int ? _int : throw Wrong(OmniValueType.Int);
        /// <summary>A float, or an int widened to one.</summary>
        public double AsFloat() => Type == OmniValueType.Float ? _float
            : Type == OmniValueType.Int ? _int : throw Wrong(OmniValueType.Float);
        public bool AsBool() => Type == OmniValueType.Bool ? _bool : throw Wrong(OmniValueType.Bool);
        public string AsString() => Type == OmniValueType.String ? _string : throw Wrong(OmniValueType.String);
        /// <summary>A cell (x, y, z) in the map's axes.</summary>
        public (int x, int y, int z) AsCell() => Type == OmniValueType.Cell
            ? (_cell[0], _cell[1], _cell[2]) : throw Wrong(OmniValueType.Cell);
        /// <summary>The id of another object (<see cref="OmniMap.ObjectById"/>).</summary>
        public long AsObject() => Type == OmniValueType.Object ? _int : throw Wrong(OmniValueType.Object);
        public IReadOnlyList<OmniValue> AsList() => Type == OmniValueType.List ? _list : throw Wrong(OmniValueType.List);

        InvalidCastException Wrong(OmniValueType want) =>
            new InvalidCastException($"the property is {Type}, not {want}");

        public override string ToString() => Type switch
        {
            OmniValueType.Int => _int.ToString(),
            OmniValueType.Float => _float.ToString(System.Globalization.CultureInfo.InvariantCulture),
            OmniValueType.Bool => _bool ? "true" : "false",
            OmniValueType.String => _string,
            OmniValueType.Cell => $"({_cell[0]}, {_cell[1]}, {_cell[2]})",
            OmniValueType.Object => $"object {_int}",
            _ => $"[{string.Join(", ", _list)}]",
        };
    }

    /// <summary>
    /// A thing with a place: a room, a structure, a spawn point. Its box is
    /// in cells, in the map's axes (x right, y down, z up): its first cell
    /// and its size. Where on the box an engine object sits is the adapter's
    /// choice (a pivot), not the map's.
    /// </summary>
    public sealed class OmniObjectData
    {
        public long Id { get; }
        public string Type { get; }
        /// <summary>The object layer it belongs to.</summary>
        public string Layer { get; }
        public int X { get; }
        public int Y { get; }
        public int Z { get; }
        public int Width { get; }
        public int Height { get; }
        public int Depth { get; }
        public IReadOnlyDictionary<string, OmniValue> Props { get; }

        public OmniObjectData(long id, string type, string layer, int x, int y, int z, int w, int h, int d,
            IReadOnlyDictionary<string, OmniValue> props)
        {
            Id = id; Type = type; Layer = layer; X = x; Y = y; Z = z; Width = w; Height = h; Depth = d; Props = props;
        }
    }

    public sealed class OmniMap
    {
        public int Width { get; }
        public int Height { get; }
        public int Depth { get; }
        public int OriginX { get; }
        public int OriginY { get; }
        public int OriginZ { get; }
        public ulong Seed { get; }
        /// <summary>The attempt that made it (a pipeline's <c>attempts</c>), from 0.</summary>
        public int Attempt { get; }
        /// <summary>Tile names by id; <c>Tiles[0]</c> is always "empty".</summary>
        public IReadOnlyList<string> Tiles { get; }
        /// <summary>The cell layers in draw order, the first at the bottom.</summary>
        public IReadOnlyList<OmniLayer> Layers { get; }
        public IReadOnlyList<OmniObjectData> Objects { get; }

        readonly Dictionary<long, OmniObjectData> _byId = new Dictionary<long, OmniObjectData>();

        public OmniMap(int width, int height, int depth, int originX, int originY, int originZ, ulong seed,
            int attempt, IReadOnlyList<string> tiles, IReadOnlyList<OmniLayer> layers,
            IReadOnlyList<OmniObjectData> objects)
        {
            Width = width; Height = height; Depth = depth;
            OriginX = originX; OriginY = originY; OriginZ = originZ;
            Seed = seed; Attempt = attempt; Tiles = tiles; Layers = layers; Objects = objects;
            int cells = CellCount;
            foreach (var l in layers)
            {
                int n = l.TileIds?.Length ?? l.Indices?.Length ?? l.Values.Length;
                if (n != cells)
                    throw new FormatException($"layer '{l.Name}' has {n} cells; the map has {cells}");
            }
            foreach (var o in objects)
                _byId[o.Id] = o;
        }

        public int CellCount => Width * Height * Depth;

        /// <summary>The index of cell (x, y, z) in a layer's array, from the map's origin.</summary>
        public int CellIndex(int x, int y, int z = 0) =>
            ((z - OriginZ) * Height + (y - OriginY)) * Width + (x - OriginX);

        /// <summary>The layer named <paramref name="name"/>, or null.</summary>
        public OmniLayer Layer(string name)
        {
            foreach (var l in Layers)
                if (l.Name == name)
                    return l;
            return null;
        }

        /// <summary>The object with id <paramref name="id"/> (a prop of type object names one), or null.</summary>
        public OmniObjectData ObjectById(long id) => _byId.TryGetValue(id, out var o) ? o : null;

        /// <summary>
        /// Reads a .omap file (output v2). Refuses another major version and a
        /// layer kind or encoding it does not know; ignores fields it does not
        /// know (a later minor version adds them).
        /// </summary>
        public static OmniMap FromOmap(string text) => OmapReader.Read(text);
    }
}
