using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;

namespace ChatOS.Desktop.Features.Chat;

internal static class WindowsFileOpenDialog
{
    private const int BufferCharacters = 65_536;
    private const uint Explorer = 0x00080000;
    private const uint AllowMultiSelect = 0x00000200;
    private const uint FileMustExist = 0x00001000;
    private const uint PathMustExist = 0x00000800;
    private const uint NoChangeDirectory = 0x00000008;

    public static IReadOnlyList<string> PickFiles(IntPtr ownerWindow, string title)
    {
        var buffer = Marshal.AllocHGlobal(BufferCharacters * sizeof(char));
        try
        {
            for (var index = 0; index < BufferCharacters; index++)
            {
                Marshal.WriteInt16(buffer, index * sizeof(char), 0);
            }

            var dialog = new OpenFileName
            {
                StructSize = Marshal.SizeOf<OpenFileName>(),
                OwnerWindow = ownerWindow,
                Filter = "All files\0*.*\0\0",
                FileBuffer = buffer,
                MaximumFileCharacters = BufferCharacters,
                Title = title,
                Flags = Explorer | AllowMultiSelect | FileMustExist | PathMustExist | NoChangeDirectory,
            };

            if (!GetOpenFileName(ref dialog))
            {
                var error = CommDlgExtendedError();
                if (error != 0)
                {
                    throw new Win32Exception(
                        unchecked((int)error),
                        $"The Windows file picker failed with common-dialog error 0x{error:X8}.");
                }

                return Array.Empty<string>();
            }

            var parts = ReadNullSeparatedStrings(buffer);
            if (parts.Count <= 1)
            {
                return parts;
            }

            var directory = parts[0];
            return parts.Skip(1).Select(fileName => Path.Combine(directory, fileName)).ToArray();
        }
        finally
        {
            Marshal.FreeHGlobal(buffer);
        }
    }

    private static IReadOnlyList<string> ReadNullSeparatedStrings(IntPtr buffer)
    {
        var results = new List<string>();
        var current = new StringBuilder();
        for (var index = 0; index < BufferCharacters; index++)
        {
            var value = (char)Marshal.ReadInt16(buffer, index * sizeof(char));
            if (value != '\0')
            {
                current.Append(value);
                continue;
            }

            if (current.Length == 0)
            {
                break;
            }

            results.Add(current.ToString());
            current.Clear();
        }

        return results;
    }

    [DllImport("comdlg32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetOpenFileName(ref OpenFileName openFileName);

    [DllImport("comdlg32.dll")]
    private static extern uint CommDlgExtendedError();

    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct OpenFileName
    {
        public int StructSize;
        public IntPtr OwnerWindow;
        public IntPtr Instance;

        [MarshalAs(UnmanagedType.LPWStr)]
        public string? Filter;

        public IntPtr CustomFilter;
        public int MaximumCustomFilterCharacters;
        public int FilterIndex;
        public IntPtr FileBuffer;
        public int MaximumFileCharacters;
        public IntPtr FileTitleBuffer;
        public int MaximumFileTitleCharacters;

        [MarshalAs(UnmanagedType.LPWStr)]
        public string? InitialDirectory;

        [MarshalAs(UnmanagedType.LPWStr)]
        public string? Title;

        public uint Flags;
        public ushort FileOffset;
        public ushort FileExtension;

        [MarshalAs(UnmanagedType.LPWStr)]
        public string? DefaultExtension;

        public IntPtr CustomData;
        public IntPtr Hook;
        public IntPtr TemplateName;
        public IntPtr Reserved;
        public int ReservedValue;
        public uint ExtendedFlags;
    }
}
