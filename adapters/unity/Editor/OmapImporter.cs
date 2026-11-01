using System;
using System.IO;
using UnityEditor.AssetImporters;
using UnityEngine;

namespace Omnigenesys.Editor
{
    /// <summary>
    /// Imports a .omap file (output v2) as a TextAsset, so it can be the
    /// fallback map of an OmniGenerator or be read with OmniMap.FromOmap. A
    /// file that does not read as a map fails here, at import, with why.
    /// </summary>
    [ScriptedImporter(1, "omap")]
    public sealed class OmapImporter : ScriptedImporter
    {
        public override void OnImportAsset(AssetImportContext ctx)
        {
            var text = File.ReadAllText(ctx.assetPath);
            try
            {
                var map = OmniMap.FromOmap(text);
                var asset = new TextAsset(text);
                asset.name = Path.GetFileNameWithoutExtension(ctx.assetPath);
                ctx.AddObjectToAsset("map", asset);
                ctx.SetMainObject(asset);
                Debug.Log($"[Omnigenesys] {ctx.assetPath}: {map.Width}×{map.Height}×{map.Depth}, seed {map.Seed}, " +
                          $"{map.Layers.Count} layers, {map.Objects.Count} objects");
            }
            catch (FormatException e)
            {
                ctx.LogImportError($"{ctx.assetPath}: {e.Message}");
            }
        }
    }
}
