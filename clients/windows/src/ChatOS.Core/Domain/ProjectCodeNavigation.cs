namespace ChatOS.Core.Domain;

public sealed record ProjectCodeSymbolSelection(
    string Token,
    int Line,
    int Column);

public sealed record ProjectCodeNavigationLocation(
    string Path,
    string RelativePath,
    int Line,
    int Column,
    int EndLine,
    int EndColumn,
    string Preview,
    double Score);

public sealed record ProjectCodeNavigationResult(
    string Provider,
    string Language,
    string Mode,
    string? Token,
    IReadOnlyList<ProjectCodeNavigationLocation> Locations);
