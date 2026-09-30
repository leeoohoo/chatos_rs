namespace ChatOS.Connector.LocalAgent;

public sealed record LocalAgentHostOptions(
    string ExecutablePath,
    string DatabasePath,
    TimeSpan StartupTimeout,
    Uri MemoryBaseUri,
    string MemorySourceId,
    TimeSpan MemoryTimeout)
{
    public static LocalAgentHostOptions? Detect(string? apiBaseUrl = null)
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
        return new(
            executable,
            database,
            TimeSpan.FromSeconds(10),
            ResolveMemoryBaseUri(apiBaseUrl),
            "local_agent",
            TimeSpan.FromSeconds(30));
    }

    private static Uri ResolveMemoryBaseUri(string? apiBaseUrl)
    {
        var configured = Environment.GetEnvironmentVariable("CHATOS_MEMORY_BASE_URL")?.Trim();
        var raw = string.IsNullOrWhiteSpace(configured)
            ? apiBaseUrl?.Trim() ?? "http://127.0.0.1:9080/"
            : configured;
        if (!Uri.TryCreate(raw, UriKind.Absolute, out var baseUri) ||
            baseUri.Scheme is not ("http" or "https") ||
            !string.IsNullOrEmpty(baseUri.UserInfo) ||
            !string.IsNullOrEmpty(baseUri.Query) ||
            !string.IsNullOrEmpty(baseUri.Fragment))
        {
            throw new InvalidOperationException("Local Agent Memory base URL must be an absolute HTTP(S) URL.");
        }
        if (!string.IsNullOrWhiteSpace(configured)) return baseUri;
        return new Uri(baseUri.AbsoluteUri.TrimEnd('/') + "/api/memory", UriKind.Absolute);
    }
}
