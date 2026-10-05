using System.Collections.Concurrent;
using System.ComponentModel;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;

namespace ChatOS.Connector.Sandbox;

internal interface IWindowsAppContainerProfileLease : IAsyncDisposable
{
    Task RegisterAdditionalPathAsync(
        string path,
        string sid,
        CancellationToken cancellationToken);
}

internal sealed class WindowsAppContainerLaunchContext : IDisposable, IAsyncDisposable
{
    private readonly IntPtr _appContainerSid;
    private readonly List<IntPtr> _capabilitySids;
    private readonly IntPtr _capabilityArray;
    private IWindowsAppContainerProfileLease? _profileLease;
    private int _disposed;

    public WindowsAppContainerLaunchContext(
        IntPtr appContainerSid,
        string appContainerSidText,
        IReadOnlyList<IntPtr> capabilitySids,
        string temporaryDirectory,
        SandboxExecutionPolicy policy,
        IReadOnlyDictionary<string, string>? environment = null,
        bool minimalEnvironment = false,
        IWindowsAppContainerProfileLease? profileLease = null)
    {
        _appContainerSid = appContainerSid;
        _capabilitySids = [.. capabilitySids];
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
        EnvironmentBlock = BuildEnvironmentBlock(
            temporaryDirectory,
            policy,
            environment,
            minimalEnvironment);
        AppContainerSid = appContainerSidText;
        _profileLease = profileLease;
    }

    public string AppContainerSid { get; }

    public IntPtr SecurityCapabilities { get; }

    public IntPtr EnvironmentBlock { get; }

    internal IAsyncDisposable? DetachProfileLease() =>
        Interlocked.Exchange(ref _profileLease, null);

