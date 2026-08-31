using System.Net;

namespace ChatOS.Api.Http;

public sealed class ChatOSApiException : Exception
{
    public ChatOSApiException(
        string message,
        HttpStatusCode? statusCode = null,
        string? responseBody = null,
        Exception? innerException = null)
        : base(message, innerException)
    {
        StatusCode = statusCode;
        ResponseBody = responseBody;
    }

    public HttpStatusCode? StatusCode { get; }

    public string? ResponseBody { get; }

    public bool IsAuthenticationExpired => StatusCode == HttpStatusCode.Unauthorized;
}
