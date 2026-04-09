#include "MapGeneratorRuntime.h"
#include "MapBuilder.h"

#include "HAL/PlatformProcess.h"
#include "HAL/FileManager.h"
#include "Misc/FileHelper.h"
#include "Misc/Paths.h"
#include "Dom/JsonObject.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"
#include "Serialization/JsonWriter.h"
#include "Interfaces/IPluginManager.h"

// Resolve um caminho relativo: prefixo "project:" usa Content/ do projeto,
// sem prefixo usa Content/ do plugin (padrão).
static FString ResolvePath(const FString& RelativePath)
{
	if (RelativePath.StartsWith(TEXT("project:")))
	{
		return FPaths::Combine(FPaths::ProjectContentDir(), RelativePath.Mid(8));
	}

	TSharedPtr<IPlugin> Plugin = IPluginManager::Get().FindPlugin(TEXT("Omnigenesys"));
	const FString Base = Plugin.IsValid() ? Plugin->GetContentDir() : FPaths::ProjectContentDir();
	return FPaths::Combine(Base, RelativePath);
}

AMapGeneratorRuntime::AMapGeneratorRuntime()
{
	PrimaryActorTick.bCanEverTick = false;
}

void AMapGeneratorRuntime::BeginPlay()
{
	Super::BeginPlay();
}

// ── API Pública ───────────────────────────────────────────────────────────────

void AMapGeneratorRuntime::Generate()
{
	GenerateWithSeed(Seed);
}

void AMapGeneratorRuntime::GenerateWithSeed(int64 OverrideSeed)
{
	if (!Builder)
	{
		UE_LOG(LogTemp, Error, TEXT("[MapGen] Builder não configurado."));
		return;
	}

	FString ConfigJson;
	if (!PrepareConfig(OverrideSeed, ConfigJson))
	{
		UE_LOG(LogTemp, Error, TEXT("[MapGen] Falha ao preparar config."));
		return;
	}

	FString MapJson;
	if (!RunMapgen(ConfigJson, MapJson))
	{
		UE_LOG(LogTemp, Error, TEXT("[MapGen] Falha ao executar mapgen.exe."));
		return;
	}

	Builder->BuildFromJson(MapJson);
}

// ── Internals ─────────────────────────────────────────────────────────────────

bool AMapGeneratorRuntime::PrepareConfig(int64 OverrideSeed, FString& OutConfigJson) const
{
	const FString ConfigPath = ResolvePath(PipelineConfigPath);

	FString RawJson;
	if (!FFileHelper::LoadFileToString(RawJson, *ConfigPath))
	{
		UE_LOG(LogTemp, Error, TEXT("[MapGen] Pipeline não encontrada: %s"), *ConfigPath);
		return false;
	}

	if (OverrideSeed != 0)
	{
		TSharedPtr<FJsonObject> Config;
		TSharedRef<TJsonReader<>> Reader = TJsonReaderFactory<>::Create(RawJson);
		if (!FJsonSerializer::Deserialize(Reader, Config) || !Config.IsValid())
		{
			UE_LOG(LogTemp, Error, TEXT("[MapGen] Falha ao parsear pipeline JSON."));
			return false;
		}

		Config->SetNumberField(TEXT("seed"), (double)OverrideSeed);

		TSharedRef<TJsonWriter<>> Writer = TJsonWriterFactory<>::Create(&OutConfigJson);
		FJsonSerializer::Serialize(Config.ToSharedRef(), Writer);
	}
	else
	{
		OutConfigJson = RawJson;
	}

	return true;
}

bool AMapGeneratorRuntime::RunMapgen(const FString& ConfigJson, FString& OutMapJson) const
{
	const FString ExePath = ResolvePath(ExecutablePath);
	if (!FPaths::FileExists(ExePath))
	{
		UE_LOG(LogTemp, Error, TEXT("[MapGen] mapgen.exe não encontrado: %s"), *ExePath);
		return false;
	}

	const FString TempConfig = FPaths::Combine(
		FPaths::ProjectSavedDir(),
		TEXT("MapGen_temp_config.json")
	);

	if (!FFileHelper::SaveStringToFile(ConfigJson, *TempConfig))
	{
		UE_LOG(LogTemp, Error, TEXT("[MapGen] Falha ao escrever config temporária: %s"), *TempConfig);
		return false;
	}

	const FString Params = FString::Printf(TEXT("-config \"%s\""), *TempConfig);
	int32 ExitCode = 0;
	FString StdErr;

	const bool bSuccess = FPlatformProcess::ExecProcess(
		*ExePath,
		*Params,
		&ExitCode,
		&OutMapJson,
		&StdErr
	);

	IFileManager::Get().Delete(*TempConfig);

	if (!bSuccess || ExitCode != 0)
	{
		UE_LOG(LogTemp, Error, TEXT("[MapGen] mapgen.exe falhou (código %d): %s"), ExitCode, *StdErr);
		return false;
	}

	if (OutMapJson.IsEmpty())
	{
		UE_LOG(LogTemp, Error, TEXT("[MapGen] mapgen.exe retornou JSON vazio. Stderr: %s"), *StdErr);
		return false;
	}

	return true;
}