    internal async Task GrantPathAccessAsync(
        string path,
        ConnectorSandboxPermissionProfile profile,
        CancellationToken cancellationToken)
    {
        if (_profileLease is not null)
        {
            await _profileLease.RegisterAdditionalPathAsync(
                path,
                AppContainerSid,
                cancellationToken).ConfigureAwait(false);
        }
        await WindowsAppContainerSandbox.GrantPathAccessAsync(
            path,
            AppContainerSid,
            profile,
            cancellationToken).ConfigureAwait(false);
    }

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
        if (_appContainerSid != IntPtr.Zero)
        {
            _ = WindowsAppContainerSandbox.FreeSid(_appContainerSid);
        }
        foreach (var sid in _capabilitySids)
        {
            if (sid != IntPtr.Zero) _ = WindowsAppContainerSandbox.FreeLocalMemory(sid);
        }
        var profileLease = Interlocked.Exchange(ref _profileLease, null);
        if (profileLease is not null)
        {
            await profileLease.DisposeAsync().ConfigureAwait(false);
        }
    }

    private static IntPtr BuildEnvironmentBlock(
        string temporaryDirectory,
        SandboxExecutionPolicy policy,
        IReadOnlyDictionary<string, string>? additions,
        bool minimalEnvironment)
    {
        var variables = new SortedDictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        var baseline = minimalEnvironment
            ? BuildPluginEnvironmentVariables(temporaryDirectory, policy)
            : BuildEnvironmentVariables(temporaryDirectory, policy);
        foreach (var pair in baseline)
        {
            variables[pair.Key] = pair.Value;
        }
        if (additions is not null)
        {
            foreach (var pair in additions)
            {
                if (pair.Key.Equals("PATH", StringComparison.OrdinalIgnoreCase) ||
                    pair.Key.Equals("SystemRoot", StringComparison.OrdinalIgnoreCase) ||
                    pair.Key.Equals("WINDIR", StringComparison.OrdinalIgnoreCase) ||
                    pair.Key.Equals("TEMP", StringComparison.OrdinalIgnoreCase) ||
                    pair.Key.Equals("TMP", StringComparison.OrdinalIgnoreCase))
                {
                    continue;
                }
                variables[pair.Key] = pair.Value;
            }
        }
        if (additions?.TryGetValue("CHATOS_PLUGIN_DATA_DIR", out var dataDirectory) == true)
        {
            variables["HOME"] = dataDirectory;
            variables["USERPROFILE"] = dataDirectory;
        }
        var block = string.Join('\0', variables.Select(pair => $"{pair.Key}={pair.Value}")) + "\0\0";
        return Marshal.StringToHGlobalUni(block);
    }

    private static IReadOnlyDictionary<string, string> BuildPluginEnvironmentVariables(
        string temporaryDirectory,
        SandboxExecutionPolicy policy)
    {
        var systemRoot = Environment.GetFolderPath(Environment.SpecialFolder.Windows);
        var systemDirectory = Environment.SystemDirectory;
        return new SortedDictionary<string, string>(StringComparer.OrdinalIgnoreCase)
        {
            ["CHATOS_SANDBOX"] = "1",
            ["CHATOS_SANDBOX_NETWORK"] = policy.NetworkAccess.ToString(),
            ["CHATOS_SANDBOX_PROFILE"] = policy.PermissionProfile.ToString(),
            ["ComSpec"] = Path.Combine(systemDirectory, "cmd.exe"),
            ["LOCALAPPDATA"] = temporaryDirectory,
            ["PATH"] = string.Join(Path.PathSeparator, new[]
            {
                systemDirectory,
                systemRoot,
                Path.Combine(systemDirectory, "Wbem"),
            }.Where(path => !string.IsNullOrWhiteSpace(path))),
            ["PATHEXT"] = ".COM;.EXE;.BAT;.CMD",
            ["SystemDrive"] = Path.GetPathRoot(systemRoot)?.TrimEnd(Path.DirectorySeparatorChar) ?? "C:\\",
            ["SystemRoot"] = systemRoot,
            ["TEMP"] = temporaryDirectory,
            ["TMP"] = temporaryDirectory,
            ["WINDIR"] = systemRoot,
        };
    }

    internal static IReadOnlyDictionary<string, string> BuildEnvironmentVariables(
        string temporaryDirectory,
        SandboxExecutionPolicy policy)
    {
        var systemRoot = Environment.GetEnvironmentVariable("SystemRoot");
        if (string.IsNullOrWhiteSpace(systemRoot))
        {
            systemRoot = Environment.GetFolderPath(Environment.SpecialFolder.Windows);
        }
        var commandInterpreter = Environment.GetEnvironmentVariable("ComSpec");
        if (string.IsNullOrWhiteSpace(commandInterpreter))
        {
            commandInterpreter = Path.Combine(systemRoot, "System32", "cmd.exe");
        }
        var path = string.Join(
            Path.PathSeparator,
            Environment.ExpandEnvironmentVariables(
                    Environment.GetEnvironmentVariable("PATH") ?? string.Empty)
                .Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries)
                .Select(value => value.Trim('"'))
                .Where(value => value.Length > 0 && !value.Contains('%') && Path.IsPathRooted(value)));
        var variables = new SortedDictionary<string, string>(StringComparer.OrdinalIgnoreCase)
        {
            ["CHATOS_SANDBOX"] = "1",
            ["CHATOS_SANDBOX_NETWORK"] = policy.NetworkAccess.ToString(),
            ["CHATOS_SANDBOX_PROFILE"] = policy.PermissionProfile.ToString(),
            ["ComSpec"] = Environment.ExpandEnvironmentVariables(commandInterpreter),
            ["LOCALAPPDATA"] = temporaryDirectory,
            ["PATH"] = path,
            ["PATHEXT"] = Environment.GetEnvironmentVariable("PATHEXT") ?? ".COM;.EXE;.BAT;.CMD",
            ["PROCESSOR_ARCHITECTURE"] = Environment.GetEnvironmentVariable("PROCESSOR_ARCHITECTURE") ?? string.Empty,
            ["SystemDrive"] = Path.GetPathRoot(systemRoot)?.TrimEnd(Path.DirectorySeparatorChar) ?? "C:",
            ["SystemRoot"] = systemRoot,
            ["TEMP"] = temporaryDirectory,
            ["TMP"] = temporaryDirectory,
            ["WINDIR"] = systemRoot,
        };
        return variables;
    }
}

[StructLayout(LayoutKind.Sequential)]
internal readonly struct SecurityCapabilities
{
    public SecurityCapabilities(
        IntPtr appContainerSid,
        IntPtr capabilities,
        uint capabilityCount,
        uint reserved)
    {
        AppContainerSid = appContainerSid;
        Capabilities = capabilities;
        CapabilityCount = capabilityCount;
        Reserved = reserved;
    }

    public readonly IntPtr AppContainerSid;
    public readonly IntPtr Capabilities;
    public readonly uint CapabilityCount;
    public readonly uint Reserved;
}

[StructLayout(LayoutKind.Sequential)]
internal readonly struct SidAndAttributes
{
    public SidAndAttributes(IntPtr sid, uint attributes)
    {
        Sid = sid;
        Attributes = attributes;
    }

    public readonly IntPtr Sid;
    public readonly uint Attributes;
}
