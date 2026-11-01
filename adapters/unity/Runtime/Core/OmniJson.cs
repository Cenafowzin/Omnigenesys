// A small JSON reader, enough for output v2: Unity's JsonUtility cannot read
// a dictionary or a list of mixed values, and the package should not depend
// on Newtonsoft. Plain C#, no UnityEngine.
//
// Values: Dictionary<string, object> (in file order is not kept; keys are
// unique), List<object>, string, long (a number without '.', 'e' or 'E' that
// fits), double, bool, null.
using System;
using System.Collections.Generic;
using System.Globalization;
using System.Text;

namespace Omnigenesys
{
    public static class OmniJson
    {
        public static object Parse(string text)
        {
            var p = new Parser(text);
            p.SkipSpace();
            var v = p.Value();
            p.SkipSpace();
            if (p.Pos != text.Length)
                throw p.Error("text after the value");
            return v;
        }

        sealed class Parser
        {
            readonly string _s;
            public int Pos;

            public Parser(string s) { _s = s; }

            public FormatException Error(string what)
            {
                int line = 1, col = 1;
                for (int i = 0; i < Pos && i < _s.Length; i++)
                {
                    if (_s[i] == '\n') { line++; col = 1; }
                    else col++;
                }
                return new FormatException($"JSON {line}:{col}: {what}");
            }

            public void SkipSpace()
            {
                while (Pos < _s.Length && (_s[Pos] == ' ' || _s[Pos] == '\n' || _s[Pos] == '\r' || _s[Pos] == '\t'))
                    Pos++;
            }

            char Peek() => Pos < _s.Length ? _s[Pos] : '\0';

            void Expect(char c)
            {
                if (Peek() != c)
                    throw Error($"expected '{c}'");
                Pos++;
            }

            public object Value()
            {
                switch (Peek())
                {
                    case '{': return Obj();
                    case '[': return Arr();
                    case '"': return Str();
                    case 't': Word("true"); return true;
                    case 'f': Word("false"); return false;
                    case 'n': Word("null"); return null;
                    default: return Num();
                }
            }

            void Word(string w)
            {
                if (string.CompareOrdinal(_s, Pos, w, 0, w.Length) != 0)
                    throw Error($"expected {w}");
                Pos += w.Length;
            }

            Dictionary<string, object> Obj()
            {
                Expect('{');
                var d = new Dictionary<string, object>();
                SkipSpace();
                if (Peek() == '}') { Pos++; return d; }
                while (true)
                {
                    SkipSpace();
                    var k = Str();
                    SkipSpace();
                    Expect(':');
                    SkipSpace();
                    d[k] = Value();
                    SkipSpace();
                    if (Peek() == ',') { Pos++; continue; }
                    Expect('}');
                    return d;
                }
            }

            List<object> Arr()
            {
                Expect('[');
                var a = new List<object>();
                SkipSpace();
                if (Peek() == ']') { Pos++; return a; }
                while (true)
                {
                    SkipSpace();
                    a.Add(Value());
                    SkipSpace();
                    if (Peek() == ',') { Pos++; continue; }
                    Expect(']');
                    return a;
                }
            }

            string Str()
            {
                Expect('"');
                int start = Pos;
                // The usual case: no escapes, one substring.
                while (Pos < _s.Length && _s[Pos] != '"' && _s[Pos] != '\\')
                    Pos++;
                if (Peek() == '"')
                {
                    var plain = _s.Substring(start, Pos - start);
                    Pos++;
                    return plain;
                }
                var sb = new StringBuilder(_s, start, Pos - start, Pos - start + 16);
                while (true)
                {
                    if (Pos >= _s.Length)
                        throw Error("unterminated string");
                    char c = _s[Pos++];
                    if (c == '"')
                        return sb.ToString();
                    if (c != '\\') { sb.Append(c); continue; }
                    if (Pos >= _s.Length)
                        throw Error("unterminated escape");
                    char e = _s[Pos++];
                    switch (e)
                    {
                        case '"': sb.Append('"'); break;
                        case '\\': sb.Append('\\'); break;
                        case '/': sb.Append('/'); break;
                        case 'b': sb.Append('\b'); break;
                        case 'f': sb.Append('\f'); break;
                        case 'n': sb.Append('\n'); break;
                        case 'r': sb.Append('\r'); break;
                        case 't': sb.Append('\t'); break;
                        case 'u':
                            if (Pos + 4 > _s.Length)
                                throw Error("short \\u escape");
                            sb.Append((char)int.Parse(_s.Substring(Pos, 4), NumberStyles.HexNumber, CultureInfo.InvariantCulture));
                            Pos += 4;
                            break;
                        default: throw Error($"unknown escape \\{e}");
                    }
                }
            }

