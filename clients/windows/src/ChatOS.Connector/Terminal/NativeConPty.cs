using System.ComponentModel;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace ChatOS.Connector.Terminal;

internal static class NativeConPty
{
    internal const uint ExtendedStartupInfoPresent = 0x0008_0000;
    internal const uint CreateUnicodeEnvironment = 0x0000_0400;
    internal const uint CreateSuspended = 0x0000_0004;
    internal const nuint ProcThreadAttributePseudoConsole = 0x0002_0016;
    internal const nuint ProcThreadAttributeHandleList = 0x0002_0002;
    private const uint HandleFlagInherit = 0x0000_0001;
    private const uint JobObjectExtendedLimitInformation = 9;
    private const uint JobObjectLimitKillOnJobClose = 0x0000_2000;
    private const uint Infinite = 0xffff_ffff;

    public static void CreatePipePair(
        out SafeFileHandle parentEnd,
        out SafeFileHandle pseudoConsoleEnd,
        bool parentReads)
    {
        var security = new SecurityAttributes
        {
            Length = Marshal.SizeOf<SecurityAttributes>(),
            InheritHandle = true,
        };
        ThrowIfFalse(CreatePipe(out var read, out var write, ref security, 0));
        if (parentReads)
        {
            parentEnd = read;
            pseudoConsoleEnd = write;
        }
        else
        {
            parentEnd = read;
            pseudoConsoleEnd = write;
            (parentEnd, pseudoConsoleEnd) = (pseudoConsoleEnd, parentEnd);
        }

        ThrowIfFalse(SetHandleInformation(parentEnd, HandleFlagInherit, 0));
    }

    public static SafePseudoConsoleHandle CreatePseudoConsole(
        TerminalSize size,
        SafeFileHandle input,
        SafeFileHandle output)
    {
        var result = CreatePseudoConsole(
            new Coord((short)size.Columns, (short)size.Rows),
            input,
            output,
            0,
            out var handle);
        if (result != 0)
        {
            Marshal.ThrowExceptionForHR(result);
        }

        return new SafePseudoConsoleHandle(handle, ownsHandle: true);
    }

    public static void Resize(SafePseudoConsoleHandle pseudoConsole, TerminalSize size)
    {
        var result = ResizePseudoConsole(
            pseudoConsole,
            new Coord((short)size.Columns, (short)size.Rows));
        if (result != 0)
        {
            Marshal.ThrowExceptionForHR(result);
        }
    }

    public static SafeKernelObjectHandle CreateKillOnCloseJob()
    {
        var job = CreateJobObject(IntPtr.Zero, null);
        if (job.IsInvalid)
        {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }

        var limits = new JobObjectExtendedLimitInformation
        {
            BasicLimitInformation = new JobObjectBasicLimitInformation
            {
                LimitFlags = JobObjectLimitKillOnJobClose,
            },
        };
        var pointer = Marshal.AllocHGlobal(Marshal.SizeOf<JobObjectExtendedLimitInformation>());
        try
        {
            Marshal.StructureToPtr(limits, pointer, fDeleteOld: false);
            ThrowIfFalse(SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                pointer,
                (uint)Marshal.SizeOf<JobObjectExtendedLimitInformation>()));
            return job;
        }
        catch
        {
            job.Dispose();
            throw;
        }
        finally
        {
            Marshal.FreeHGlobal(pointer);
        }
    }

    public static void TerminateJob(SafeKernelObjectHandle job, uint exitCode)
    {
        if (!job.IsClosed && !job.IsInvalid)
        {
            _ = TerminateJobObject(job, exitCode);
        }
    }

    public static int WaitForExit(SafeKernelObjectHandle process)
    {
        var wait = WaitForSingleObject(process, Infinite);
        if (wait != 0)
        {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }

        ThrowIfFalse(GetExitCodeProcess(process, out var code));
        return unchecked((int)code);
    }

    public static void ThrowIfFalse(bool result)
    {
        if (!result)
        {
            throw new Win32Exception(Marshal.GetLastWin32Error());
        }
    }

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool CreatePipe(
        out SafeFileHandle readPipe,
        out SafeFileHandle writePipe,
        ref SecurityAttributes pipeAttributes,
        uint size);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool SetHandleInformation(
        SafeFileHandle handle,
        uint mask,
        uint flags);

    [DllImport("kernel32.dll")]
    private static extern int CreatePseudoConsole(
        Coord size,
        SafeFileHandle input,
        SafeFileHandle output,
        uint flags,
        out IntPtr pseudoConsole);

    [DllImport("kernel32.dll")]
    private static extern int ResizePseudoConsole(
        SafePseudoConsoleHandle pseudoConsole,
        Coord size);

    [DllImport("kernel32.dll")]
    internal static extern void ClosePseudoConsole(IntPtr pseudoConsole);

    [DllImport("kernel32.dll", SetLastError = true)]
    internal static extern bool InitializeProcThreadAttributeList(
        IntPtr attributeList,
        int attributeCount,
        int flags,
        ref nuint size);

    [DllImport("kernel32.dll", SetLastError = true)]
    internal static extern bool UpdateProcThreadAttribute(
        IntPtr attributeList,
        uint flags,
        nuint attribute,
        IntPtr value,
        nuint size,
        IntPtr previousValue,
        IntPtr returnSize);

    [DllImport("kernel32.dll")]
    internal static extern void DeleteProcThreadAttributeList(IntPtr attributeList);

    [DllImport("kernel32.dll", EntryPoint = "CreateProcessW", CharSet = CharSet.Unicode, SetLastError = true)]
    internal static extern bool CreateProcess(
        string? applicationName,
        System.Text.StringBuilder commandLine,
        IntPtr processAttributes,
        IntPtr threadAttributes,
        bool inheritHandles,
        uint creationFlags,
        IntPtr environment,
        string currentDirectory,
        ref StartupInfoEx startupInfo,
        out ProcessInformation processInformation);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern SafeKernelObjectHandle CreateJobObject(
        IntPtr jobAttributes,
        string? name);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool SetInformationJobObject(
        SafeKernelObjectHandle job,
        uint informationClass,
        IntPtr information,
        uint informationLength);

    [DllImport("kernel32.dll", SetLastError = true)]
    internal static extern bool AssignProcessToJobObject(
        SafeKernelObjectHandle job,
        SafeKernelObjectHandle process);

    [DllImport("kernel32.dll", EntryPoint = "AssignProcessToJobObject", SetLastError = true)]
    internal static extern bool AssignProcessToJobObject(
        SafeKernelObjectHandle job,
        SafeProcessHandle process);

    [DllImport("kernel32.dll", SetLastError = true)]
    internal static extern uint ResumeThread(SafeKernelObjectHandle thread);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool TerminateJobObject(SafeKernelObjectHandle job, uint exitCode);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern uint WaitForSingleObject(SafeKernelObjectHandle handle, uint milliseconds);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool GetExitCodeProcess(SafeKernelObjectHandle process, out uint exitCode);

    [DllImport("kernel32.dll")]
    internal static extern bool CloseHandle(IntPtr handle);
}

