using System;
using System.Diagnostics;
using System.IO;
using Newtonsoft.Json.Linq;
using Newtonsoft.Json;
using UnityEngine;

namespace Omnigenesys
{
    /// <summary>
    /// Gera mapas em runtime chamando o binário Go como subprocess.
    ///
    /// Fluxo:
    ///   1. Lê o pipelineConfig (TextAsset JSON)
    ///   2. Substitui seed se necessário
    ///   3. Envia o JSON via stdin para mapgen.exe
    ///   4. Recebe o mapa JSON via stdout
    ///   5. Chama MapBuilder.Build()
    ///
    /// O mapgen.exe é instalado automaticamente em StreamingAssets/MapGen/
    /// pelo editor script MapGenInstaller ao importar o package.
    /// </summary>
    public class MapGeneratorRuntime : MonoBehaviour
    {
        [Header("Pipeline")]
        [Tooltip("Arraste o arquivo .json da pipeline (ex: cornfield_pipeline.json)")]
        [SerializeField] private TextAsset pipelineConfig;

        [Tooltip("Seed (0 = valor do JSON ou aleatório se o JSON também for 0)")]
        [SerializeField] private long seed = 0;

        [Header("Referências da cena")]
        [SerializeField] private MapBuilder builder;

        [Header("Binário Go")]
        [Tooltip("Caminho relativo a StreamingAssets/ — instalado automaticamente pelo package")]
        [SerializeField] private string executablePath = "MapGen/mapgen.exe";

        // ── API pública ───────────────────────────────────────────────────────

        /// <summary>Gera usando a seed configurada no Inspector (ou a do JSON se seed=0).</summary>
        public void Generate() => GenerateWithSeed(seed);

        /// <summary>Gera com seed explícita (útil para seeds de save/load).</summary>
        public void GenerateWithSeed(long overrideSeed)
        {
            try
            {
                var config = JObject.Parse(pipelineConfig.text);
                var deterministic = config["deterministic_variants"]?.Value<bool>() ?? true;
                var configJson = PrepareConfig(config, overrideSeed);
                var mapJson = RunMapgen(configJson);
                builder.Build(mapJson, deterministic);
            }
            catch (Exception ex)
            {
                UnityEngine.Debug.LogError($"[MapGen] {ex.Message}");
            }
        }

        // ── internals ─────────────────────────────────────────────────────────

        private static string PrepareConfig(JObject config, long overrideSeed)
        {
            if (overrideSeed != 0)
                config["seed"] = overrideSeed;
            return config.ToString(Formatting.None);
        }

        private string RunMapgen(string configJson)
        {
            var path = Path.Combine(Application.streamingAssetsPath, executablePath);
            if (!File.Exists(path))
                throw new FileNotFoundException(
                    $"mapgen.exe não encontrado: {path}\n" +
                    "Use Tools → Procedural Map Framework → Install StreamingAssets para reinstalar.");

            var psi = new ProcessStartInfo
            {
                FileName               = path,
                RedirectStandardInput  = true,
                RedirectStandardOutput = true,
                RedirectStandardError  = true,
                UseShellExecute        = false,
                CreateNoWindow         = true,
            };

            using var proc = Process.Start(psi) ?? throw new Exception($"Falha ao iniciar: {path}");

            proc.StandardInput.Write(configJson);
            proc.StandardInput.Close();

            var mapJson = proc.StandardOutput.ReadToEnd();
            var stderr  = proc.StandardError.ReadToEnd();
            proc.WaitForExit();

            if (proc.ExitCode != 0)
                throw new Exception($"mapgen saiu com código {proc.ExitCode}: {stderr}");

            return mapJson;
        }
    }
}
