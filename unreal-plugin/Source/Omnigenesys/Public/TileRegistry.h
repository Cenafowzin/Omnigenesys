#pragma once

#include "CoreMinimal.h"
#include "Engine/DataAsset.h"
#include "TileRegistry.generated.h"

/**
 * Mapeamento de um tipo de tile para um Static Mesh.
 * Usado para tiles de superfície (chão, paredes, vegetação).
 */
USTRUCT(BlueprintType)
struct FTileMeshMapping
{
	GENERATED_BODY()

	// Tipo de tile exportado pelo Go (ex: "floor", "wall")
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category="Tile")
	FString TileType;

	// Mesh a instanciar no grid para este tipo
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category="Tile")
	TObjectPtr<UStaticMesh> Mesh;

	// Material override opcional
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category="Tile")
	TObjectPtr<UMaterialInterface> MaterialOverride;
};

/**
 * Mapeamento de um tipo de tile para um ou mais Blueprint/Actor.
 * Usado para estruturas, entidades e props que precisam de lógica (colisão, interação).
 * Suporta variantes — o sistema escolhe aleatoriamente baseado na seed.
 */
USTRUCT(BlueprintType)
struct FTileActorMapping
{
	GENERATED_BODY()

	// Tipo de tile exportado pelo Go (ex: "estrutura_loja", "spawn")
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category="Actor")
	FString TileType;

	// Um ou mais Blueprints/classes de Actor como variantes
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category="Actor")
	TArray<TSubclassOf<AActor>> Variants;
};

/**
 * UTileRegistry é um Data Asset que mapeia strings de tipo de tile
 * para assets do Unreal (meshes ou actors).
 *
 * Como criar:
 *   Content Browser → clique direito → Miscellaneous → Data Asset → TileRegistry
 */
UCLASS(BlueprintType)
class OMNIGENESYS_API UTileRegistry : public UDataAsset
{
	GENERATED_BODY()

public:
	// Tiles de superfície → Static Mesh (ex: chão, vegetação baixa)
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category="Registry")
	TArray<FTileMeshMapping> MeshMappings;

	// Tiles de estrutura/entidade → Actor Blueprint (ex: lojas, spawn point)
	UPROPERTY(EditAnywhere, BlueprintReadWrite, Category="Registry")
	TArray<FTileActorMapping> ActorMappings;

	// Retorna o Static Mesh associado ao tipo, ou nullptr se não mapeado
	UStaticMesh* GetMesh(const FString& TileType) const;

	// Retorna o material override, ou nullptr se não houver
	UMaterialInterface* GetMeshMaterial(const FString& TileType) const;

	// Retorna uma classe de Actor (variante escolhida deterministicamente via Rng), ou nullptr
	TSubclassOf<AActor> GetActorClass(const FString& TileType, FRandomStream& Rng) const;

	// Retorna true se o tipo tem mapeamento de mesh OU de actor
	bool HasMapping(const FString& TileType) const;
};
