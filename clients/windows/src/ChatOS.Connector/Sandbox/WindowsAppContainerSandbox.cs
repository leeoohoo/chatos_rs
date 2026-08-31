using System.Collections.Concurrent;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace ChatOS.Connector.Sandbox;

internal sealed class WindowsAppContainerLaunchContext : IDisposable, IAsyncDisposable
{
    private readonly List<IntPtr> _ownedSids;
    private readonly IntPtr _capabilityArray;
    private IAsyncDisposable? _profileLease;
    private int _disposed;

    public WindowsAppContainerLaunchContext(
        IntPtr appContainerSid,
        string appContainerSidText,
        IReadOnlyList<IntPtr> capabilitySids,
        string temporaryDirectory,
        SandboxExecutionPolicy policy,
        IAsyncDisposable? profileLease = null)
    {
        _ownedSids = [appContainerSid, .. capabilitySids];
        if (capabilitySids.Count > 0)
        {
            var itemSize = Marshal.SizeOf<SidAndAttributes>();
            _capabilityArray = Marshal.AllocHGlobal(checked(itemSize * capabilitySids.Count));
            for (var index = 0; index < capabilitySids.Count; index++)
            {
                Marshal.StructureToPtr(
                    new SidAndAttributes(capabilitySids[index], WindowsAppContainerSandbox.SeGroupEnabled),
                    _capabilityArray + index * itemSize,
                    fDeleteOld: false);
            }
        }

        SecurityCapabilities = Marshal.AllocHGlobal(Marshal.SizeOf<SecurityCapabilities>());
        Marshal.StructureToPtr(new SecurityCapabilities(
            appContainerSid,
            _capabilityArray,
            checked((uint)capabilitySids.Count),
            0), SecurityCapabilities, fDeleteOld: false);
        EnvironmentBlock = BuildEnvironmentBlock(temporaryDirectory, policy);
        AppContainerSid = appContainerSidText;
        _profileLease = profileLease;
    }

    public string AppContainerSid { get; }

    public IntPtr SecurityCapabilities { get; }

    public IntPtr EnvironmentBlock { get; }

    internal IAsyncDisposable? DetachProfileLease() =>
        Interlocked.Exchange(ref _profileLease, null);

    public void Dispose() => DisposeAsync().AsTask().GetAwaiter().GetResult();

    public async ValueTask DisposeAsync()
    {
        if (Interlocked.Exchange(ref _disposed, 1) != 0)
        {
            return;
        }
        if (SecurityCapabilities != IntPtr.Zero)
        {
            Marshal.FreeHGlobal(SecurityCapabilities);
        }
        if (EnvironmentBlock != IntPtr.Zero)
        {
            Marshal.FreeHGlobal(EnvironmentBlock);
        }
        if (_capabilityArray != IntPtr.Zero)
        {
            Marshal.FreeHGlobal(_capabilityArray);
        }
        foreach (var sid in _ownedSids)
        {
            if (sid != IntPtr.Zero)
            {
                _ = WindowsAppContainerSandbox.FreeSid(sid);
            }
        }
        var profileLease = Interlocked.Exchange(ref _profileLease, null);
        if (profileLease is not null)
        {
            await profileLease.DisposeAsync().ConfigureAwait(false);
        }
    }

    private static IntPtr BuildEnvironmentBlock(
        string temporaryDirectory,
        SandboxExecutionPolicy policy)
    {
        var variables = BuildEnvironmentVariables(temporaryDirectory, policy);
        var block = string.Join('\0', variables.Select(pair => $"{pair.Key}={pair.Value}")) + "\0\0";
        return Marshal.StringToHGlobalUni(block);
    }

