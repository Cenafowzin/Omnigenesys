#include "MapBuilder.h"
#include "TileRegistry.h"

#include "Components/InstancedStaticMeshComponent.h"
#include "Dom/JsonObject.h"
#include "Dom/JsonValue.h"
#include "Serialization/JsonReader.h"
#include "Serialization/JsonSerializer.h"

AMapBuilder::AMapBuilder()
{
	PrimaryActorTick.bCanEverTick = false;

	// Componente raiz invisível que serve de âncora para os filhos
	USceneComponent* Root = CreateDefaultSubobject<USceneComponent>(TEXT("Root"));
	SetRootComponent(Root);
}

void AMapBuilder::BeginPlay()
{
	Super::BeginPlay();
}

// ── API Pública ───────────────────────────────────────────────────────────────

void AMapBuilder::BuildFromJson(const FString& MapJson)
{
	if (!Registry)
	{
		UE_LOG(LogTemp, Error, TEXT("[MapBuilder] Registry não configurado."));
		return;
	}

	Clear();

	FMapData Map;
	if (!ParseMapJson(MapJson, Map))
	{
		UE_LOG(LogTemp, Error, TEXT("[MapBuilder] Falha ao parsear JSON do mapa."));
		return;
	}

	// Inicializa RNG com a seed do mapa para variantes determinísticas
	FRandomStream Rng(bDeterministicVariants ? (int32)Map.Seed : FMath::Rand());

	for (const FMapLayerData& Layer : Map.Layers)
	{
		ProcessLayer(Layer, Map, Rng);
	}

	UE_LOG(LogTemp, Log, TEXT("[MapBuilder] Mapa %dx%d (seed %lld) construído. %d actors, %d tipos de mesh."),
		Map.Width, Map.Height, Map.Seed,
		SpawnedActors.Num(), MeshInstances.Num());
}

void AMapBuilder::Clear()
{
	// Destroi actors spawned
	for (AActor* Actor : SpawnedActors)
	{
		if (IsValid(Actor))
		{
			Actor->Destroy();
		}
	}
	SpawnedActors.Empty();

	// Remove os InstancedStaticMeshComponents
	for (auto& Pair : MeshInstances)
	{
		if (IsValid(Pair.Value))
		{
			Pair.Value->ClearInstances();
			Pair.Value->DestroyComponent();
		}
	}
	MeshInstances.Empty();
}

// ── Internals ─────────────────────────────────────────────────────────────────

bool AMapBuilder::ParseMapJson(const FString& Json, FMapData& OutMap) const
{
	TSharedPtr<FJsonObject> Root;
	TSharedRef<TJsonReader<>> Reader = TJsonReaderFactory<>::Create(Json);

	if (!FJsonSerializer::Deserialize(Reader, Root) || !Root.IsValid())
	{
		return false;
	}

	OutMap.Width  = Root->GetIntegerField(TEXT("width"));
	OutMap.Height = Root->GetIntegerField(TEXT("height"));
	OutMap.Seed   = (int64)Root->GetNumberField(TEXT("seed"));

	const TArray<TSharedPtr<FJsonValue>>* LayersArray;
	if (!Root->TryGetArrayField(TEXT("layers"), LayersArray))
	{
		return false;
	}

	for (const TSharedPtr<FJsonValue>& LayerVal : *LayersArray)
	{
		const TSharedPtr<FJsonObject>* LayerObj;
		if (!LayerVal->TryGetObject(LayerObj)) continue;

		FMapLayerData LayerData;
		LayerData.Name = (*LayerObj)->GetStringField(TEXT("name"));

		const TArray<TSharedPtr<FJsonValue>>* RowsArray;
		if (!(*LayerObj)->TryGetArrayField(TEXT("cells"), RowsArray)) continue;

		for (const TSharedPtr<FJsonValue>& RowVal : *RowsArray)
		{
			const TArray<TSharedPtr<FJsonValue>>* ColsArray;
			if (!RowVal->TryGetArray(ColsArray)) continue;

			TArray<FMapCellData> Row;
			for (const TSharedPtr<FJsonValue>& CellVal : *ColsArray)
			{
				const TSharedPtr<FJsonObject>* CellObj;
				FMapCellData Cell;

				if (CellVal->TryGetObject(CellObj))
				{
					(*CellObj)->TryGetStringField(TEXT("type"), Cell.Type);
				}
				Row.Add(Cell);
			}
			LayerData.Cells.Add(Row);
		}

		OutMap.Layers.Add(LayerData);
	}

	return true;
}

