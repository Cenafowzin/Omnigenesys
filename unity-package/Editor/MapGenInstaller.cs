using System.IO;
using UnityEditor;
using UnityEngine;

namespace Omnigenesys.Editor
{
    /// <summary>
    /// Copia automaticamente o mapgen.exe de StreamingAssets~/ para
    /// Assets/StreamingAssets/MapGen/ do projeto ao importar o package.
    ///
    /// Também disponível manualmente em:
    ///   Tools → Procedural Map Framework → Install StreamingAssets
    /// </summary>
    [InitializeOnLoad]
    public static class MapGenInstaller
    {
        static MapGenInstaller()
        {
            // Adia para não travar o editor durante a inicialização
            EditorApplication.delayCall += InstallStreamingAssets;
        }

        [MenuItem("Tools/Procedural Map Framework/Install StreamingAssets")]
        public static void InstallStreamingAssets()
        {
            var packageInfo = UnityEditor.PackageManager.PackageInfo.FindForAssembly(typeof(MapGenInstaller).Assembly);
            if (packageInfo == null)
            {
                Debug.LogWarning("[Omnigenesys] Não foi possível localizar o package.");
                return;
            }

            var source = Path.Combine(packageInfo.resolvedPath, "StreamingAssets~", "MapGen");
            if (!Directory.Exists(source))
            {
                Debug.LogWarning(
                    $"[Omnigenesys] Pasta StreamingAssets~ não encontrada: {source}\n" +
                    "Execute build.sh para gerar o mapgen.exe antes de distribuir o package.");
                return;
            }

            var dest = Path.Combine(Application.dataPath, "StreamingAssets", "MapGen");
            Directory.CreateDirectory(dest);

            int count = 0;
            foreach (var file in Directory.GetFiles(source, "*", SearchOption.TopDirectoryOnly))
            {
                var destFile = Path.Combine(dest, Path.GetFileName(file));
                File.Copy(file, destFile, overwrite: true);
                count++;
            }

            AssetDatabase.Refresh();
            Debug.Log($"[Omnigenesys] {count} arquivo(s) instalados em {dest}");
        }
    }
}
