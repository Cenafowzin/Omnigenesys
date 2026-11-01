// Checks of the package's plain C# against the real native library. Exit
// code 0 when every check passes; each failure is printed.
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading.Tasks;
using Omnigenesys;

static class Program
{
    static int _failed;
    static int _passed;

    static void Check(bool ok, string what)
    {
        if (ok) { _passed++; return; }
        _failed++;
        Console.WriteLine($"FAIL: {what}");
    }

    static void Throws<T>(Action f, string what, Func<T, bool> also = null) where T : Exception
    {
        try
        {
            f();
            Check(false, $"{what}: did not throw");
        }
        catch (T e)
        {
            Check(also == null || also(e), $"{what}: {e.Message}");
        }
        catch (Exception e)
        {
            Check(false, $"{what}: threw {e.GetType().Name}: {e.Message}");
        }
    }

    static string Root()
    {
        var dir = AppContext.BaseDirectory;
        while (dir != null && !File.Exists(Path.Combine(dir, "Cargo.toml")))
            dir = Path.GetDirectoryName(dir);
        return dir ?? throw new Exception("the repository root (Cargo.toml) is not above the test");
    }

    static bool SameMap(OmniMap a, OmniMap b, out string why)
    {
        why = null;
        if ((a.Width, a.Height, a.Depth, a.OriginX, a.OriginY, a.OriginZ, a.Seed, a.Attempt) !=
            (b.Width, b.Height, b.Depth, b.OriginX, b.OriginY, b.OriginZ, b.Seed, b.Attempt))
        { why = "size, origin, seed or attempt"; return false; }
        if (!a.Tiles.SequenceEqual(b.Tiles)) { why = "tiles"; return false; }
        if (a.Layers.Count != b.Layers.Count) { why = $"{a.Layers.Count} against {b.Layers.Count} layers"; return false; }
        for (int i = 0; i < a.Layers.Count; i++)
        {
            var (x, y) = (a.Layers[i], b.Layers[i]);
            bool same = x.Name == y.Name && x.Kind == y.Kind && x.Kind switch
            {
                OmniLayerKind.Tiles => x.TileIds.SequenceEqual(y.TileIds),
                OmniLayerKind.Index => x.Indices.SequenceEqual(y.Indices),
                _ => x.Values.SequenceEqual(y.Values),
            };
            if (!same) { why = $"layer {x.Name}"; return false; }
        }
        if (a.Objects.Count != b.Objects.Count) { why = "object count"; return false; }
        for (int i = 0; i < a.Objects.Count; i++)
        {
            var (x, y) = (a.Objects[i], b.Objects[i]);
            if ((x.Id, x.Type, x.Layer, x.X, x.Y, x.Z, x.Width, x.Height, x.Depth) !=
                (y.Id, y.Type, y.Layer, y.X, y.Y, y.Z, y.Width, y.Height, y.Depth))
            { why = $"object {x.Id}"; return false; }
            if (x.Props.Count != y.Props.Count ||
                x.Props.Any(p => !y.Props.TryGetValue(p.Key, out var v) || v.Type != p.Value.Type || v.ToString() != p.Value.ToString()))
            { why = $"props of object {x.Id}"; return false; }
        }
        return true;
    }

