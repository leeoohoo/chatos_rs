using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using ChatOS.Connector.Gateway;
using ChatOS.Connector.Runtime;
using ChatOS.Connector.Workspaces;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.Plugins;

public interface ILocalPluginApplicationService
{
    Task<IReadOnlyList<LocalPluginApplication>> ListAsync(
        CancellationToken cancellationToken = default);

    Task<LocalPluginApplicationLaunch> LaunchAsync(
        string pluginId,
        string componentKey,
        string expectedOwnerUserId,
        ProjectContextSnapshot? projectContext,
        CancellationToken cancellationToken = default);

    Task StopAllAsync();
}

internal sealed class LocalPluginApplicationService(
    ConnectorRuntimeContext runtime,
    IConnectorGatewayClient gateway,
    IInstalledPluginStore installed,
    PluginManifestLoader manifestLoader,
    WindowsPluginApplicationRuntime applicationRuntime) : ILocalPluginApplicationService
{
    public async Task<IReadOnlyList<LocalPluginApplication>> ListAsync(
        CancellationToken cancellationToken = default)
    {
        var session = await RequireSessionAsync(cancellationToken).ConfigureAwait(false);
        var sources = await gateway.ListPluginSourcesAsync(
            session.GatewayBaseUri,
            session.AccessToken,
            cancellationToken).ConfigureAwait(false);
        var records = await installed.ListAsync(cancellationToken).ConfigureAwait(false);
        var sourceById = sources.ToDictionary(value => value.Catalog.Id, StringComparer.Ordinal);
        var applications = new List<LocalPluginApplication>();
        foreach (var record in records)
        {
            if (!sourceById.TryGetValue(record.PluginId, out var source) ||
                source.Preference?.Enabled == false)
            {
                continue;
            }
            applications.AddRange(await manifestLoader.ListApplicationsAsync(record, cancellationToken)
                .ConfigureAwait(false));
        }
        return applications
            .OrderBy(value => value.DisplayName, StringComparer.CurrentCultureIgnoreCase)
            .ThenBy(value => value.Id, StringComparer.Ordinal)
            .ToArray();
    }

    public async Task<LocalPluginApplicationLaunch> LaunchAsync(
        string pluginId,
        string componentKey,
        string expectedOwnerUserId,
        ProjectContextSnapshot? projectContext,
        CancellationToken cancellationToken = default)
    {
        var session = await RequireSessionAsync(cancellationToken).ConfigureAwait(false);
        var state = runtime.Snapshot.State
            ?? throw new PluginRuntimeException("Local Connector is not paired.");
        if (!string.Equals(state.User.Id, expectedOwnerUserId, StringComparison.Ordinal))
        {
            throw new OperationCanceledException("The signed-in account changed.");
        }
        var sources = await gateway.ListPluginSourcesAsync(
            session.GatewayBaseUri,
            session.AccessToken,
            cancellationToken).ConfigureAwait(false);
        var source = sources.FirstOrDefault(value =>
            string.Equals(value.Catalog.Id, pluginId, StringComparison.Ordinal));
        if (source is null)
        {
            throw new PluginRuntimeException("Plugin is no longer available in the catalog.");
        }
        if (source.Preference?.Enabled == false)
        {
            throw new PluginRuntimeException("Plugin is disabled.");
        }
        var record = await installed.GetAsync(pluginId, cancellationToken).ConfigureAwait(false)
            ?? throw new PluginRuntimeException("Plugin is not installed.");

        string? workspaceId = null;
        string? workspaceRoot = null;
        string? projectId = null;
        string? projectName = null;
        if (projectContext is not null)
        {
            if (projectContext.SchemaVersion != 1 ||
                !string.Equals(projectContext.ExecutionTarget.DeviceId, state.DeviceId, StringComparison.Ordinal))
            {
                throw new PluginRuntimeException("Plugin project context does not belong to this device.");
            }
            var workspace = state.Workspaces.FirstOrDefault(value =>
                string.Equals(value.Id, projectContext.ExecutionTarget.WorkspaceId, StringComparison.Ordinal))
                ?? throw new PluginRuntimeException("Plugin project workspace is not authorized.");
            workspaceId = workspace.Id;
            workspaceRoot = new WorkspacePathGuard(workspace.AbsoluteRoot).ResolveExisting(
                string.IsNullOrEmpty(projectContext.ExecutionTarget.RelativeRoot)
                    ? "."
                    : projectContext.ExecutionTarget.RelativeRoot);
            if (!Directory.Exists(workspaceRoot))
            {
                throw new PluginRuntimeException("Plugin project root is unavailable.");
            }
            projectId = projectContext.ProjectId;
            projectName = projectContext.ProjectName;
        }

        var prepared = await manifestLoader.PrepareApplicationAsync(
            record,
            componentKey,
            record.DeclaredPermissions.ToHashSet(StringComparer.Ordinal),
            state.User.Id,
            state.DeviceId,
            workspaceId,
            workspaceRoot,
            projectId,
            projectName,
            cancellationToken).ConfigureAwait(false);
        if (!ReferenceEquals(runtime.Snapshot.State, state))
        {
            throw new OperationCanceledException("Local Connector context changed.");
        }
        return await applicationRuntime.LaunchAsync(prepared, cancellationToken).ConfigureAwait(false);
    }

    public Task StopAllAsync() => applicationRuntime.StopAllAsync();

    private async Task<ConnectorSessionConfiguration> RequireSessionAsync(
        CancellationToken cancellationToken)
    {
        await runtime.InitializeAsync(cancellationToken).ConfigureAwait(false);
        return await runtime.SessionConfigurationAsync(cancellationToken).ConfigureAwait(false)
            ?? throw new PluginRuntimeException("Local Connector is not paired.");
    }
}

