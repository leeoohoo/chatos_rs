using System.Net;
using System.Text.Json.Serialization;
using ChatOS.Api.Http;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Api.Conversation;

public sealed class ConversationCommandService : IConversationCommandService
{
    private readonly ChatOSApiClient _client;
    private readonly IConversationAttachmentService _attachmentService;

    public ConversationCommandService(
        ChatOSApiClient client,
        IConversationAttachmentService attachmentService)
    {
        _client = client;
        _attachmentService = attachmentService;
    }

    public async Task<ConversationCommandAck> SendNewTurnAsync(
        ConversationSendCommand command,
        CancellationToken cancellationToken = default)
    {
        var runtimeTask = _client.GetAsync<RuntimeSettingsDto>(
            $"conversations/{Uri.EscapeDataString(command.ConversationId)}/runtime-settings",
            cancellationToken);
        var modelsTask = _client.GetAsync<IReadOnlyList<ModelConfigDto>>(
            "ai-model-configs",
            cancellationToken);
        await Task.WhenAll(runtimeTask, modelsTask).ConfigureAwait(false);

        var runtime = runtimeTask.Result;
        var model = ResolveModel(runtime, modelsTask.Result);
        var attachments = await _attachmentService.UploadAsync(
            command.Attachments,
            command.ConversationId,
            cancellationToken).ConfigureAwait(false);
        var request = new ChatCommandRequestDto(
            command.ConversationId,
            command.Content,
            attachments.Select(AttachmentReferenceDto.FromDomain).ToArray(),
            command.ReasoningEnabled ?? runtime.ReasoningEnabled,
            command.PlanModeEnabled ?? runtime.PlanModeEnabled,
            command.TurnId,
            runtime.RemoteConnectionId,
            runtime.WorkspaceRoot,
            model.Id,
            new AiModelConfigDto(
                model.Temperature ?? 0.7,
                model.ModelName,
                model.ThinkingLevel.TrimmedOrNull()));
        var response = await _client.PostAsync<ChatCommandResponseDto>(
            "agent/chat/send",
            request,
            cancellationToken).ConfigureAwait(false);
        return MapAcknowledgement(response, command.TurnId, "The chat command was not accepted.");
    }

    public async Task<ConversationCommandAck> SendGuidanceAsync(
        ConversationSendCommand command,
        CancellationToken cancellationToken = default)
    {
        var attachments = await _attachmentService.UploadAsync(
            command.Attachments,
            command.ConversationId,
            cancellationToken).ConfigureAwait(false);
        ChatCommandResponseDto response;
        try
        {
            response = await _client.PostAsync<ChatCommandResponseDto>(
                "agent/chat/guidance",
                new GuidanceRequestDto(
                    command.ConversationId,
                    command.TurnId,
                    command.Content,
                    attachments.Select(AttachmentReferenceDto.FromDomain).ToArray()),
                cancellationToken).ConfigureAwait(false);
        }
        catch (ChatOSApiException exception) when (IsInactiveGuidanceConflict(exception))
        {
            throw new GuidanceTargetInactiveException();
        }

        return MapAcknowledgement(response, command.TurnId, "The guidance command was not accepted.");
    }

    public async Task StopTurnAsync(
        string conversationId,
        string? turnId,
        CancellationToken cancellationToken = default)
    {
        var response = await _client.PostAsync<StopChatResponseDto>(
            "agent/chat/stop",
            new StopChatRequestDto(conversationId, turnId.TrimmedOrNull()),
            cancellationToken).ConfigureAwait(false);
        if (!response.Success)
        {
            throw new ChatOSApiException(
                response.Message.TrimmedOrNull() ?? "The active AI turn could not be stopped.",
                HttpStatusCode.Conflict);
        }
    }

    private static ModelConfigDto ResolveModel(
        RuntimeSettingsDto runtime,
        IReadOnlyList<ModelConfigDto> configs)
    {
        var enabled = configs
            .Where(static config => config.Enabled != false && config.ModelName.TrimmedOrNull() is not null)
            .ToArray();
        var selectedId = runtime.SelectedModelId.TrimmedOrNull();
        var selected = selectedId is null
            ? null
            : enabled.FirstOrDefault(config => string.Equals(config.Id, selectedId, StringComparison.Ordinal));
        return selected ?? enabled.FirstOrDefault()
            ?? throw new ChatOSApiException("No enabled AI model is configured for this conversation.");
    }

