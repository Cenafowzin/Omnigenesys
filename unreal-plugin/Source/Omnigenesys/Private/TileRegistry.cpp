#include "TileRegistry.h"

UStaticMesh* UTileRegistry::GetMesh(const FString& TileType) const
{
	for (const FTileMeshMapping& Mapping : MeshMappings)
	{
		if (Mapping.TileType == TileType)
		{
			return Mapping.Mesh;
		}
	}
	return nullptr;
}

UMaterialInterface* UTileRegistry::GetMeshMaterial(const FString& TileType) const
{
	for (const FTileMeshMapping& Mapping : MeshMappings)
	{
		if (Mapping.TileType == TileType)
		{
			return Mapping.MaterialOverride;
		}
	}
	return nullptr;
}

TSubclassOf<AActor> UTileRegistry::GetActorClass(const FString& TileType, FRandomStream& Rng) const
{
	for (const FTileActorMapping& Mapping : ActorMappings)
	{
		if (Mapping.TileType == TileType && Mapping.Variants.Num() > 0)
		{
			const int32 Index = Rng.RandRange(0, Mapping.Variants.Num() - 1);
			return Mapping.Variants[Index];
		}
	}
	return nullptr;
}

bool UTileRegistry::HasMapping(const FString& TileType) const
{
	for (const FTileMeshMapping& M : MeshMappings)
	{
		if (M.TileType == TileType) return true;
	}
	for (const FTileActorMapping& M : ActorMappings)
	{
		if (M.TileType == TileType) return true;
	}
	return false;
}