void AMapBuilder::ProcessLayer(const FMapLayerData& Layer, const FMapData& Map, FRandomStream& Rng)
{
	// Actors são spawnados uma vez por região contígua (ex: estrutura completa no centro)
	SpawnActorRegions(Layer, Rng);

	// Meshes são instanciados por célula (chão, vegetação, etc.)
	for (int32 Row = 0; Row < Layer.Cells.Num(); ++Row)
	{
		const TArray<FMapCellData>& RowData = Layer.Cells[Row];
		for (int32 Col = 0; Col < RowData.Num(); ++Col)
		{
			const FString& TileType = RowData[Col].Type;
			if (TileType.IsEmpty()) continue;

			if (UStaticMesh* Mesh = Registry->GetMesh(TileType))
			{
				UMaterialInterface* Mat = Registry->GetMeshMaterial(TileType);
				UInstancedStaticMeshComponent* ISM = GetOrCreateMeshComponent(TileType, Mesh, Mat);
				FTransform InstanceTransform(FRotator::ZeroRotator, TileToWorld(Col, Row), FVector::OneVector);
				ISM->AddInstance(InstanceTransform);
			}
		}
	}
}

void AMapBuilder::SpawnActorRegions(const FMapLayerData& Layer, FRandomStream& Rng)
{
	const int32 NumRows = Layer.Cells.Num();
	if (NumRows == 0) return;
	const int32 NumCols = Layer.Cells[0].Num();

	// Marca células já processadas para não spawnar duas vezes
	TArray<TArray<bool>> Visited;
	Visited.SetNum(NumRows);
	for (auto& Row : Visited) Row.SetNumZeroed(NumCols);

	for (int32 Row = 0; Row < NumRows; ++Row)
	{
		for (int32 Col = 0; Col < NumCols; ++Col)
		{
			if (Visited[Row][Col]) continue;

			const FString& TileType = Layer.Cells[Row][Col].Type;
			if (TileType.IsEmpty()) continue;

			// Só processa tipos que têm mapeamento de actor
			TSubclassOf<AActor> ActorClass = Registry->GetActorClass(TileType, Rng);
			if (!ActorClass) continue;

			// BFS para encontrar toda a região contígua deste tipo
			int32 MinCol = Col, MaxCol = Col, MinRow = Row, MaxRow = Row;

			TQueue<TPair<int32,int32>> Queue;
			Queue.Enqueue({Col, Row});
			Visited[Row][Col] = true;

			while (!Queue.IsEmpty())
			{
				TPair<int32,int32> Current;
				Queue.Dequeue(Current);
				const int32 C = Current.Key;
				const int32 R = Current.Value;

				MinCol = FMath::Min(MinCol, C);
				MaxCol = FMath::Max(MaxCol, C);
				MinRow = FMath::Min(MinRow, R);
				MaxRow = FMath::Max(MaxRow, R);

				// 4-vizinhos
				const TPair<int32,int32> Neighbors[] = {{C+1,R},{C-1,R},{C,R+1},{C,R-1}};
				for (const auto& N : Neighbors)
				{
					const int32 NC = N.Key, NR = N.Value;
					if (NR < 0 || NR >= NumRows || NC < 0 || NC >= NumCols) continue;
					if (Visited[NR][NC]) continue;
					if (Layer.Cells[NR][NC].Type != TileType) continue;
					Visited[NR][NC] = true;
					Queue.Enqueue({NC, NR});
				}
			}

			// Spawna 1 actor no centro da região
			const FVector WorldCenter = RegionCenterToWorld(MinCol, MinRow, MaxCol, MaxRow);

			FActorSpawnParameters Params;
			Params.Owner = this;
			AActor* Spawned = GetWorld()->SpawnActor<AActor>(ActorClass, WorldCenter, FRotator::ZeroRotator, Params);
			if (Spawned)
			{
				Spawned->AttachToActor(this, FAttachmentTransformRules::KeepWorldTransform);
				SpawnedActors.Add(Spawned);
			}
		}
	}
}

FVector AMapBuilder::TileToWorld(int32 Col, int32 Row) const
{
	return FVector(-Col * TileSize, -Row * TileSize, BaseZ);
}

FVector AMapBuilder::RegionCenterToWorld(int32 MinCol, int32 MinRow, int32 MaxCol, int32 MaxRow) const
{
	const float CenterCol = (MinCol + MaxCol) * 0.5f;
	const float CenterRow = (MinRow + MaxRow) * 0.5f;
	return FVector(-(CenterCol - 0.5f) * TileSize, -(CenterRow - 0.5f) * TileSize, BaseZ);
}

UInstancedStaticMeshComponent* AMapBuilder::GetOrCreateMeshComponent(
	const FString& TileType, UStaticMesh* Mesh, UMaterialInterface* Mat)
{
	if (TObjectPtr<UInstancedStaticMeshComponent>* Existing = MeshInstances.Find(TileType))
	{
		return Existing->Get();
	}

	UInstancedStaticMeshComponent* ISM = NewObject<UInstancedStaticMeshComponent>(this);
	ISM->SetStaticMesh(Mesh);
	if (Mat)
	{
		ISM->SetMaterial(0, Mat);
	}
	ISM->RegisterComponent();
	ISM->AttachToComponent(GetRootComponent(), FAttachmentTransformRules::KeepRelativeTransform);

	MeshInstances.Add(TileType, ISM);
	return ISM;
}
