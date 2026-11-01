using System;
using System.Globalization;
using System.Text;
using System.Threading.Tasks;
using UnityEngine;
using UnityEngine.Events;

namespace Omnigenesys
{
    public enum OmniSeedMode
    {
        /// <summary>The pipeline file's seed.</summary>
        FromFile,
        /// <summary>The seed set here.</summary>
        Fixed,
        /// <summary>A new seed each time.</summary>
        Random,
    }

    /// <summary>
    /// Makes a map from a pipeline file and builds it with an
    /// <see cref="OmniMapBuilder"/>: the map is made off the main thread (the
    /// game keeps running) and built on it.
    ///
    /// Where the native library does not run (WebGL for now), it loads
    /// <see cref="fallbackMap"/>, a .omap made beforehand.
    /// </summary>
    public sealed class OmniGenerator : MonoBehaviour
    {
        [Serializable]
        public sealed class Param
        {
            [Tooltip("A param the pipeline declares.")]
            public string name;
            [Tooltip("Its value as you would write it: 12, 0.5, true, forest.")]
            public string value;
        }

        [Tooltip("The pipeline file (.json).")]
        public TextAsset pipeline;
        public OmniMapBuilder builder;
        public OmniSeedMode seedMode = OmniSeedMode.FromFile;
        [Tooltip("The seed for Fixed (a whole number up to 18446744073709551615).")]
        public string seed = "0";
        [Tooltip("Overrides of the pipeline's params, by name.")]
        public Param[] parameters = Array.Empty<Param>();
        [Tooltip("Make and build the map when the scene starts.")]
        public bool generateOnStart = true;
        [Tooltip("A map made beforehand (.omap), for platforms without the native library.")]
        public TextAsset fallbackMap;

        [Tooltip("Called with the map once it is built.")]
        public UnityEvent<OmniMap> onBuilt = new UnityEvent<OmniMap>();

        /// <summary>The seed of the last map made.</summary>
        public ulong LastSeed { get; private set; }
        public bool Busy { get; private set; }

        async void Start()
        {
            if (generateOnStart)
                await Generate();
        }

        /// <summary>Makes and builds a map with this component's settings; null if it failed (logged).</summary>
        public Task<OmniMap> Generate() => Generate(SeedToUse());

        /// <summary>Makes and builds the map of <paramref name="withSeed"/> (null: the file's).</summary>
        public async Task<OmniMap> Generate(ulong? withSeed)
        {
            if (Busy)
                throw new InvalidOperationException($"{name}: already making a map");
            Busy = true;
            try
            {
                OmniMap map;
#if UNITY_WEBGL && !UNITY_EDITOR
                map = Fallback();
#else
                if (pipeline == null)
                    throw new InvalidOperationException($"{name}: OmniGenerator has no pipeline");
                var text = pipeline.text;
                var pars = ParamsJson();
                map = await Task.Run(() => OmniNative.Generate(text, withSeed, pars));
#endif
                // Back on the main thread: Unity objects only here.
                if (this == null)
                    return null;
                LastSeed = map.Seed;
                if (builder != null)
                    builder.Build(map);
                onBuilt.Invoke(map);
                return map;
            }
            catch (OmniException e)
            {
                Debug.LogError($"[Omnigenesys] {pipeline?.name}: {e.Message}", this);
                return null;
            }
            finally
            {
                Busy = false;
            }
        }

        OmniMap Fallback()
        {
            if (fallbackMap == null)
                throw new InvalidOperationException(
                    $"{name}: this platform has no native library; give OmniGenerator a fallback map (.omap)");
            return OmniMap.FromOmap(fallbackMap.text);
        }

        ulong? SeedToUse()
        {
            switch (seedMode)
            {
                case OmniSeedMode.Fixed:
                    if (!ulong.TryParse(seed, NumberStyles.None, CultureInfo.InvariantCulture, out var s))
                        throw new FormatException($"{name}: seed '{seed}' is not a whole number");
                    return s;
                case OmniSeedMode.Random:
                    var bytes = new byte[8];
                    using (var rng = System.Security.Cryptography.RandomNumberGenerator.Create())
                        rng.GetBytes(bytes);
                    return BitConverter.ToUInt64(bytes, 0);
                default:
                    return null;
            }
        }

        /// <summary><c>{"name": "value"}</c>: the library reads each value by the param's declared type.</summary>
        string ParamsJson()
        {
            if (parameters.Length == 0)
                return null;
            var sb = new StringBuilder("{");
            for (int i = 0; i < parameters.Length; i++)
            {
                if (i > 0)
                    sb.Append(',');
                Quote(sb, parameters[i].name);
                sb.Append(':');
                Quote(sb, parameters[i].value ?? "");
            }
            return sb.Append('}').ToString();
        }

        static void Quote(StringBuilder sb, string s)
        {
            sb.Append('"');
            foreach (var c in s)
            {
                if (c == '"' || c == '\\')
                    sb.Append('\\').Append(c);
                else if (c < 0x20)
                    sb.Append("\\u").Append(((int)c).ToString("x4"));
                else
                    sb.Append(c);
            }
            sb.Append('"');
        }

#if UNITY_EDITOR
        /// <summary>Saves the last map made as a .omap next to the pipeline: a seed worth keeping, or the fallback.</summary>
        [ContextMenu("Save last seed as .omap")]
        void SaveOmap()
        {
            if (pipeline == null)
                return;
            var omap = OmniNative.GenerateOmap(pipeline.text, LastSeed, ParamsJson());
            var path = UnityEditor.AssetDatabase.GetAssetPath(pipeline);
            path = System.IO.Path.ChangeExtension(path, null) + $"-{LastSeed}.omap";
            System.IO.File.WriteAllText(path, omap, new UTF8Encoding(false));
            UnityEditor.AssetDatabase.ImportAsset(path);
            Debug.Log($"[Omnigenesys] saved {path}", this);
        }
#endif
    }
}