internal sealed class WindowsPluginApplicationRuntime : IAsyncDisposable
{
    private sealed record RunningApplication(
        Process Process,
        Uri BaseUri,
        string HealthPath,
        string ReleaseId,
        string ArtifactSha256,
        Task OutputDrain,
        Task ErrorDrain);

    private readonly SemaphoreSlim _gate = new(1, 1);
    private readonly Dictionary<string, RunningApplication> _running = new(StringComparer.Ordinal);

    public async Task<LocalPluginApplicationLaunch> LaunchAsync(
        PreparedPluginApplication prepared,
        CancellationToken cancellationToken)
    {
        if (prepared.ExecutablePath is null)
        {
            return new LocalPluginApplicationLaunch(
                prepared.Application,
                new Uri(prepared.SourcePath),
                prepared.Record.ReleaseId,
                prepared.Record.Version,
                prepared.Record.ArtifactSha256);
        }

        var key = $"{prepared.Application.Id}:{prepared.ContextKey}";
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (_running.TryGetValue(key, out var current) &&
                string.Equals(current.ReleaseId, prepared.Record.ReleaseId, StringComparison.Ordinal) &&
                string.Equals(current.ArtifactSha256, prepared.Record.ArtifactSha256, StringComparison.OrdinalIgnoreCase) &&
                !current.Process.HasExited &&
                await IsHealthyAsync(current.BaseUri, current.HealthPath, cancellationToken).ConfigureAwait(false))
            {
                return LaunchResult(prepared, current.BaseUri);
            }
            Stop(key);

            var port = AvailableLoopbackPort();
            var baseUri = new Uri($"http://127.0.0.1:{port}/", UriKind.Absolute);
            var start = new ProcessStartInfo
            {
                FileName = prepared.ExecutablePath,
                WorkingDirectory = prepared.InstallationPath,
                UseShellExecute = false,
                CreateNoWindow = true,
                RedirectStandardInput = true,
                RedirectStandardOutput = true,
                RedirectStandardError = true,
            };
            foreach (var argument in prepared.Arguments) start.ArgumentList.Add(argument);
            foreach (var pair in prepared.Environment) start.Environment[pair.Key] = pair.Value;
            start.Environment["CHATOS_PLUGIN_APP_HOST"] = "127.0.0.1";
            start.Environment["CHATOS_PLUGIN_APP_PORT"] = port.ToString();
            var process = new Process { StartInfo = start, EnableRaisingEvents = true };
            try
            {
                if (!process.Start())
                {
                    throw new PluginRuntimeException("Plugin application process could not be started.");
                }
                // Drain for the whole process lifetime. The launch token only controls startup;
                // cancelling a page launch must not stop pipe consumption and deadlock a reused runtime.
                var outputDrain = process.StandardOutput.ReadToEndAsync();
                var errorDrain = process.StandardError.ReadToEndAsync();
                var running = new RunningApplication(
                    process,
                    baseUri,
                    prepared.HealthPath,
                    prepared.Record.ReleaseId,
                    prepared.Record.ArtifactSha256,
                    outputDrain,
                    errorDrain);
                _running[key] = running;
                await WaitUntilHealthyAsync(running, prepared.LaunchTimeoutMilliseconds, cancellationToken)
                    .ConfigureAwait(false);
                return LaunchResult(prepared, baseUri);
            }
            catch
            {
                if (_running.ContainsKey(key)) Stop(key);
                else process.Dispose();
                throw;
            }
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task StopPluginAsync(string pluginId)
    {
        await _gate.WaitAsync().ConfigureAwait(false);
        try
        {
            foreach (var key in _running.Keys
                         .Where(value => value.StartsWith($"{pluginId}:", StringComparison.Ordinal))
                         .ToArray())
            {
                Stop(key);
            }
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task StopAllAsync()
    {
        await _gate.WaitAsync().ConfigureAwait(false);
        try
        {
            foreach (var key in _running.Keys.ToArray()) Stop(key);
        }
        finally
        {
            _gate.Release();
        }
    }

    public async ValueTask DisposeAsync() => await StopAllAsync().ConfigureAwait(false);

    private static LocalPluginApplicationLaunch LaunchResult(PreparedPluginApplication prepared, Uri uri) =>
        new(prepared.Application, uri, prepared.Record.ReleaseId, prepared.Record.Version, prepared.Record.ArtifactSha256);

    private async Task WaitUntilHealthyAsync(
        RunningApplication running,
        int timeoutMilliseconds,
        CancellationToken cancellationToken)
    {
        using var timeout = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken);
        timeout.CancelAfter(TimeSpan.FromMilliseconds(Math.Clamp(timeoutMilliseconds, 100, 120_000)));
        while (true)
        {
            timeout.Token.ThrowIfCancellationRequested();
            if (running.Process.HasExited)
            {
                throw new PluginRuntimeException("Plugin application exited during startup.");
            }
            if (await IsHealthyAsync(running.BaseUri, running.HealthPath, timeout.Token).ConfigureAwait(false)) return;
            await Task.Delay(80, timeout.Token).ConfigureAwait(false);
        }
    }

    private static async Task<bool> IsHealthyAsync(
        Uri baseUri,
        string healthPath,
        CancellationToken cancellationToken)
    {
        using var client = new HttpClient(new HttpClientHandler
        {
            AllowAutoRedirect = false,
            AutomaticDecompression = DecompressionMethods.None,
            UseCookies = false,
        }) { Timeout = TimeSpan.FromMilliseconds(700) };
        try
        {
            using var response = await client.GetAsync(new Uri(baseUri, healthPath), cancellationToken)
                .ConfigureAwait(false);
            return response.IsSuccessStatusCode;
        }
        catch (Exception exception) when (exception is HttpRequestException or TaskCanceledException)
        {
            return false;
        }
    }

    private void Stop(string key)
    {
        if (!_running.Remove(key, out var running)) return;
        try
        {
            if (!running.Process.HasExited) running.Process.Kill(entireProcessTree: true);
        }
        catch (InvalidOperationException) { }
        catch (System.ComponentModel.Win32Exception) { }
        running.Process.Dispose();
    }

    private static int AvailableLoopbackPort()
    {
        var listener = new TcpListener(IPAddress.Loopback, 0);
        listener.Start();
        try { return ((IPEndPoint)listener.LocalEndpoint).Port; }
        finally { listener.Stop(); }
    }
}