    static int Main()
    {
        var root = Root();
        var pipelines = Path.Combine(root, "crates", "omnigenesys-core", "tests", "pipelines");
        var lost = File.ReadAllText(Path.Combine(pipelines, "lost_fields.json"));

        Check(OmniNative.Version.Length > 0 && char.IsDigit(OmniNative.Version[0]), $"version '{OmniNative.Version}'");

        // The library's map and the same map read back from its .omap are one map.
        var map = OmniNative.Generate(lost, 77);
        var omap = OmniNative.GenerateOmap(lost, 77);
        var read = OmniMap.FromOmap(omap);
        Check(SameMap(map, read, out var why), $"native map against its .omap: {why}");
        Check(map.Width == 80 && map.Height == 80 && map.Depth == 1 && map.Seed == 77, "Lost Fields is 80×80, seed 77");
        Check(map.Tiles[0] == "empty", "tiles[0] is empty");
        foreach (var kind in new[] { OmniLayerKind.Tiles, OmniLayerKind.Scalar, OmniLayerKind.Index })
            Check(map.Layers.Any(l => l.Kind == kind), $"a {kind} layer");
        Check(map.Objects.Count > 5, $"{map.Objects.Count} objects");
        Check(map.Objects.All(o => map.ObjectById(o.Id) == o), "objects by id");

        // Every recipe, each map against its .omap; then the same seed twice.
        foreach (var file in Directory.GetFiles(pipelines, "*.json").OrderBy(f => f))
        {
            var text = File.ReadAllText(file);
            var name = Path.GetFileName(file);
            var m = OmniNative.Generate(text);
            Check(SameMap(m, OmniMap.FromOmap(OmniNative.GenerateOmap(text)), out why), $"{name}: map against .omap: {why}");
            Check(SameMap(m, OmniNative.Generate(text), out why), $"{name}: the same twice: {why}");
        }

        // Seeds, params and salt change the map; the file's seed when none.
        var ilha = File.ReadAllText(Path.Combine(pipelines, "ilha.json"));
        var a = OmniNative.Generate(ilha, 5);
        Check(!SameMap(a, OmniNative.Generate(ilha, 6), out _), "another seed, another map");
        Check(!SameMap(a, OmniNative.Generate(ilha, 5, "{\"tamanho\": 0.5}"), out _), "a param changes the map");
        Check(!SameMap(a, OmniNative.Generate(ilha, 5, null, "outro"), out _), "the salt changes the map");
        Check(!SameMap(OmniNative.Generate(ilha, 5, null, ""), OmniNative.Generate(ilha, 5, null, "x"), out _), "an empty salt is a salt");

        // Many threads at once: each its own map, each the same.
        var maps = Task.WhenAll(Enumerable.Range(0, 8).Select(_ => Task.Run(() => OmniNative.Generate(lost, 77)))).Result;
        Check(maps.All(m => SameMap(m, map, out _)), "8 threads make the same map");
        var errors = Task.WhenAll(Enumerable.Range(0, 8).Select(i => Task.Run(() =>
        {
            try { OmniNative.Generate("{ \"oops\": " + i); return ""; }
            catch (OmniException e) { return e.Message; }
        }))).Result;
        Check(errors.All(e => e.Length > 0) && errors.Distinct().Count() >= 1, "each thread gets its own error");

        // Errors.
        Throws<OmniException>(() => OmniNative.Generate("{ not json"), "bad JSON", e => e.IsPipelineError && e.Message.StartsWith("1:"));
        Throws<OmniException>(() => OmniNative.Generate(lost.Replace("\"tries\"", "\"triez\"")), "a typo",
            e => e.IsPipelineError && e.Message.Contains("triez"));
        Throws<OmniException>(() => OmniNative.Generate(ilha, 1, "[1]"), "params not an object",
            e => e.Code == Omnigenesys.Native.NativeMethods.OMNI_ERR_ARGUMENT);
        Throws<ArgumentNullException>(() => OmniNative.Generate(null), "a null pipeline");

        // The reader refuses what it must and reads past what a later minor adds.
        Throws<FormatException>(() => OmniMap.FromOmap(omap.Replace("\"version\": 2", "\"version\": 3")), "another major");
        Throws<FormatException>(() => OmniMap.FromOmap(omap.Replace("\"kind\": \"tiles\"", "\"kind\": \"voxels\"")), "an unknown kind");
        Throws<FormatException>(() => OmniMap.FromOmap(omap.Replace("\"encoding\": \"rle\"", "\"encoding\": \"zstd\"")), "an unknown encoding");
        Throws<FormatException>(() => OmniMap.FromOmap("[]"), "not an object");
        var newer = omap.Replace("\"minor\": 0", "\"minor\": 4, \"chunks\": [], \"extra\": {\"a\": [1, 2.5e3, \"x\\u00e9\"]}");
        Check(SameMap(OmniMap.FromOmap(newer), read, out why), $"a later minor reads the same: {why}");

        // JSON corner cases.
        var j = OmniJson.Parse("{\"a\": [1, -2, 3.5, 1e2, true, false, null, \"\\\"q\\\\\\n\"], \"b\": {}}") as Dictionary<string, object>;
        var arr = (List<object>)j["a"];
        Check(arr[0] is long l1 && l1 == 1 && arr[1] is long l2 && l2 == -2, "integers are long");
        Check(arr[2] is double d1 && d1 == 3.5 && arr[3] is double d2 && d2 == 100, "reals are double");
        Check((string)arr[7] == "\"q\\\n", "escapes");
        Throws<FormatException>(() => OmniJson.Parse("{\"a\": 1} x"), "text after the value");

        Console.WriteLine($"{_passed} passed, {_failed} failed");
        return _failed == 0 ? 0 : 1;
    }
}