            object Num()
            {
                int start = Pos;
                bool real = false;
                if (Peek() == '-') Pos++;
                while (Pos < _s.Length)
                {
                    char c = _s[Pos];
                    if (c >= '0' && c <= '9') { Pos++; continue; }
                    if (c == '.' || c == 'e' || c == 'E' || c == '+' || c == '-') { real = true; Pos++; continue; }
                    break;
                }
                if (Pos == start)
                    throw Error("expected a value");
                var t = _s.Substring(start, Pos - start);
                if (!real && long.TryParse(t, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out var n))
                    return n;
                if (double.TryParse(t, NumberStyles.Float, CultureInfo.InvariantCulture, out var d))
                    return d;
                throw Error($"bad number {t}");
            }
        }
    }

    /// <summary>Reads output v2 (a .omap file, or the library's objects array).</summary>
    static class OmapReader
    {
        public const int Version = 2;

        public static OmniMap Read(string text)
        {
            var root = OmniJson.Parse(text) as Dictionary<string, object>
                ?? throw new FormatException("a .omap file is a JSON object");
            if (Get<string>(root, "format") != "omnigenesys-map")
                throw new FormatException("not an Omnigenesys map (format is not \"omnigenesys-map\")");
            long version = Get<long>(root, "version");
            if (version != Version)
                throw new FormatException($"map format version {version}; this reader knows {Version}");
            var size = Ints(Get<List<object>>(root, "size"), 3, "size");
            var origin = Ints(Get<List<object>>(root, "origin"), 3, "origin");
            var seedText = Get<string>(root, "seed");
            if (!ulong.TryParse(seedText, NumberStyles.None, CultureInfo.InvariantCulture, out var seed))
                throw new FormatException($"seed '{seedText}' is not a whole number");
            int attempt = root.TryGetValue("attempt", out var a) && a is long al ? (int)al : 0;
            var tiles = new List<string>();
            foreach (var t in Get<List<object>>(root, "tiles"))
                tiles.Add(t as string ?? throw new FormatException("a tile name is not a string"));

            int cells = size[0] * size[1] * size[2];
            var layers = new List<OmniLayer>();
            foreach (var item in Get<List<object>>(root, "layers"))
            {
                var l = item as Dictionary<string, object> ?? throw new FormatException("a layer is not an object");
                layers.Add(Layer(l, cells, tiles.Count));
            }
            var objects = Objects(Get<List<object>>(root, "objects"));
            return new OmniMap(size[0], size[1], size[2], origin[0], origin[1], origin[2], seed, attempt,
                tiles, layers, objects);
        }

        static OmniLayer Layer(Dictionary<string, object> l, int cells, int tileCount)
        {
            var name = Get<string>(l, "name");
            var kind = Get<string>(l, "kind");
            var encoding = Get<string>(l, "encoding");
            if (kind == "scalar")
            {
                if (encoding != "dense")
                    throw new FormatException($"layer '{name}': encoding '{encoding}' is unknown for a scalar layer");
                var values = Get<List<object>>(l, "values");
                var f = new float[values.Count];
                for (int i = 0; i < f.Length; i++)
                    f[i] = values[i] switch
                    {
                        long n => n,
                        double d => (float)d,
                        _ => throw new FormatException($"layer '{name}': value {i} is not a number"),
                    };
                return new OmniLayer(name, f);
            }
            if (kind != "tiles" && kind != "index")
                throw new FormatException($"layer '{name}': kind '{kind}' is unknown (a newer format?)");
            uint[] whole;
            if (encoding == "rle")
                whole = Runs(Get<List<object>>(l, "runs"), cells, name);
            else if (encoding == "dense")
            {
                var list = Get<List<object>>(l, "cells");
                whole = new uint[list.Count];
                for (int i = 0; i < whole.Length; i++)
                    whole[i] = Whole(list[i], name);
            }
            else
                throw new FormatException($"layer '{name}': encoding '{encoding}' is unknown (a newer format?)");
            if (kind == "index")
                return new OmniLayer(name, whole);
            var ids = new ushort[whole.Length];
            for (int i = 0; i < ids.Length; i++)
            {
                if (whole[i] >= tileCount)
                    throw new FormatException($"layer '{name}': tile id {whole[i]}, but the map names {tileCount} tiles");
                ids[i] = (ushort)whole[i];
            }
            return new OmniLayer(name, ids);
        }