    private static ConversationCommandAck MapAcknowledgement(
        ChatCommandResponseDto response,
        string fallbackTurnId,
        string errorMessage)
    {
        if (response.Accepted == false)
        {
            throw new ChatOSApiException(errorMessage, HttpStatusCode.Conflict);
        }

        return new ConversationCommandAck(
            true,
            response.TurnId.TrimmedOrNull() ?? fallbackTurnId,
            response.SourceUserMessageId.TrimmedOrNull() ?? response.UserMessageId.TrimmedOrNull());
    }

    private static bool IsInactiveGuidanceConflict(ChatOSApiException exception)
    {
        if (exception.StatusCode != HttpStatusCode.Conflict)
        {
            return false;
        }

        var message = $"{exception.Message} {exception.ResponseBody}".ToLowerInvariant();
        return message.Contains("目标轮次已结束", StringComparison.Ordinal) ||
               message.Contains("无法追加指令", StringComparison.Ordinal) ||
               message.Contains("turn has ended", StringComparison.Ordinal) ||
               message.Contains("turn is no longer active", StringComparison.Ordinal);
    }
}

internal sealed record ChatCommandRequestDto(
    [property: JsonPropertyName("conversation_id")] string ConversationId,
    [property: JsonPropertyName("content")] string Content,
    [property: JsonPropertyName("attachments")] IReadOnlyList<AttachmentReferenceDto> Attachments,
    [property: JsonPropertyName("reasoning_enabled")] bool ReasoningEnabled,
    [property: JsonPropertyName("plan_mode")] bool PlanMode,
    [property: JsonPropertyName("turn_id")] string TurnId,
    [property: JsonPropertyName("remote_connection_id")] string? RemoteConnectionId,
    [property: JsonPropertyName("workspace_root")] string? WorkspaceRoot,
    [property: JsonPropertyName("model_config_id")] string ModelConfigId,
    [property: JsonPropertyName("ai_model_config")] AiModelConfigDto AiModelConfig);

internal sealed record AiModelConfigDto(
    [property: JsonPropertyName("temperature")] double Temperature,
    [property: JsonPropertyName("model_name")] string ModelName,
    [property: JsonPropertyName("thinking_level")] string? ThinkingLevel);

internal sealed record GuidanceRequestDto(
    [property: JsonPropertyName("conversation_id")] string ConversationId,
    [property: JsonPropertyName("turn_id")] string TurnId,
    [property: JsonPropertyName("content")] string Content,
    [property: JsonPropertyName("attachments")] IReadOnlyList<AttachmentReferenceDto> Attachments);

internal sealed record StopChatRequestDto(
    [property: JsonPropertyName("conversation_id")] string ConversationId,
    [property: JsonPropertyName("turn_id")] string? TurnId);

internal sealed record StopChatResponseDto(
    [property: JsonPropertyName("success")] bool Success,
    [property: JsonPropertyName("message")] string? Message);

internal sealed record ChatCommandResponseDto
{
    [JsonPropertyName("accepted")]
    public bool? Accepted { get; init; }

    [JsonPropertyName("turn_id")]
    public string? TurnId { get; init; }

    [JsonPropertyName("user_message_id")]
    public string? UserMessageId { get; init; }

    [JsonPropertyName("source_user_message_id")]
    public string? SourceUserMessageId { get; init; }
}

internal sealed record AttachmentReferenceDto(
    [property: JsonPropertyName("id")] string Id,
    [property: JsonPropertyName("name")] string Name,
    [property: JsonPropertyName("mimeType")] string MimeType,
    [property: JsonPropertyName("size")] int Size,
    [property: JsonPropertyName("type")] string Kind,
    [property: JsonPropertyName("storageProvider")] string? StorageProvider,
    [property: JsonPropertyName("bucket")] string? Bucket,
    [property: JsonPropertyName("objectKey")] string? ObjectKey,
    [property: JsonPropertyName("url")] string? Url,
    [property: JsonPropertyName("viewUrl")] string? ViewUrl)
{
    public static AttachmentReferenceDto FromDomain(ConversationAttachmentReference value) => new(
        value.Id,
        value.Name,
        value.MimeType,
        value.Size,
        value.Kind.ToApiValue(),
        value.StorageProvider,
        value.Bucket,
        value.ObjectKey,
        value.Url,
        value.ViewUrl);
}
