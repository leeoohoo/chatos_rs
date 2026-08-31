using System.ComponentModel;
using System.Runtime.InteropServices;

namespace VisualComputerUse.Windows;

internal static class InputService
{
    private const ushort VkShift = 0x10;
    private const ushort VkControl = 0x11;
    private const ushort VkMenu = 0x12;
    private const ushort VkLwin = 0x5B;
    private const ushort VkReturn = 0x0D;

    internal static void Click(PointDto point, string button, int count, double intervalSeconds)
    {
        var (down, up) = button.ToLowerInvariant() switch
        {
            "left" => (NativeMethods.MouseeventfLeftdown, NativeMethods.MouseeventfLeftup),
            "right" => (NativeMethods.MouseeventfRightdown, NativeMethods.MouseeventfRightup),
            "middle" => (NativeMethods.MouseeventfMiddledown, NativeMethods.MouseeventfMiddleup),
            _ => throw new VisualComputerUseException("button must be left, right, or middle.")
        };

        count = Math.Clamp(count, 1, 3);
        intervalSeconds = Math.Clamp(intervalSeconds, 0, 1);
        WithPhysicalCursorAt(point, () =>
        {
            for (var index = 0; index < count; index++)
            {
                Send(Mouse(down), Mouse(up));
                if (index + 1 < count && intervalSeconds > 0)
                    Thread.Sleep(TimeSpan.FromSeconds(intervalSeconds));
            }
        });
    }

    internal static async Task ScrollAsync(
        PointDto point,
        int deltaX,
        int deltaY,
        double durationSeconds,
        int steps)
    {
        steps = Math.Clamp(steps, 2, 80);
        durationSeconds = Math.Clamp(durationSeconds, 0, 3);
        var xDeltas = SmoothDeltas(deltaX, steps);
        var yDeltas = SmoothDeltas(deltaY, steps);

        if (!NativeMethods.GetCursorPos(out var saved))
            throw new VisualComputerUseException("Could not read the physical Windows cursor position.");
        if (!NativeMethods.SetCursorPos((int)Math.Round(point.X), (int)Math.Round(point.Y)))
            throw new VisualComputerUseException("Could not position the physical cursor for scrolling.");

        try
        {
            for (var index = 0; index < steps; index++)
            {
                var inputs = new List<NativeMethods.Input>(2);
                if (yDeltas[index] != 0)
                    inputs.Add(Mouse(NativeMethods.MouseeventfWheel, unchecked((uint)(yDeltas[index] * NativeMethods.WheelDelta))));
                if (xDeltas[index] != 0)
                    inputs.Add(Mouse(NativeMethods.MouseeventfHwheel, unchecked((uint)(xDeltas[index] * NativeMethods.WheelDelta))));
                if (inputs.Count > 0)
                    Send(inputs.ToArray());
                if (durationSeconds > 0 && index + 1 < steps)
                    await Task.Delay(TimeSpan.FromSeconds(durationSeconds / (steps - 1))).ConfigureAwait(false);
            }
        }
        finally
        {
            NativeMethods.SetCursorPos(saved.X, saved.Y);
        }
    }

    internal static void TypeText(string text)
    {
        var inputs = new List<NativeMethods.Input>(Math.Max(2, text.Length * 2));
        foreach (var character in text)
        {
            if (character == '\r')
                continue;
            if (character == '\n')
            {
                inputs.Add(Key(VkReturn, false));
                inputs.Add(Key(VkReturn, true));
                continue;
            }
            inputs.Add(UnicodeKey(character, false));
            inputs.Add(UnicodeKey(character, true));
        }
        SendInChunks(inputs, 256);
    }

    internal static void PressKeys(IReadOnlyList<string> rawKeys)
    {
        if (rawKeys.Count == 0)
            throw new VisualComputerUseException("keys must contain at least one key.");

        var modifiers = new List<ushort>();
        ushort? primary = null;
        foreach (var raw in rawKeys)
        {
            var key = raw.Trim().ToLowerInvariant();
            var modifier = key switch
            {
                "shift" => VkShift,
                "control" or "ctrl" => VkControl,
                "alt" or "option" => VkMenu,
                "windows" or "win" or "command" or "cmd" => VkLwin,
                _ => (ushort)0
            };
            if (modifier != 0)
            {
                if (!modifiers.Contains(modifier))
                    modifiers.Add(modifier);
                continue;
            }
            if (primary is not null)
                throw new VisualComputerUseException("key_press accepts one non-modifier key plus optional modifiers.");
            primary = ParsePrimaryKey(key);
        }

        if (primary is null)
            throw new VisualComputerUseException("key_press requires one non-modifier key.");
        var inputs = new List<NativeMethods.Input>();
        inputs.AddRange(modifiers.Select(value => Key(value, false)));
        inputs.Add(Key(primary.Value, false));
        inputs.Add(Key(primary.Value, true));
        inputs.AddRange(modifiers.AsEnumerable().Reverse().Select(value => Key(value, true)));
        Send(inputs.ToArray());
    }

