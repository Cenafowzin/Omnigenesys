using UnrealBuildTool;

public class Omnigenesys : ModuleRules
{
    public Omnigenesys(ReadOnlyTargetRules Target) : base(Target)
    {
        PCHUsage = PCHUsageMode.UseExplicitOrSharedPCHs;

        PublicDependencyModuleNames.AddRange(new string[]
        {
            "Core",
            "CoreUObject",
            "Engine",
            "Json",
            "JsonUtilities",
            "Projects",  // IPluginManager — para resolver o Content dir do plugin
        });
    }
}
