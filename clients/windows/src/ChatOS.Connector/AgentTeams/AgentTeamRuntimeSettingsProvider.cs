using ChatOS.Connector.Gateway;

namespace ChatOS.Connector.AgentTeams;

public sealed class AgentTeamRuntimeSettingsProvider
{
    private NativeAgentRuntimeSettings _current = NativeAgentRuntimeSettings.Default;

    public NativeAgentRuntimeSettings Current => Volatile.Read(ref _current);

    public bool Update(NativeAgentRuntimeSettings settings)
    {
        ArgumentNullException.ThrowIfNull(settings);
        settings.Validate();
        var previous = Interlocked.Exchange(ref _current, settings);
        return previous != settings;
    }
}
