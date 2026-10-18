using System.Text.Json;
using ChatOS.Connector.Plugins;
using ChatOS.Core.Abstractions;

namespace ChatOS.Connector.LocalAgent;

/// <summary>
/// Executes the narrow local-source installer tools published only to the dedicated
/// plugin-installer contact. Authorization is re-derived from the persisted Run and
/// conversation binding for every call; a model-provided conversation id is never trusted.
/// </summary>
public sealed class WindowsLocalAgentPluginInstallerToolExecutor(
    ILocalAgentHostClient host,
    WindowsLocalAgentConversationClient conversations,
    ILocalPluginSourceInstaller installer)
{
    internal async Task<JsonElement> ExecuteAsync(
        string ownerUserId,
        WindowsLocalToolInvocation invocation,
        CancellationToken cancellationToken)
    {
        await AuthorizeAsync(ownerUserId, invocation.RunId, cancellationToken)
            .ConfigureAwait(false);
        return invocation.ToolName switch
        {
            WindowsLocalAgentCapabilityCatalog.PluginInstallerInspectNpmToolName
                when !invocation.SideEffecting => await InspectNpmAsync(
                    invocation.Arguments, cancellationToken).ConfigureAwait(false),
            WindowsLocalAgentCapabilityCatalog.PluginInstallerInstallNpmToolName
                when invocation.SideEffecting && !invocation.RequiresApproval => await InstallNpmAsync(
                    invocation.Arguments, cancellationToken).ConfigureAwait(false),
            WindowsLocalAgentCapabilityCatalog.PluginInstallerInspectSourceToolName
                when !invocation.SideEffecting => await InspectAsync(
                    invocation.Arguments, cancellationToken).ConfigureAwait(false),
            WindowsLocalAgentCapabilityCatalog.PluginInstallerInstallSourceToolName
                when invocation.SideEffecting && !invocation.RequiresApproval => await InstallAsync(
                    invocation.Arguments, cancellationToken).ConfigureAwait(false),
            _ => throw new InvalidOperationException(
                "The Plugin installer tool or safety classification is invalid."),
        };
    }

    private async Task<JsonElement> InspectNpmAsync(
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var draft = NpmDraft(arguments);
        var inspection = await installer.InspectNpmMcpAsync(draft, cancellationToken)
            .ConfigureAwait(false);
        return JsonSerializer.SerializeToElement(new
        {
            status = "inspection_passed",
            fingerprint = inspection.Fingerprint,
            display_name = inspection.DisplayName,
            version = inspection.Version,
            source_summary = inspection.SourceSummary,
            components = inspection.Components.Select(component => new
            {
                component_key = component.ComponentKey,
                display_name = component.DisplayName,
                kind = "mcp",
                tool_names = component.ToolNames,
            }).ToArray(),
            resolved_install_request = new
            {
                display_name = draft.DisplayName,
                description = draft.Description,
                component_key = draft.ComponentKey,
                package_name = draft.PackageName,
                package_version = draft.PackageVersion,
                binary_name = draft.BinaryName,
                arguments = draft.Arguments,
            },
        });
    }

    private async Task<JsonElement> InstallNpmAsync(
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var draft = NpmDraft(arguments);
        var expectedFingerprint = Fingerprint(arguments);
        var record = await installer.InstallNpmMcpAsync(
            draft, expectedFingerprint, cancellationToken).ConfigureAwait(false);
        return JsonSerializer.SerializeToElement(new
        {
            status = "installed",
            plugin_id = record.PluginId,
            plugin_key = record.PluginKey,
            release_id = record.ReleaseId,
            version = record.Version,
            artifact_sha256 = record.ArtifactSha256,
        });
    }

    private async Task AuthorizeAsync(
        string ownerUserId,
        string runId,
        CancellationToken cancellationToken)
    {
        var runResult = await host.SendAsync<GetLocalRunCommand, GetLocalRunResult>(
            new("get_run", ownerUserId, runId), cancellationToken).ConfigureAwait(false);
        if (runResult.Type != "run" ||
            !string.Equals(runResult.Run.OwnerUserId, ownerUserId, StringComparison.Ordinal) ||
            !TryConversationId(runResult.Run.Input, out var conversationId))
        {
            throw new InvalidOperationException("The Plugin installer Run context is invalid.");
        }
        var conversation = await conversations.GetAsync(
            ownerUserId, conversationId, cancellationToken).ConfigureAwait(false);
        if (!WindowsLocalAgentConversationCommandService.UsesPluginInstallerCapability(
                conversation.Conversation.Resource))
        {
            throw new InvalidOperationException(
                "Plugin installer tools are restricted to the dedicated installer contact.");
        }
    }

    private async Task<JsonElement> InspectAsync(
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var source = Source(arguments);
        var inspection = await installer.InspectAsync(source, cancellationToken)
            .ConfigureAwait(false);
        return JsonSerializer.SerializeToElement(new
        {
            status = "inspection_passed",
            fingerprint = inspection.Fingerprint,
            display_name = inspection.DisplayName,
            version = inspection.Version,
            source_summary = inspection.SourceSummary,
            components = inspection.Components.Select(component => new
            {
                component_key = component.ComponentKey,
                display_name = component.DisplayName,
                kind = component.Kind == LocalPluginInspectionComponentKind.Mcp ? "mcp" : "skill",
                tool_names = component.ToolNames,
            }).ToArray(),
            resolved_source = new
            {
                source_kind = source.Kind is LocalPluginSourceKind.GitRepository
                    ? "git_repository"
                    : "local_folder",
                location = source.Location,
                git_reference = source.GitReference,
                subdirectory = source.Subdirectory,
            },
        });
    }

    private async Task<JsonElement> InstallAsync(
        JsonElement arguments,
        CancellationToken cancellationToken)
    {
        var source = Source(arguments);
        var expectedFingerprint = Fingerprint(arguments);
        var record = await installer.InstallAsync(
            source, expectedFingerprint, cancellationToken).ConfigureAwait(false);
        return JsonSerializer.SerializeToElement(new
        {
            status = "installed",
            plugin_id = record.PluginId,
            plugin_key = record.PluginKey,
            release_id = record.ReleaseId,
            version = record.Version,
            artifact_sha256 = record.ArtifactSha256,
        });
    }

    private static NpmMcpDraft NpmDraft(JsonElement arguments)
    {
        if (arguments.ValueKind != JsonValueKind.Object ||
            !arguments.TryGetProperty("arguments", out var argumentValues) ||
            argumentValues.ValueKind != JsonValueKind.Array)
        {
            throw new InvalidOperationException("The npm MCP installer fields are invalid.");
        }
        var values = argumentValues.EnumerateArray().Select(value =>
        {
            if (value.ValueKind != JsonValueKind.String)
            {
                throw new InvalidOperationException("The npm MCP arguments are invalid.");
            }
            var text = value.GetString() ?? string.Empty;
            if (text.Length > 2_048 || text.IndexOf('\0') >= 0)
            {
                throw new InvalidOperationException("The npm MCP arguments are invalid.");
            }
            return text;
        }).ToArray();
        if (values.Length > 64)
        {
            throw new InvalidOperationException("The npm MCP arguments are invalid.");
        }
        var displayName = RequiredString(arguments, "display_name", 120);
        var description = arguments.TryGetProperty("description", out var descriptionValue) &&
            descriptionValue.ValueKind == JsonValueKind.String
                ? descriptionValue.GetString()?.Trim() ?? string.Empty
                : string.Empty;
        if (description.Length > 2_000 || description.IndexOf('\0') >= 0)
        {
            throw new InvalidOperationException("The npm MCP description is invalid.");
        }
        return new NpmMcpDraft(
            displayName,
            description,
            RequiredString(arguments, "component_key", 128),
            RequiredString(arguments, "package_name", 214),
            RequiredString(arguments, "package_version", 128),
            RequiredString(arguments, "binary_name", 128),
            values);
    }

    private static string Fingerprint(JsonElement arguments)
    {
        var expectedFingerprint = RequiredString(arguments, "expected_fingerprint", 64);
        if (expectedFingerprint.Length != 64 ||
            expectedFingerprint.Any(character =>
                character is not (>= '0' and <= '9') and not (>= 'a' and <= 'f')))
        {
            throw new InvalidOperationException("The preflight fingerprint is invalid.");
        }
        return expectedFingerprint;
    }

    private static LocalPluginSourceDraft Source(JsonElement arguments)
    {
        if (arguments.ValueKind != JsonValueKind.Object)
        {
            throw new InvalidOperationException("The Plugin source fields are invalid.");
        }
        var kind = RequiredString(arguments, "source_kind", 32) switch
        {
            "local_folder" => LocalPluginSourceKind.LocalFolder,
            "git_repository" => LocalPluginSourceKind.GitRepository,
            _ => throw new InvalidOperationException(
                "This Windows installer accepts only local_folder or git_repository sources."),
        };
        var gitReference = OptionalString(arguments, "git_reference", 256);
        if (kind is LocalPluginSourceKind.GitRepository && gitReference is null)
        {
            throw new InvalidOperationException("A Git Plugin source requires an explicit git_reference.");
        }
        return new LocalPluginSourceDraft(
            kind,
            RequiredString(arguments, "location", 4_096),
            GitReference: gitReference,
            Subdirectory: OptionalString(arguments, "subdirectory", 1_024));
    }

    private static string? OptionalString(JsonElement arguments, string name, int maximumLength)
    {
        if (!arguments.TryGetProperty(name, out var property) || property.ValueKind == JsonValueKind.Null)
        {
            return null;
        }
        if (property.ValueKind != JsonValueKind.String)
        {
            throw new InvalidOperationException($"The Plugin installer field '{name}' is invalid.");
        }
        var value = property.GetString()?.Trim() ?? string.Empty;
        if (value.Length == 0) return null;
        if (value.Length > maximumLength || value.IndexOf('\0') >= 0)
        {
            throw new InvalidOperationException($"The Plugin installer field '{name}' is invalid.");
        }
        return value;
    }

    private static string RequiredString(
        JsonElement arguments,
        string name,
        int maximumLength)
    {
        if (!arguments.TryGetProperty(name, out var property) ||
            property.ValueKind != JsonValueKind.String)
        {
            throw new InvalidOperationException($"The Plugin installer field '{name}' is invalid.");
        }
        var value = property.GetString()?.Trim() ?? string.Empty;
        if (value.Length == 0 || value.Length > maximumLength || value.IndexOf('\0') >= 0)
        {
            throw new InvalidOperationException($"The Plugin installer field '{name}' is invalid.");
        }
        return value;
    }

    private static bool TryConversationId(JsonElement input, out string conversationId)
    {
        conversationId = string.Empty;
        if (input.ValueKind != JsonValueKind.Object) return false;
        if (!input.TryGetProperty("source_conversation_id", out var value) &&
            !input.TryGetProperty("conversation_id", out value)) return false;
        if (value.ValueKind != JsonValueKind.String) return false;
        conversationId = value.GetString() ?? string.Empty;
        return conversationId.Length > 0;
    }
}
