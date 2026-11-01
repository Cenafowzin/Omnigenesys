// The native library, safely: makes an OmniMap from a pipeline file. Plain
// C#, no UnityEngine. Safe to call from any thread (one map per call); the
// result is plain data, so building the scene from it is up to the main
// thread.
using System;
using System.Collections.Generic;
using System.Text;
using Omnigenesys.Native;
// The library's opaque handle; Omnigenesys.OmniMap is the map as data.
using NativeMap = Omnigenesys.Native.OmniMap;

namespace Omnigenesys
{
    /// <summary>A failure from the library: the pipeline, an argument, or a bug.</summary>
    public sealed class OmniException : Exception
    {
        /// <summary>The library's status (OMNI_ERR_*).</summary>
        public int Code { get; }

        public OmniException(int code, string message) : base(message) { Code = code; }

        /// <summary>The pipeline does not parse, build or run (the message says where).</summary>
        public bool IsPipelineError => Code == NativeMethods.OMNI_ERR_PIPELINE;
    }

    public static unsafe class OmniNative
    {
        /// <summary>The engine version of the library ("0.1.0").</summary>
        public static string Version
        {
            get
            {
                var buf = new byte[64];
                fixed (byte* p = buf)
                {
                    int n = (int)NativeMethods.omni_version(p, (nuint)buf.Length);
                    return Encoding.UTF8.GetString(buf, 0, Math.Min(n, buf.Length));
                }
            }
        }

        /// <summary>
        /// Makes the map of <paramref name="pipeline"/> (a pipeline file's text).
        /// </summary>
        /// <param name="seed">The seed; null for the file's (which it must then have).</param>
        /// <param name="paramsJson">Param overrides as a JSON object, <c>{"name": value}</c>; null for the file's.</param>
        /// <param name="salt">The salt; null for the file's.</param>
        /// <exception cref="OmniException">The pipeline fails, with where and why.</exception>
        public static OmniMap Generate(string pipeline, ulong? seed = null, string paramsJson = null, string salt = null)
        {
            var map = Make(pipeline, seed, paramsJson, salt);
            try
            {
                return Read(map);
            }
            finally
            {
                NativeMethods.omni_map_free(map);
            }
        }

        /// <summary>
        /// The map of <paramref name="pipeline"/> as a .omap file's text: to keep
        /// a seed worth keeping, or to load on a platform without the library.
        /// </summary>
        public static string GenerateOmap(string pipeline, ulong? seed = null, string paramsJson = null, string salt = null)
        {
            var map = Make(pipeline, seed, paramsJson, salt);
            try
            {
                return Text((b, c, l) => NativeMethods.omni_map_to_omap(map, b, c, l));
            }
            finally
            {
                NativeMethods.omni_map_free(map);
            }
        }

        /// <summary>Runs the pipeline; the caller frees the handle.</summary>
        static NativeMap* Make(string pipeline, ulong? seed, string paramsJson, string salt)
        {
            if (pipeline == null)
                throw new ArgumentNullException(nameof(pipeline));
            var file = Encoding.UTF8.GetBytes(pipeline);
            var pars = paramsJson == null ? Array.Empty<byte>() : Encoding.UTF8.GetBytes(paramsJson);
            // `fixed` on an empty array gives a null pointer, which means "the
            // file's salt": a terminating zero keeps the empty salt not null.
            var saltBytes = salt == null ? null : Encoding.UTF8.GetBytes(salt + "\0");
            NativeMap* map = null;
            int code;
            fixed (byte* f = file)
            fixed (byte* p = pars)
            fixed (byte* s = saltBytes)
            {
                code = NativeMethods.omni_generate(f, (nuint)file.Length, p, (nuint)pars.Length,
                    seed.HasValue, seed ?? 0, s, saltBytes == null ? 0 : (nuint)(saltBytes.Length - 1), &map);
            }
            // The message is per thread: read it now, on this thread.
            Check(code);
            return map;
        }

        static OmniMap Read(NativeMap* map)
        {
            OmniMapInfo info;
            Check(NativeMethods.omni_map_info(map, &info));
            int cells = checked((int)(info.width * info.height * info.depth));

            var tiles = new List<string>((int)info.tile_count);
            for (uint t = 0; t < info.tile_count; t++)
            {
                var id = t;
                tiles.Add(Text((b, c, l) => NativeMethods.omni_map_tile_name(map, id, b, c, l)));
            }

            var layers = new List<OmniLayer>();
            for (uint i = 0; i < info.layer_count; i++)
            {
                var at = i;
                int kind = -1;
                var name = Text((b, c, l) =>
                {
                    int k;
                    int code = NativeMethods.omni_map_layer(map, at, &k, b, c, l);
                    kind = k;
                    return code;
                });
                switch (kind)
                {
                    case NativeMethods.OMNI_KIND_TILES:
                        var ids = new ushort[cells];
                        fixed (ushort* d = ids)
                            Check(NativeMethods.omni_map_copy_tiles(map, i, d, (nuint)cells));
                        layers.Add(new OmniLayer(name, ids));
                        break;
                    case NativeMethods.OMNI_KIND_INDEX:
                        var whole = new uint[cells];
                        fixed (uint* d = whole)
                            Check(NativeMethods.omni_map_copy_index(map, i, d, (nuint)cells));
                        layers.Add(new OmniLayer(name, whole));
                        break;
                    case NativeMethods.OMNI_KIND_SCALAR:
                        var values = new float[cells];
                        fixed (float* d = values)
                            Check(NativeMethods.omni_map_copy_scalars(map, i, d, (nuint)cells));
                        layers.Add(new OmniLayer(name, values));
                        break;
                    case NativeMethods.OMNI_KIND_OBJECTS:
                        // Its objects come in the objects array.
                        break;
                    default:
                        throw new OmniException(NativeMethods.OMNI_ERR_LAYER, $"layer '{name}' has kind {kind}, unknown to this package");
                }
            }

            var json = Text((b, c, l) => NativeMethods.omni_map_objects_json(map, b, c, l));
            var objects = OmapReader.Objects(OmniJson.Parse(json) as List<object>
                ?? throw new FormatException("the objects are not a JSON array"));
            return new OmniMap((int)info.width, (int)info.height, (int)info.depth,
                info.origin_x, info.origin_y, info.origin_z, info.seed, (int)info.attempt, tiles, layers, objects);
        }

        delegate int TextCall(byte* buf, nuint cap, nuint* len);

        /// <summary>A text the library gives: its length first, then its bytes.</summary>
        static string Text(TextCall call)
        {
            nuint n;
            Check(call(null, 0, &n));
            if (n == 0)
                return "";
            var buf = new byte[(int)n];
            fixed (byte* b = buf)
                Check(call(b, n, &n));
            return Encoding.UTF8.GetString(buf);
        }

        static void Check(int code)
        {
            if (code == NativeMethods.OMNI_OK)
                return;
            nuint n = NativeMethods.omni_last_error(null, 0);
            var buf = new byte[(int)n];
            fixed (byte* b = buf)
                NativeMethods.omni_last_error(b, n);
            throw new OmniException(code, Encoding.UTF8.GetString(buf));
        }
    }
}
