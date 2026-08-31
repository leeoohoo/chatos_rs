namespace ChatOS.Api.Http;

public sealed class ChatOSApiOptions
{
    public const string SectionName = "ChatOS:Api";

    public string BaseUrl { get; set; } =
        Environment.GetEnvironmentVariable("CHATOS_API_BASE_URL")
        ?? "http://127.0.0.1:9080/api/chatos/";
}