    internal static IReadOnlyDictionary<string, string> BuildEnvironmentVariables(
        string temporaryDirectory,
        SandboxExecutionPolicy policy)
    {
        var systemRoot = Environment.GetFolderPath(Environment.SpecialFolder.Windows);
        var variables = new SortedDictionary<string, string>(StringComparer.OrdinalIgnoreCase)
        {
            ["CHATOS_SANDBOX"] = "1",
            ["CHATOS_SANDBOX_NETWORK"] = policy.NetworkAccess.ToString(),
            ["CHATOS_SANDBOX_PROFILE"] = policy.PermissionProfile.ToString(),
            ["ComSpec"] = Environment.GetEnvironmentVariable("ComSpec")
                ?? Path.Combine(systemRoot, "System32", "cmd.exe"),
            ["PATH"] = Environment.GetEnvironmentVariable("PATH") ?? string.Empty,
            ["PATHEXT"] = Environment.GetEnvironmentVariable("PATHEXT") ?? ".COM;.EXE;.BAT;.CMD",
            ["SystemRoot"] = systemRoot,
            ["TEMP"] = temporaryDirectory,
            ["TMP"] = temporaryDirectory,
            ["WINDIR"] = systemRoot,
        };
        return variables;
    }
}

[StructLayout(LayoutKind.Sequential)]
internal readonly record struct SecurityCapabilities(
    IntPtr AppContainerSid,
    IntPtr Capabilities,
    uint CapabilityCount,
    uint Reserved);

[StructLayout(LayoutKind.Sequential)]
internal readonly record struct SidAndAttributes(IntPtr Sid, uint Attributes);

internal static class WindowsAppContainerSandbox
{
    internal const uint SeGroupEnabled = 0x0000_0004;
    internal const nuint ProcThreadAttributeSecurityCapabilities = 0x0002_0009;
    private const int ErrorAlreadyExistsHResult = unchecked((int)0x800700B7);
    private const string InternetClientSid = "S-1-15-3-1";
    private const string PrivateNetworkClientServerSid = "S-1-15-3-3";
    private static readonly ConcurrentDictionary<string, Lazy<Task>> PreparedWorkspaceAcls =
        new(StringComparer.OrdinalIgnoreCase);
    private static readonly ConcurrentDictionary<string, EphemeralProfileState> EphemeralProfiles =
        new(StringComparer.OrdinalIgnoreCase);
    private static readonly SemaphoreSlim StaleCleanupGate = new(1, 1);
    private static int _staleCleanupStarted;

    public static Task<WindowsAppContainerLaunchContext> PrepareAsync(
        string workspaceRoot,
        SandboxExecutionPolicy policy,
        CancellationToken cancellationToken) =>
        PrepareAsync(workspaceRoot, policy, isolationKey: null, cancellationToken);

    public static async Task<WindowsAppContainerLaunchContext> PrepareAsync(
        string workspaceRoot,
        SandboxExecutionPolicy policy,
        string? isolationKey,
        CancellationToken cancellationToken)
    {
        if (!OperatingSystem.IsWindows())
        {
            throw new PlatformNotSupportedException("Windows AppContainer sandboxing requires Windows.");
        }
        if (!policy.UseAppContainer ||
            policy.PermissionProfile is ConnectorSandboxPermissionProfile.FullAccess)
        {
            throw new InvalidOperationException("An AppContainer context requires a restricted profile.");
        }

        var profileName = ProfileName(workspaceRoot, policy.PermissionProfile, isolationKey);
        EphemeralProfileLease? profileLease = null;
        if (!string.IsNullOrWhiteSpace(isolationKey))
        {
            _ = CleanupStaleControlledProfilesOnceAsync();
            profileLease = await AcquireEphemeralProfileAsync(profileName, cancellationToken)
                .ConfigureAwait(false);
        }
        IntPtr appContainerSid = IntPtr.Zero;
        try
        {
            appContainerSid = CreateOrDeriveProfileSid(profileName);
            var sidText = SidToString(appContainerSid);
            await EnsureWorkspaceAclAsync(
                workspaceRoot,
                sidText,
                policy.PermissionProfile,
                cancellationToken).ConfigureAwait(false);
            var temporaryDirectory = Path.Combine(
                Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
                "ChatOS",
                "WindowsClient",
                "SandboxTemp",
                profileName);
            Directory.CreateDirectory(temporaryDirectory);
            await EnsurePathAclAsync(
                temporaryDirectory,
                sidText,
                "(OI)(CI)M",
                cancellationToken).ConfigureAwait(false);
            if (profileLease is not null)
            {
                await profileLease.RegisterAsync(
                    Path.GetFullPath(workspaceRoot),
                    sidText,
                    temporaryDirectory,
                    cancellationToken).ConfigureAwait(false);
            }
            var capabilities = policy.GrantInternetCapabilities
                ? new[] { CapabilitySid(InternetClientSid), CapabilitySid(PrivateNetworkClientServerSid) }
                : Array.Empty<IntPtr>();
            return new WindowsAppContainerLaunchContext(
                appContainerSid,
                sidText,
                capabilities,
                temporaryDirectory,
                policy,
                profileLease);
        }
        catch
        {
            if (appContainerSid != IntPtr.Zero)
            {
                _ = FreeSid(appContainerSid);
            }
            if (profileLease is not null)
            {
                await profileLease.DisposeAsync().ConfigureAwait(false);
            }
            throw;
        }
    }

