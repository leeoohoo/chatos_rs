namespace ChatOS.Connector.LocalAgent;

public sealed record LocalAgentHostOptions(
    string ExecutablePath,
    string DatabasePath,
    TimeSpan StartupTimeout)
{
    public static LocalAgentHostOptions? Detect()
    {
        var configured = Environment.GetEnvironmentVariable("CHATOS_LOCAL_AGENT_HOST_PATH")?.Trim();
        var executable = string.IsNullOrEmpty(configured)
            ? Path.Combine(AppContext.BaseDirectory, "chatos_local_agent_host.exe")
            : Path.GetFullPath(configured);
        if (!File.Exists(executable)) return null;

        var database = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            "ChatOS",
            "LocalAgent",
            "local-agent.sqlite3");
        return new(executable, database, TimeSpan.FromSeconds(10));
    }
}
