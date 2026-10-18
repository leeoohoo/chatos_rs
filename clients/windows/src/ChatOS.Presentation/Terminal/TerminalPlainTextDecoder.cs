using System.Text;

namespace ChatOS.Presentation.Terminal;

/// <summary>
/// Converts a stream of terminal output into readable plain text for controls
/// that do not implement a VT/ANSI terminal emulator.
/// </summary>
public sealed class TerminalPlainTextDecoder
{
    private ParserState _state;

    public void Reset() => _state = ParserState.Text;

    public string Decode(string value)
    {
        if (string.IsNullOrEmpty(value)) return string.Empty;

        var output = new StringBuilder(value.Length);
        foreach (var character in value)
        {
            switch (_state)
            {
                case ParserState.Text:
                    DecodeText(character, output);
                    break;
                case ParserState.Escape:
                    DecodeEscape(character);
                    break;
                case ParserState.ControlSequence:
                    if (character == '\u001b')
                    {
                        _state = ParserState.Escape;
                    }
                    else if (character is >= '@' and <= '~')
                    {
                        _state = ParserState.Text;
                    }
                    break;
                case ParserState.OperatingSystemCommand:
                    if (character == '\a' || character == '\u009c')
                    {
                        _state = ParserState.Text;
                    }
                    else if (character == '\u001b')
                    {
                        _state = ParserState.OperatingSystemCommandEscape;
                    }
                    break;
                case ParserState.OperatingSystemCommandEscape:
                    _state = character == '\\'
                        ? ParserState.Text
                        : character == '\u001b'
                            ? ParserState.OperatingSystemCommandEscape
                            : ParserState.OperatingSystemCommand;
                    break;
                case ParserState.ControlString:
                    if (character == '\u009c')
                    {
                        _state = ParserState.Text;
                    }
                    else if (character == '\u001b')
                    {
                        _state = ParserState.ControlStringEscape;
                    }
                    break;
                case ParserState.ControlStringEscape:
                    _state = character == '\\'
                        ? ParserState.Text
                        : character == '\u001b'
                            ? ParserState.ControlStringEscape
                            : ParserState.ControlString;
                    break;
            }
        }

        return output.ToString();
    }

    private void DecodeText(char character, StringBuilder output)
    {
        switch (character)
        {
            case '\u001b':
                _state = ParserState.Escape;
                return;
            case '\u009b':
                _state = ParserState.ControlSequence;
                return;
            case '\u009d':
                _state = ParserState.OperatingSystemCommand;
                return;
            case '\0':
            case '\a':
            case '\b':
            case '\u007f':
                return;
        }

        if (character < ' ' && character is not ('\t' or '\r' or '\n')) return;
        if (character is >= '\u0080' and <= '\u009f') return;
        output.Append(character);
    }

    private void DecodeEscape(char character)
    {
        _state = character switch
        {
            '[' => ParserState.ControlSequence,
            ']' => ParserState.OperatingSystemCommand,
            'P' or 'X' or '^' or '_' => ParserState.ControlString,
            '\u001b' => ParserState.Escape,
            _ => ParserState.Text,
        };
    }

    private enum ParserState
    {
        Text,
        Escape,
        ControlSequence,
        OperatingSystemCommand,
        OperatingSystemCommandEscape,
        ControlString,
        ControlStringEscape,
    }
}