internal sealed class SafePseudoConsoleHandle : SafeHandleZeroOrMinusOneIsInvalid
{
    public SafePseudoConsoleHandle()
        : base(ownsHandle: true)
    {
    }

    public SafePseudoConsoleHandle(IntPtr handle, bool ownsHandle)
        : base(ownsHandle)
    {
        SetHandle(handle);
    }

    protected override bool ReleaseHandle()
    {
        NativeConPty.ClosePseudoConsole(handle);
        return true;
    }
}

internal sealed class SafeKernelObjectHandle : SafeHandleZeroOrMinusOneIsInvalid
{
    public SafeKernelObjectHandle()
        : base(ownsHandle: true)
    {
    }

    public SafeKernelObjectHandle(IntPtr handle, bool ownsHandle)
        : base(ownsHandle)
    {
        SetHandle(handle);
    }

    protected override bool ReleaseHandle() => NativeConPty.CloseHandle(handle);
}

[StructLayout(LayoutKind.Sequential)]
internal struct Coord(short x, short y)
{
    public short X = x;
    public short Y = y;
}

[StructLayout(LayoutKind.Sequential)]
internal struct SecurityAttributes
{
    public int Length;
    public IntPtr SecurityDescriptor;
    [MarshalAs(UnmanagedType.Bool)]
    public bool InheritHandle;
}

[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
internal struct StartupInfo
{
    public uint Size;
    public string? Reserved;
    public string? Desktop;
    public string? Title;
    public uint X;
    public uint Y;
    public uint XSize;
    public uint YSize;
    public uint XCountChars;
    public uint YCountChars;
    public uint FillAttribute;
    public uint Flags;
    public ushort ShowWindow;
    public ushort Reserved2;
    public IntPtr Reserved2Pointer;
    public IntPtr StandardInput;
    public IntPtr StandardOutput;
    public IntPtr StandardError;
}

[StructLayout(LayoutKind.Sequential)]
internal struct StartupInfoEx
{
    public StartupInfo StartupInfo;
    public IntPtr AttributeList;
}

[StructLayout(LayoutKind.Sequential)]
internal struct ProcessInformation
{
    public IntPtr Process;
    public IntPtr Thread;
    public uint ProcessId;
    public uint ThreadId;
}

[StructLayout(LayoutKind.Sequential)]
internal struct JobObjectBasicLimitInformation
{
    public long PerProcessUserTimeLimit;
    public long PerJobUserTimeLimit;
    public uint LimitFlags;
    public nuint MinimumWorkingSetSize;
    public nuint MaximumWorkingSetSize;
    public uint ActiveProcessLimit;
    public nuint Affinity;
    public uint PriorityClass;
    public uint SchedulingClass;
}

[StructLayout(LayoutKind.Sequential)]
internal struct IoCounters
{
    public ulong ReadOperationCount;
    public ulong WriteOperationCount;
    public ulong OtherOperationCount;
    public ulong ReadTransferCount;
    public ulong WriteTransferCount;
    public ulong OtherTransferCount;
}

[StructLayout(LayoutKind.Sequential)]
internal struct JobObjectExtendedLimitInformation
{
    public JobObjectBasicLimitInformation BasicLimitInformation;
    public IoCounters IoInfo;
    public nuint ProcessMemoryLimit;
    public nuint JobMemoryLimit;
    public nuint PeakProcessMemoryUsed;
    public nuint PeakJobMemoryUsed;
}