    internal static string ProfileName(
        string workspaceRoot,
        ConnectorSandboxPermissionProfile profile,
        string? isolationKey = null)
    {
        if (profile is ConnectorSandboxPermissionProfile.FullAccess)
        {
            throw new ArgumentException("Full access does not use an AppContainer profile.", nameof(profile));
        }
        var root = Path.TrimEndingDirectorySeparator(Path.GetFullPath(workspaceRoot));
        if (OperatingSystem.IsWindows()) root = root.ToUpperInvariant();
        var digest = Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(root)))
            .ToLowerInvariant()[..32];
        var permission = profile is ConnectorSandboxPermissionProfile.ReadOnly ? "r" : "w";
        if (string.IsNullOrWhiteSpace(isolationKey))
        {
            return $"ChatOS.Sandbox.{permission}.v2.{digest}";
        }
        var isolationDigest = Convert.ToHexString(SHA256.HashData(
            Encoding.UTF8.GetBytes(isolationKey.Trim()))).ToLowerInvariant()[..24];
        return $"ChatOS.Sandbox.{permission}.controlled.v1.{digest}.{isolationDigest}";
    }

    internal static bool HasPendingProfileCleanup(string profileName) =>
        EphemeralProfiles.ContainsKey(profileName) || File.Exists(ProfileMetadataPath(profileName));

    private static IntPtr CreateOrDeriveProfileSid(string profileName)
    {
        var result = CreateAppContainerProfile(
            profileName,
            "ChatOS Windows command sandbox",
            "Isolated command execution for the ChatOS Windows client.",
            IntPtr.Zero,
            0,
            out var sid);
        if (result == 0)
        {
            return sid;
        }
        if (result != ErrorAlreadyExistsHResult)
        {
            Marshal.ThrowExceptionForHR(result);
        }

        result = DeriveAppContainerSidFromAppContainerName(profileName, out sid);
        if (result != 0)
        {
            Marshal.ThrowExceptionForHR(result);
        }
        return sid;
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
                    profile is ConnectorSandboxPermissionProfile.ReadOnly ? "(OI)(CI)RX" : "(OI)(CI)M",
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
        CancellationToken cancellationToken)
    {
        var systemDirectory = Environment.GetFolderPath(Environment.SpecialFolder.System);
        var icacls = Path.Combine(systemDirectory, "icacls.exe");
        if (!File.Exists(icacls))
        {
            throw new FileNotFoundException("Windows ACL utility was not found.", icacls);
        }

        var start = new ProcessStartInfo
        {
            FileName = icacls,
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        start.ArgumentList.Add(root);
        start.ArgumentList.Add("/grant:r");
        start.ArgumentList.Add($"*{sid}:{access}");
        start.ArgumentList.Add("/T");
        start.ArgumentList.Add("/C");
        start.ArgumentList.Add("/L");
        start.ArgumentList.Add("/Q");
        using var process = Process.Start(start)
            ?? throw new InvalidOperationException("Unable to start Windows ACL preparation.");
        var stdout = process.StandardOutput.ReadToEndAsync(cancellationToken);
        var stderr = process.StandardError.ReadToEndAsync(cancellationToken);
        using var timeout = new CancellationTokenSource(TimeSpan.FromMinutes(2));
        using var linked = CancellationTokenSource.CreateLinkedTokenSource(
            cancellationToken,
            timeout.Token);
        try
        {
            await process.WaitForExitAsync(linked.Token).ConfigureAwait(false);
        }
        catch
        {
            if (!process.HasExited)
            {
                process.Kill(entireProcessTree: true);
            }
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

    private static async Task<EphemeralProfileLease> AcquireEphemeralProfileAsync(
        string profileName,
        CancellationToken cancellationToken)
    {
        while (true)
        {
            var state = EphemeralProfiles.GetOrAdd(profileName, static _ => new EphemeralProfileState());
            await state.Gate.WaitAsync(cancellationToken).ConfigureAwait(false);
            try
            {
                if (!EphemeralProfiles.TryGetValue(profileName, out var current) ||
                    !ReferenceEquals(current, state))
                {
                    continue;
                }
                state.ReferenceCount++;
                return new EphemeralProfileLease(profileName, state);
            }
            finally
            {
                state.Gate.Release();
            }
        }
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

    private static async ValueTask ReleaseEphemeralProfileAsync(
        string profileName,
        EphemeralProfileState state)
    {
        await state.Gate.WaitAsync(CancellationToken.None).ConfigureAwait(false);
        try
        {
            if (state.ReferenceCount <= 0)
            {
                return;
            }
            state.ReferenceCount--;
            if (state.ReferenceCount > 0)
            {
                return;
            }

            var metadata = state.Metadata ?? await LoadProfileMetadataAsync(profileName)
                .ConfigureAwait(false);
            if (metadata is not null && await CleanupProfileAsync(metadata).ConfigureAwait(false))
            {
                DeleteProfileMetadata(profileName);
            }
            EphemeralProfiles.TryRemove(
                new KeyValuePair<string, EphemeralProfileState>(profileName, state));
        }
        finally
        {
            state.Gate.Release();
        }
    }

    private static async Task<bool> CleanupProfileAsync(EphemeralProfileMetadata metadata)
    {
        if (!IsValidControlledMetadata(metadata))
        {
            return false;
        }

        var deleteResult = DeleteAppContainerProfile(metadata.ProfileName);
        if (deleteResult != 0 &&
            deleteResult != unchecked((int)0x80070490) &&
            deleteResult != unchecked((int)0x80070002))
        {
            return false;
        }

        try
        {
            await RemovePathAclAsync(metadata.WorkspaceRoot, metadata.Sid, CancellationToken.None)
                .ConfigureAwait(false);
            PreparedWorkspaceAcls.TryRemove(
                WorkspaceAclKey(
                    metadata.WorkspaceRoot,
                    metadata.Sid,
                    ProfilePermission(metadata.ProfileName)),
                out _);
            await DeleteDirectoryWithRetriesAsync(metadata.TemporaryDirectory).ConfigureAwait(false);
            return true;
        }
        catch
        {
            return false;
        }
    }

    private static async Task RemovePathAclAsync(
        string root,
        string sid,
        CancellationToken cancellationToken)
    {
        if (!Directory.Exists(root))
        {
            return;
        }
        var systemDirectory = Environment.GetFolderPath(Environment.SpecialFolder.System);
        var icacls = Path.Combine(systemDirectory, "icacls.exe");
        if (!File.Exists(icacls))
        {
            throw new FileNotFoundException("Windows ACL utility was not found.", icacls);
        }
        var start = new ProcessStartInfo
        {
            FileName = icacls,
            UseShellExecute = false,
            CreateNoWindow = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
        };
        start.ArgumentList.Add(root);
        start.ArgumentList.Add("/remove:g");
        start.ArgumentList.Add($"*{sid}");
        start.ArgumentList.Add("/T");
        start.ArgumentList.Add("/C");
        start.ArgumentList.Add("/L");
        start.ArgumentList.Add("/Q");
        using var process = Process.Start(start)
            ?? throw new InvalidOperationException("Unable to start Windows ACL cleanup.");
        var stdout = process.StandardOutput.ReadToEndAsync(cancellationToken);
        var stderr = process.StandardError.ReadToEndAsync(cancellationToken);
        using var timeout = new CancellationTokenSource(TimeSpan.FromMinutes(2));
        using var linked = CancellationTokenSource.CreateLinkedTokenSource(
            cancellationToken,
            timeout.Token);
        try
        {
            await process.WaitForExitAsync(linked.Token).ConfigureAwait(false);
        }
        catch
        {
            if (!process.HasExited)
            {
                process.Kill(entireProcessTree: true);
            }
            throw;
        }
        var output = await stdout.ConfigureAwait(false);
        var error = await stderr.ConfigureAwait(false);
        if (process.ExitCode != 0)
        {
            throw new InvalidOperationException(
                $"Windows could not remove the workspace sandbox ACL (icacls {process.ExitCode}): {SafeAclError(error, output)}");
        }
    }

    private static async Task DeleteDirectoryWithRetriesAsync(string path)
    {
        for (var attempt = 0; attempt < 5; attempt++)
        {
            if (!Directory.Exists(path))
            {
                return;
            }
            try
            {
                Directory.Delete(path, recursive: true);
                return;
            }
            catch (IOException) when (attempt < 4)
            {
            }
            catch (UnauthorizedAccessException) when (attempt < 4)
            {
            }
            await Task.Delay(TimeSpan.FromMilliseconds(100 * (attempt + 1))).ConfigureAwait(false);
        }
        if (Directory.Exists(path))
        {
            Directory.Delete(path, recursive: true);
        }
    }

    private static Task CleanupStaleControlledProfilesOnceAsync()
    {
        if (Interlocked.Exchange(ref _staleCleanupStarted, 1) != 0)
        {
            return Task.CompletedTask;
        }
        return CleanupStaleControlledProfilesAsync();
    }

    private static async Task CleanupStaleControlledProfilesAsync()
    {
        await StaleCleanupGate.WaitAsync().ConfigureAwait(false);
        try
        {
            var registryRoot = ProfileRegistryRoot();
            if (!Directory.Exists(registryRoot))
            {
                return;
            }
            var threshold = DateTimeOffset.UtcNow.AddHours(-26);
            foreach (var file in Directory.EnumerateFiles(registryRoot, "*.json").Take(64))
            {
                try
                {
                    var metadata = JsonSerializer.Deserialize<EphemeralProfileMetadata>(
                        await File.ReadAllTextAsync(file).ConfigureAwait(false));
                    if (metadata is null || metadata.CreatedAt > threshold ||
                        EphemeralProfiles.ContainsKey(metadata.ProfileName))
                    {
                        continue;
                    }
                    if (await CleanupProfileAsync(metadata).ConfigureAwait(false))
                    {
                        File.Delete(file);
                    }
                }
                catch
                {
                }
            }
        }
        catch
        {
        }
        finally
        {
            StaleCleanupGate.Release();
        }
    }

    private static async Task SaveProfileMetadataAsync(
        EphemeralProfileMetadata metadata,
        CancellationToken cancellationToken)
    {
        var registryRoot = ProfileRegistryRoot();
        Directory.CreateDirectory(registryRoot);
        var target = ProfileMetadataPath(metadata.ProfileName);
        var pending = target + $".{Guid.NewGuid():N}.tmp";
        try
        {
            await File.WriteAllTextAsync(
                pending,
                JsonSerializer.Serialize(metadata),
                cancellationToken).ConfigureAwait(false);
            File.Move(pending, target, overwrite: true);
        }
        finally
        {
            if (File.Exists(pending))
            {
                File.Delete(pending);
            }
        }
    }

    private static async Task<EphemeralProfileMetadata?> LoadProfileMetadataAsync(string profileName)
    {
        var path = ProfileMetadataPath(profileName);
        if (!File.Exists(path))
        {
            return null;
        }
        try
        {
            return JsonSerializer.Deserialize<EphemeralProfileMetadata>(
                await File.ReadAllTextAsync(path).ConfigureAwait(false));
        }
        catch
        {
            return null;
        }
    }

    private static void DeleteProfileMetadata(string profileName)
    {
        try
        {
            var path = ProfileMetadataPath(profileName);
            if (File.Exists(path))
            {
                File.Delete(path);
            }
        }
        catch (IOException)
        {
        }
        catch (UnauthorizedAccessException)
        {
        }
    }

    private static string ProfileRegistryRoot() => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "ChatOS",
        "WindowsClient",
        "SandboxProfiles");

    private static string ProfileMetadataPath(string profileName) =>
        Path.Combine(ProfileRegistryRoot(), profileName + ".json");

    private static bool IsValidControlledMetadata(EphemeralProfileMetadata metadata)
    {
        if (!metadata.ProfileName.StartsWith("ChatOS.Sandbox.", StringComparison.Ordinal) ||
            !metadata.ProfileName.Contains(".controlled.v1.", StringComparison.Ordinal) ||
            string.IsNullOrWhiteSpace(metadata.Sid) ||
            !metadata.Sid.StartsWith("S-1-15-2-", StringComparison.Ordinal))
        {
            return false;
        }
        var expectedTempRoot = Path.GetFullPath(Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "ChatOS",
            "WindowsClient",
            "SandboxTemp"));
        var actualTemp = Path.GetFullPath(metadata.TemporaryDirectory);
        return actualTemp.StartsWith(expectedTempRoot + Path.DirectorySeparatorChar,
            StringComparison.OrdinalIgnoreCase);
    }

    private static ConnectorSandboxPermissionProfile ProfilePermission(string profileName) =>
        profileName.StartsWith("ChatOS.Sandbox.r.", StringComparison.Ordinal)
            ? ConnectorSandboxPermissionProfile.ReadOnly
            : ConnectorSandboxPermissionProfile.WorkspaceWrite;

    private static string WorkspaceAclKey(
        string workspaceRoot,
        string sid,
        ConnectorSandboxPermissionProfile profile) =>
        string.Join('\0', Path.GetFullPath(workspaceRoot), sid, profile);

    private static string SafeAclError(string error, string output)
    {
        var value = string.IsNullOrWhiteSpace(error) ? output : error;
        value = new string(value.Where(value => !char.IsControl(value) || value == ' ').Take(500).ToArray());
        return string.IsNullOrWhiteSpace(value) ? "ACL update failed" : value;
    }

    private static IntPtr CapabilitySid(string value)
    {
        if (!ConvertStringSidToSid(value, out var sid))
        {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }
        return sid;
    }

    private static string SidToString(IntPtr sid)
    {
        if (!ConvertSidToStringSid(sid, out var value))
        {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }
        try
        {
            return Marshal.PtrToStringUni(value)
                ?? throw new InvalidOperationException("Windows returned an empty AppContainer SID.");
        }
        finally
        {
            _ = LocalFree(value);
        }
    }

    [DllImport("userenv.dll", CharSet = CharSet.Unicode)]
    private static extern int CreateAppContainerProfile(
        string appContainerName,
        string displayName,
        string description,
        IntPtr capabilities,
        uint capabilityCount,
        out IntPtr appContainerSid);

    [DllImport("userenv.dll", CharSet = CharSet.Unicode)]
    private static extern int DeriveAppContainerSidFromAppContainerName(
        string appContainerName,
        out IntPtr appContainerSid);

    [DllImport("userenv.dll", CharSet = CharSet.Unicode)]
    private static extern int DeleteAppContainerProfile(string appContainerName);

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool ConvertStringSidToSid(string stringSid, out IntPtr sid);

    [DllImport("advapi32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool ConvertSidToStringSid(IntPtr sid, out IntPtr stringSid);

    [DllImport("kernel32.dll")]
    private static extern IntPtr LocalFree(IntPtr memory);

    [DllImport("advapi32.dll")]
    internal static extern IntPtr FreeSid(IntPtr sid);

    private sealed class EphemeralProfileState
    {
        public SemaphoreSlim Gate { get; } = new(1, 1);

        public int ReferenceCount { get; set; }

        public EphemeralProfileMetadata? Metadata { get; set; }
    }

    private sealed class EphemeralProfileLease(
        string profileName,
        EphemeralProfileState state) : IAsyncDisposable
    {
        private int _disposed;

        public Task RegisterAsync(
            string workspaceRoot,
            string sid,
            string temporaryDirectory,
            CancellationToken cancellationToken) =>
            RegisterEphemeralProfileAsync(
                profileName,
                state,
                workspaceRoot,
                sid,
                temporaryDirectory,
                cancellationToken);

        public async ValueTask DisposeAsync()
        {
            if (Interlocked.Exchange(ref _disposed, 1) == 0)
            {
                await ReleaseEphemeralProfileAsync(profileName, state).ConfigureAwait(false);
            }
        }
    }

    private sealed record EphemeralProfileMetadata(
        string ProfileName,
        string WorkspaceRoot,
        string Sid,
        string TemporaryDirectory,
        DateTimeOffset CreatedAt);
}
