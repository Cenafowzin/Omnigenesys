Coloque aqui os arquivos do framework Go:

  mapgen.exe              <- binário compilado para Windows x64
  example_pipeline.json  <- pipeline de exemplo (ou qualquer pipeline JSON)

Para compilar o mapgen.exe:
  cd <repo>
  ./build.sh windows

O AMapGeneratorRuntime já aponta para esta pasta por padrão.
Para usar arquivos do seu próprio projeto, use o prefixo "project:" nos campos:
  ExecutablePath:      project:MapGen/mapgen.exe
  PipelineConfigPath:  project:MapGen/minha_pipeline.json
