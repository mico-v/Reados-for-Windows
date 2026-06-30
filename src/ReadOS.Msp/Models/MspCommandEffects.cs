namespace ReadOS.Msp.Models;

[Flags]
public enum MspCommandEffects
{
    None = 0,
    ReadWorkspace = 1,
    WriteWorkspace = 2,
    DeleteWorkspace = 4,
    CreateArtifact = 8,
    ExternalNetwork = 16,
    ExternalModel = 32
}
