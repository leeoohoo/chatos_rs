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
