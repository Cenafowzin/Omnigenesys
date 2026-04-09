#pragma once

#include "CoreMinimal.h"
#include "GameFramework/Actor.h"
#include "MapGeneratorRuntime.generated.h"

class AMapBuilder;

/**
 * AMapGeneratorRuntime executa o binário Go (mapgen.exe) como subprocess,
 * passa a pipeline JSON via arquivo temporário e entrega o mapa ao AMapBuilder.
 *
 * Fluxo:
 *   1. Lê o arquivo de pipeline JSON (PipelineConfigPath)
 *   2. Substitui a seed se necessário
 *   3. Escreve um arquivo temporário de config
 *   4. Executa: mapgen.exe -config <tempfile>
 *   5. Lê o JSON do mapa no stdout
 *   6. Chama Builder->BuildFromJson(mapJson)
 *
 * Setup:
 *   - Coloque mapgen.exe em Content/MapGen/mapgen.exe
 *   - Coloque o .json da pipeline em Content/MapGen/<pipeline>.json
 *   - Crie um Actor AMapGeneratorRuntime na cena e configure as referências
 */
UCLASS()
class OMNIGENESYS_API AMapGeneratorRuntime : public AActor
{
	GENERATED_BODY()

public:
	AMapGeneratorRuntime();

	// ── Configuração ──────────────────────────────────────────────────────────

	// Caminho para o arquivo JSON da pipeline, relativo ao Content/ do plugin.
	// Para usar um arquivo do seu projeto, prefixe com "project:" ex: "project:MapGen/minha_pipeline.json"
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category="MapGen|Pipeline")
	FString PipelineConfigPath = TEXT("MapGen/cornfield_pipeline.json");

	// Seed (0 = usar o valor do JSON, ou aleatório se o JSON também for 0)
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category="MapGen|Pipeline")
	int64 Seed = 0;

	// Referência ao MapBuilder na cena
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category="MapGen|Scene")
	TObjectPtr<AMapBuilder> Builder;

	// Caminho para mapgen.exe, relativo ao Content/ do plugin.
	// Para usar um executável do seu projeto, prefixe com "project:" ex: "project:MapGen/mapgen.exe"
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category="MapGen|Binario")
	FString ExecutablePath = TEXT("MapGen/mapgen.exe");

	// ── API pública ───────────────────────────────────────────────────────────

	// Gera usando a seed configurada no painel (ou a do JSON se Seed=0)
	UFUNCTION(BlueprintCallable, Category="MapGen")
	void Generate();

	// Gera com seed explícita (útil para save/load ou testes)
	UFUNCTION(BlueprintCallable, Category="MapGen")
	void GenerateWithSeed(int64 OverrideSeed);

protected:
	virtual void BeginPlay() override;

private:
	bool PrepareConfig(int64 OverrideSeed, FString& OutConfigJson) const;
	bool RunMapgen(const FString& ConfigJson, FString& OutMapJson) const;
};
