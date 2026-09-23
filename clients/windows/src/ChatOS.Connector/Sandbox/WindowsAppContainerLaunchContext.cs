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
    private readonly IntPtr _appContainerSid;
    private readonly List<IntPtr> _capabilitySids;
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
