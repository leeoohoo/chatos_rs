using System.Runtime.InteropServices;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using WinRT.Interop;

namespace ChatOS.Desktop.Features.QuickSearch;

public sealed class WindowsGlobalHotKeyService : IDisposable
{
    private const int HotKeyId = 0x4348;
    private const uint WmHotKey = 0x0312;
    private const uint ModAlt = 0x0001;
    private const uint ModControl = 0x0002;
    private const uint ModNoRepeat = 0x4000;
    private const uint VirtualKeySpace = 0x20;
    private SubclassProcedure? _procedure;
    private IntPtr _windowHandle;
    private DispatcherQueue? _dispatcher;

    public event EventHandler? Pressed;

    public string RegisteredShortcut { get; private set; } = string.Empty;

    public void Register(Window owner)
    {
        if (_windowHandle != IntPtr.Zero) return;
        _windowHandle = WindowNative.GetWindowHandle(owner);
        _dispatcher = owner.DispatcherQueue;
        _procedure = WindowProcedure;
        if (!SetWindowSubclass(_windowHandle, _procedure, (UIntPtr)HotKeyId, UIntPtr.Zero))
            throw new InvalidOperationException("Unable to observe the global search shortcut.");

        if (RegisterHotKey(_windowHandle, HotKeyId, ModControl | ModNoRepeat, VirtualKeySpace))
        {
            RegisteredShortcut = "Ctrl+Space";
            return;
        }
        if (RegisterHotKey(_windowHandle, HotKeyId, ModControl | ModAlt | ModNoRepeat, VirtualKeySpace))
        {
            RegisteredShortcut = "Ctrl+Alt+Space";
            return;
        }

        RemoveWindowSubclass(_windowHandle, _procedure, (UIntPtr)HotKeyId);
        _windowHandle = IntPtr.Zero;
        _procedure = null;
        throw new InvalidOperationException("Ctrl+Space and Ctrl+Alt+Space are already in use.");
    }

    private IntPtr WindowProcedure(
        IntPtr window,
        uint message,
        UIntPtr wParam,
        IntPtr lParam,
        UIntPtr subclassId,
        UIntPtr referenceData)
    {
        if (message == WmHotKey && wParam.ToUInt64() == HotKeyId)
            _ = _dispatcher?.TryEnqueue(() => Pressed?.Invoke(this, EventArgs.Empty));
        return DefSubclassProc(window, message, wParam, lParam);
    }

    public void Dispose()
    {
        if (_windowHandle == IntPtr.Zero || _procedure is null) return;
        UnregisterHotKey(_windowHandle, HotKeyId);
        RemoveWindowSubclass(_windowHandle, _procedure, (UIntPtr)HotKeyId);
        _windowHandle = IntPtr.Zero;
        _procedure = null;
    }

    private delegate IntPtr SubclassProcedure(
        IntPtr window,
        uint message,
        UIntPtr wParam,
        IntPtr lParam,
        UIntPtr subclassId,
        UIntPtr referenceData);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool RegisterHotKey(IntPtr window, int id, uint modifiers, uint virtualKey);

    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool UnregisterHotKey(IntPtr window, int id);

    [DllImport("comctl32.dll", SetLastError = true)]
    private static extern bool SetWindowSubclass(
        IntPtr window,
        SubclassProcedure procedure,
        UIntPtr subclassId,
        UIntPtr referenceData);

    [DllImport("comctl32.dll", SetLastError = true)]
    private static extern bool RemoveWindowSubclass(
        IntPtr window,
        SubclassProcedure procedure,
        UIntPtr subclassId);

    [DllImport("comctl32.dll")]
    private static extern IntPtr DefSubclassProc(IntPtr window, uint message, UIntPtr wParam, IntPtr lParam);
}
