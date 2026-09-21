using System.Collections.ObjectModel;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;

namespace ChatOS.Presentation.Projects;


public sealed partial class ProjectRunToolchainSelectionViewModel : ObservableObject
{
    public ProjectRunToolchainSelectionViewModel(
        string kind,
        IReadOnlyList<ProjectRunToolchainOption> options,
        string? selectedOptionId)
    {
        Kind = kind;
        Title = ToolchainTitle(kind);
        Options.Add(new ProjectRunToolchainChoice(
            string.Empty,
            options.FirstOrDefault()?.Label is { } automaticLabel
                ? $"自动 · {automaticLabel}"
                : "自动" ,
            options.FirstOrDefault()?.Path ?? string.Empty));
        foreach (var option in options)
        {
            Options.Add(new ProjectRunToolchainChoice(
                option.Id,
                string.IsNullOrWhiteSpace(option.Version)
                    ? option.Label
                    : $"{option.Label} · {option.Version}",
                option.Path));
        }

        if (!string.IsNullOrWhiteSpace(selectedOptionId) &&
            Options.All(value => value.Id != selectedOptionId))
        {
            Options.Add(new ProjectRunToolchainChoice(selectedOptionId, $"手动 · {selectedOptionId}", selectedOptionId));
        }

        _selectedOptionId = selectedOptionId ?? string.Empty;
    }

    public string Kind { get; }

    public string Title { get; }

    public ObservableCollection<ProjectRunToolchainChoice> Options { get; } = [];

    [ObservableProperty]
    private string? _selectedOptionId;

    public string SelectedPath => Options.FirstOrDefault(value => value.Id == SelectedOptionId)?.Path ?? string.Empty;

    partial void OnSelectedOptionIdChanged(string? value) => OnPropertyChanged(nameof(SelectedPath));

    private static string ToolchainTitle(string kind) => kind.ToLowerInvariant() switch
    {
        "java_home" => "JDK",
        "java" => "JDK / Java",
        "mvn" => "Maven",
        "gradle" => "Gradle",
        "python" => "Python",
        "node" => "Node.js",
        "npm" => "npm",
        "pnpm" => "pnpm",
        "yarn" => "Yarn",
        "cargo" => "Cargo",
        "go" => "Go",
        "swift" => "Swift",
        _ => kind,
    };
}

public sealed record ProjectRunToolchainChoice(string Id, string Label, string Path);

public sealed partial class ProjectRunEnvironmentVariableViewModel : ObservableObject
{
    public ProjectRunEnvironmentVariableViewModel(string key = "", string value = "")
    {
        _key = key;
        _value = value;
    }

    public Guid Id { get; } = Guid.NewGuid();

    [ObservableProperty]
    private string _key;

    [ObservableProperty]
    private string _value;
}
