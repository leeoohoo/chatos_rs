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

    private static async Task EnsureWorkspaceAclAsync(
        string workspaceRoot,
        string sid,
        ConnectorSandboxPermissionProfile profile,
        CancellationToken cancellationToken)
    {
        var root = Path.GetFullPath(workspaceRoot);
        if (!Directory.Exists(root))
        {
            throw new DirectoryNotFoundException("Sandbox workspace root was not found.");
        }
        var key = string.Join('\0', root, sid, profile);
        var preparation = PreparedWorkspaceAcls.GetOrAdd(
            key,
            _ => new Lazy<Task>(
                () => EnsurePathAclAsync(
                    root,
                    sid,
                    profile is ConnectorSandboxPermissionProfile.ReadOnly
                        ? "(OI)(CI)RX"
                        : "(OI)(CI)M",
                    cancellationToken),
                LazyThreadSafetyMode.ExecutionAndPublication));
        try
        {
            await preparation.Value.WaitAsync(cancellationToken).ConfigureAwait(false);
        }
        catch
        {
            PreparedWorkspaceAcls.TryRemove(new KeyValuePair<string, Lazy<Task>>(key, preparation));
            throw;
        }
    }

    private static async Task EnsurePathAclAsync(
        string root,
        string sid,
        string access,
        CancellationToken cancellationToken,
        bool recursive = false)
    {
        var executable = Path.Combine(Environment.SystemDirectory, "icacls.exe");
        if (!File.Exists(executable))
        {
            throw new FileNotFoundException("Windows ACL utility was not found.", executable);
        }
        var start = new System.Diagnostics.ProcessStartInfo
        {
            FileName = executable,
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        start.ArgumentList.Add(root);
        start.ArgumentList.Add("/grant:r");
        start.ArgumentList.Add($"*{sid}:{access}");
        if (recursive) start.ArgumentList.Add("/T");
        start.ArgumentList.Add("/C");
        if (recursive) start.ArgumentList.Add("/L");
        start.ArgumentList.Add("/Q");
        using var process = System.Diagnostics.Process.Start(start)
            ?? throw new InvalidOperationException("Unable to start Windows ACL preparation.");
        var stdout = process.StandardOutput.ReadToEndAsync(cancellationToken);
        var stderr = process.StandardError.ReadToEndAsync(cancellationToken);
        using var timeout = new CancellationTokenSource(
            recursive ? TimeSpan.FromMinutes(2) : TimeSpan.FromSeconds(10));
        using var linked = CancellationTokenSource.CreateLinkedTokenSource(
            cancellationToken,
            timeout.Token);
        try
        {
            await process.WaitForExitAsync(linked.Token).ConfigureAwait(false);
        }
        catch
        {
            if (!process.HasExited) process.Kill(entireProcessTree: true);
            throw;
        }
        var output = await stdout.ConfigureAwait(false);
        var error = await stderr.ConfigureAwait(false);
        if (process.ExitCode != 0)
        {
            throw new InvalidOperationException(
                $"Windows could not prepare the workspace sandbox ACL (icacls {process.ExitCode}): {SafeAclError(error, output)}");
        }
    }

    private static async Task RegisterEphemeralProfileAsync(
        string profileName,
        EphemeralProfileState state,
        string workspaceRoot,
        string sid,
        string temporaryDirectory,
        bool loopbackExempt,
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
                DateTimeOffset.UtcNow,
                LoopbackExempt: loopbackExempt,
                RecursiveAclMaterialization: false);
            if (state.Metadata is not null &&
                (!string.Equals(state.Metadata.WorkspaceRoot, workspaceRoot, StringComparison.OrdinalIgnoreCase) ||
                 !string.Equals(state.Metadata.Sid, sid, StringComparison.Ordinal) ||
                 !string.Equals(
                     state.Metadata.TemporaryDirectory,
                     temporaryDirectory,
                     StringComparison.OrdinalIgnoreCase) ||
                 state.Metadata.LoopbackExempt != loopbackExempt))
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

    private static async Task SetLoopbackExemptionAsync(
        string profileName,
        bool enabled,
        CancellationToken cancellationToken)
    {
        var executable = Path.Combine(Environment.SystemDirectory, "CheckNetIsolation.exe");
        if (!File.Exists(executable))
        {
            throw new FileNotFoundException(
                "Windows loopback isolation utility was not found.",
                executable);
        }
        var start = new System.Diagnostics.ProcessStartInfo
        {
            FileName = executable,
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        start.ArgumentList.Add("LoopbackExempt");
        start.ArgumentList.Add(enabled ? "-a" : "-d");
        start.ArgumentList.Add($"-n={profileName}");
        using var process = System.Diagnostics.Process.Start(start)
            ?? throw new InvalidOperationException("Unable to configure AppContainer loopback access.");
        var stdout = process.StandardOutput.ReadToEndAsync(cancellationToken);
        var stderr = process.StandardError.ReadToEndAsync(cancellationToken);
        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(10));
        using var linked = CancellationTokenSource.CreateLinkedTokenSource(
            cancellationToken,
            timeout.Token);
        try
        {
            await process.WaitForExitAsync(linked.Token).ConfigureAwait(false);
        }
        catch
        {
            if (!process.HasExited) process.Kill(entireProcessTree: true);
            throw;
        }
        var output = await stdout.ConfigureAwait(false);
        var error = await stderr.ConfigureAwait(false);
        if (process.ExitCode != 0)
        {
            throw new InvalidOperationException(
                $"Windows could not configure loopback-only plugin access ({process.ExitCode}): {SafeAclError(error, output)}");
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
