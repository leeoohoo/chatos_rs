namespace ChatOS.Connector.Sandbox;

internal static partial class WindowsAppContainerSandbox
{
    internal static async Task GrantPathAccessAsync(
        string path,
        string appContainerSid,
        ConnectorSandboxPermissionProfile profile,
        CancellationToken cancellationToken)
    {
        await EnsureWorkspaceAclAsync(path, appContainerSid, profile, cancellationToken)
            .ConfigureAwait(false);
        await EnsureAncestorTraverseAclsAsync(path, appContainerSid, cancellationToken)
            .ConfigureAwait(false);
    }

    private static async Task RegisterEphemeralProfileAsync(
        string profileName,
        EphemeralProfileState state,
        string workspaceRoot,
        string sid,
        string temporaryDirectory,
        CancellationToken cancellationToken)
    {
        await state.Gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            var metadata = new EphemeralProfileMetadata(
                profileName,
                workspaceRoot,
                sid,
                null,
                temporaryDirectory,
                DateTimeOffset.UtcNow);
            if (state.Metadata is not null &&
                (!string.Equals(state.Metadata.WorkspaceRoot, workspaceRoot, StringComparison.OrdinalIgnoreCase) ||
                 !string.Equals(state.Metadata.Sid, sid, StringComparison.Ordinal) ||
                 !string.Equals(
                     state.Metadata.TemporaryDirectory,
                     temporaryDirectory,
                     StringComparison.OrdinalIgnoreCase)))
            {
                throw new InvalidOperationException(
                    "Controlled AppContainer profile identity changed while it was active.");
            }
            state.Metadata ??= metadata;
            await SaveProfileMetadataAsync(state.Metadata, cancellationToken).ConfigureAwait(false);
        }
        finally
        {
            state.Gate.Release();
        }
    }

    private static async Task RegisterEphemeralAdditionalPathAsync(
        string profileName,
        EphemeralProfileState state,
        string path,
        string sid,
        CancellationToken cancellationToken)
    {
        await state.Gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (state.Metadata is null ||
                !string.Equals(state.Metadata.Sid, sid, StringComparison.Ordinal))
            {
                throw new InvalidOperationException("AppContainer profile was not registered.");
            }
            var fullPath = Path.GetFullPath(path);
            var additionalRoots = (state.Metadata.AdditionalRoots ?? Array.Empty<string>())
                .Append(fullPath)
                .Distinct(StringComparer.OrdinalIgnoreCase)
                .ToArray();
            state.Metadata = state.Metadata with { AdditionalRoots = additionalRoots };
            await SaveProfileMetadataAsync(state.Metadata, cancellationToken).ConfigureAwait(false);
        }
        finally
        {
            state.Gate.Release();
        }
    }
}