        static uint[] Runs(List<object> runs, int cells, string name)
        {
            if (runs.Count % 2 != 0)
                throw new FormatException($"layer '{name}': runs come in pairs (value, count)");
            var out_ = new uint[cells];
            int at = 0;
            for (int i = 0; i < runs.Count; i += 2)
            {
                uint v = Whole(runs[i], name);
                uint n = Whole(runs[i + 1], name);
                if (at + (long)n > cells)
                    throw new FormatException($"layer '{name}': the runs hold more than {cells} cells");
                for (uint k = 0; k < n; k++)
                    out_[at++] = v;
            }
            if (at != cells)
                throw new FormatException($"layer '{name}': the runs hold {at} cells; the map has {cells}");
            return out_;
        }

        static uint Whole(object v, string name) =>
            v is long n && n >= 0 && n <= uint.MaxValue ? (uint)n
                : throw new FormatException($"layer '{name}': {v} is not a whole number");

        /// <summary>The <c>objects</c> array of output v2.</summary>
        public static List<OmniObjectData> Objects(List<object> list)
        {
            var out_ = new List<OmniObjectData>(list.Count);
            foreach (var item in list)
            {
                var o = item as Dictionary<string, object> ?? throw new FormatException("an object is not a JSON object");
                var b = Ints(Get<List<object>>(o, "bbox"), 6, "bbox");
                var props = new Dictionary<string, OmniValue>();
                foreach (var kv in Get<Dictionary<string, object>>(o, "props"))
                    props[kv.Key] = Prop(kv.Value, kv.Key);
                out_.Add(new OmniObjectData(Get<long>(o, "id"), Get<string>(o, "type"), Get<string>(o, "layer"),
                    b[0], b[1], b[2], b[3], b[4], b[5], props));
            }
            return out_;
        }

        static OmniValue Prop(object v, string name)
        {
            var p = v as Dictionary<string, object> ?? throw new FormatException($"prop '{name}' is not {{type, value}}");
            var type = Get<string>(p, "type");
            p.TryGetValue("value", out var value);
            switch (type)
            {
                case "int": return OmniValue.Int(value is long n ? n : throw Bad());
                case "float":
                    return OmniValue.Float(value is double d ? d : value is long l ? l : throw Bad());
                case "bool": return OmniValue.Bool(value is bool b ? b : throw Bad());
                case "string": return OmniValue.String(value as string ?? throw Bad());
                case "cell":
                    var c = Ints(value as List<object> ?? throw Bad(), 3, name);
                    return OmniValue.Cell(c[0], c[1], c[2]);
                case "object": return OmniValue.ObjectRef(value is long id ? id : throw Bad());
                case "list":
                    var items = new List<OmniValue>();
                    foreach (var item in value as List<object> ?? throw Bad())
                        items.Add(Prop(item, name));
                    return OmniValue.List(items);
                default: throw new FormatException($"prop '{name}': type '{type}' is unknown (a newer format?)");
            }

            FormatException Bad() => new FormatException($"prop '{name}': its value is not a {type}");
        }

        static int[] Ints(List<object> list, int n, string what)
        {
            if (list.Count != n)
                throw new FormatException($"{what} has {list.Count} numbers, not {n}");
            var out_ = new int[n];
            for (int i = 0; i < n; i++)
                out_[i] = list[i] is long v && v >= int.MinValue && v <= int.MaxValue ? (int)v
                    : throw new FormatException($"{what}[{i}] is not a whole number");
            return out_;
        }

        static T Get<T>(Dictionary<string, object> d, string key)
        {
            if (!d.TryGetValue(key, out var v))
                throw new FormatException($"'{key}' is missing");
            if (v is T t)
                return t;
            throw new FormatException($"'{key}' is not a {typeof(T).Name}");
        }
    }
}