    internal static int[] SmoothDeltas(int total, int steps)
    {
        steps = Math.Clamp(steps, 2, 80);
        var result = new int[steps];
        long previous = 0;
        for (var index = 1; index <= steps; index++)
        {
            var progress = (double)index / steps;
            var eased = progress * progress * (3 - 2 * progress);
            var cumulative = (long)Math.Round(total * eased);
            result[index - 1] = checked((int)(cumulative - previous));
            previous = cumulative;
        }
        return result;
    }

    private static void WithPhysicalCursorAt(PointDto point, Action action)
    {
        if (!NativeMethods.GetCursorPos(out var saved))
            throw new VisualComputerUseException("Could not read the physical Windows cursor position.");
        if (!NativeMethods.SetCursorPos((int)Math.Round(point.X), (int)Math.Round(point.Y)))
            throw new VisualComputerUseException("Could not position the physical cursor for input.");
        try
        {
            action();
        }
        finally
        {
            NativeMethods.SetCursorPos(saved.X, saved.Y);
        }
    }

    private static ushort ParsePrimaryKey(string key)
    {
        var named = key switch
        {
            "enter" or "return" => 0x0D,
            "escape" or "esc" => 0x1B,
            "space" => 0x20,
            "tab" => 0x09,
            "backspace" or "delete_backward" => 0x08,
            "delete" or "delete_forward" => 0x2E,
            "insert" => 0x2D,
            "home" => 0x24,
            "end" => 0x23,
            "pageup" or "page_up" => 0x21,
            "pagedown" or "page_down" => 0x22,
            "left" or "arrowleft" => 0x25,
            "up" or "arrowup" => 0x26,
            "right" or "arrowright" => 0x27,
            "down" or "arrowdown" => 0x28,
            "f1" => 0x70, "f2" => 0x71, "f3" => 0x72, "f4" => 0x73,
            "f5" => 0x74, "f6" => 0x75, "f7" => 0x76, "f8" => 0x77,
            "f9" => 0x78, "f10" => 0x79, "f11" => 0x7A, "f12" => 0x7B,
            _ => 0
        };
        if (named != 0)
            return (ushort)named;
        if (key.Length != 1)
            throw new VisualComputerUseException($"Unsupported key '{key}'.");
        var mapped = NativeMethods.VkKeyScan(key[0]);
        if (mapped == -1)
            throw new VisualComputerUseException($"Unsupported key '{key}'.");
        return (ushort)(mapped & 0xFF);
    }

    private static NativeMethods.Input Mouse(uint flags, uint data = 0) => new()
    {
        Type = NativeMethods.InputMouse,
        Union = new NativeMethods.InputUnion
        {
            Mouse = new NativeMethods.MouseInput { Flags = flags, MouseData = data }
        }
    };

    private static NativeMethods.Input Key(ushort virtualKey, bool up) => new()
    {
        Type = NativeMethods.InputKeyboard,
        Union = new NativeMethods.InputUnion
        {
            Keyboard = new NativeMethods.KeyboardInput
            {
                VirtualKey = virtualKey,
                Flags = up ? NativeMethods.KeyeventfKeyup : 0
            }
        }
    };

    private static NativeMethods.Input UnicodeKey(char character, bool up) => new()
    {
        Type = NativeMethods.InputKeyboard,
        Union = new NativeMethods.InputUnion
        {
            Keyboard = new NativeMethods.KeyboardInput
            {
                ScanCode = character,
                Flags = NativeMethods.KeyeventfUnicode | (up ? NativeMethods.KeyeventfKeyup : 0)
            }
        }
    };

    private static void SendInChunks(IReadOnlyList<NativeMethods.Input> inputs, int chunkSize)
    {
        for (var offset = 0; offset < inputs.Count; offset += chunkSize)
            Send(inputs.Skip(offset).Take(Math.Min(chunkSize, inputs.Count - offset)).ToArray());
    }

    private static void Send(params NativeMethods.Input[] inputs)
    {
        if (inputs.Length == 0)
            return;
        var sent = NativeMethods.SendInput((uint)inputs.Length, inputs, Marshal.SizeOf<NativeMethods.Input>());
        if (sent != (uint)inputs.Length)
            throw new VisualComputerUseException($"Windows SendInput accepted {sent} of {inputs.Length} events: {new Win32Exception(Marshal.GetLastWin32Error()).Message}");
    }
}
