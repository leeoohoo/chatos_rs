using System.Text.Json;
using ChatOS.Core.Abstractions;
using ChatOS.Core.Domain;

namespace ChatOS.Connector.LocalAgent;

public sealed class WindowsLocalAgentAskUserPromptService : IAskUserPromptService
{
    private const string Prefix = "local-ask:";
    private readonly WindowsLocalAgentRuntimeClient _runtime;
    private readonly WindowsLocalAgentConversationClient _conversations;
    private readonly object _gate = new();
    private string? _owner;

    public WindowsLocalAgentAskUserPromptService(
        WindowsLocalAgentRuntimeClient runtime,
        WindowsLocalAgentConversationClient conversations)
    {
        _runtime = runtime; _conversations = conversations;
    }

    public void Configure(string ownerUserId) { lock (_gate) _owner = ownerUserId; }
    public void Reset() { lock (_gate) _owner = null; }

    public async Task<IReadOnlyList<AskUserPrompt>> FetchPromptsAsync(
        string conversationId, int limit = 100,
        CancellationToken cancellationToken = default)
    {
        var owner = RequireOwner();
        var page = await _runtime.ListRunsAsync(
                owner, "active", 100, status: "waiting_user",
                cancellationToken: cancellationToken)
            .ConfigureAwait(false);
        var prompts = new List<AskUserPrompt>();
        foreach (var run in page.Runs.Where(value => value.Status == "waiting_user"))
        {
            if (Context(run) is not { } context || context.ConversationId != conversationId) continue;
            if (await PromptAsync(run, context, cancellationToken).ConfigureAwait(false) is { } prompt)
                prompts.Add(prompt);
            if (prompts.Count == Math.Clamp(limit, 1, 100)) break;
        }
        return prompts;
    }

    public async Task<AskUserPrompt> SubmitAsync(
        string promptId, string conversationId, AskUserSubmission submission,
        CancellationToken cancellationToken = default)
    {
        var resolved = await ResolveAsync(promptId, conversationId, cancellationToken)
            .ConfigureAwait(false);
        RejectSecrets(submission, resolved.Prompt);
        var input = SubmissionInput(promptId, submission);
        if (resolved.Run.ProfileKey == "main_chat")
        {
            var conversation = await _conversations.GetAsync(
                resolved.Run.OwnerUserId, conversationId, cancellationToken).ConfigureAwait(false);
            _ = await _conversations.ResumeTurnAsync(new(
                "resume_conversation_turn", resolved.Run.OwnerUserId, conversationId,
                conversation.Conversation.Version, resolved.Context.TurnId, resolved.Run.Version,
                "waiting_user", $"local-ask-response-{Guid.NewGuid():N}",
                SubmissionMessage(submission), input, [], "ask_user_submitted"), cancellationToken)
                .ConfigureAwait(false);
        }
        else
        {
            _ = await _runtime.ResumeWaitingRunAsync(
                resolved.Run.OwnerUserId, resolved.Run, input, cancellationToken).ConfigureAwait(false);
        }
        return resolved.Prompt with { Status = AskUserPromptStatus.Ok, UpdatedAt = DateTimeOffset.UtcNow };
    }

    public async Task<AskUserPrompt> CancelAsync(
        string promptId, string conversationId,
        CancellationToken cancellationToken = default)
    {
        var resolved = await ResolveAsync(promptId, conversationId, cancellationToken)
            .ConfigureAwait(false);
        if (resolved.Run.ProfileKey == "main_chat")
        {
            var conversation = await _conversations.GetAsync(
                resolved.Run.OwnerUserId, conversationId, cancellationToken).ConfigureAwait(false);
            _ = await _conversations.CancelTurnAsync(new(
                "cancel_conversation_turn", resolved.Run.OwnerUserId, conversationId,
                conversation.Conversation.Version, resolved.Context.TurnId,
                resolved.Run.Version, "user_cancelled"), cancellationToken).ConfigureAwait(false);
        }
        else if (resolved.Run.OwnerEntityType == "task")
        {
            await _runtime.CancelTaskAsync(
                resolved.Run.OwnerUserId, resolved.Run.OwnerEntityId, cancellationToken)
                .ConfigureAwait(false);
        }
        else throw new InvalidOperationException("The local Run cannot be cancelled here.");
        return resolved.Prompt with {
            Status = AskUserPromptStatus.Canceled, UpdatedAt = DateTimeOffset.UtcNow,
        };
    }

    private async Task<Resolved> ResolveAsync(
        string promptId, string conversationId, CancellationToken cancellationToken)
    {
        if (!promptId.StartsWith(Prefix, StringComparison.Ordinal) || promptId.Length == Prefix.Length)
            throw new InvalidOperationException("The local Ask User prompt was not found.");
        var run = await _runtime.GetRunAsync(
            RequireOwner(), promptId[Prefix.Length..], cancellationToken).ConfigureAwait(false);
        if (run.Status != "waiting_user" || Context(run) is not { } context ||
            context.ConversationId != conversationId ||
            await PromptAsync(run, context, cancellationToken).ConfigureAwait(false) is not { } prompt)
            throw new InvalidOperationException("The local Ask User prompt is no longer pending.");
        return new(run, context, prompt);
    }

    private async Task<AskUserPrompt?> PromptAsync(
        WindowsLocalAgentRun run, RunContext context, CancellationToken cancellationToken)
    {
        var page = await _runtime.ListEventPageAsync(
            run.OwnerUserId, 0, run.RunId, 1, "user_input_requested", newestFirst: true,
            cancellationToken: cancellationToken).ConfigureAwait(false);
        var requested = page.Events.FirstOrDefault();
        if (requested?.Payload is not { ValueKind: JsonValueKind.Object } payload ||
            !payload.TryGetProperty("prompt", out var value)) return null;
        return MapPrompt(value, run, context, requested);
    }

    private static AskUserPrompt MapPrompt(
        JsonElement value, WindowsLocalAgentRun run, RunContext context,
        WindowsLocalAgentEvent requested)
    {
        var stored = value.ValueKind == JsonValueKind.Object ? value : default;
        var payload = Object(stored, "payload") is { } nested ? nested : stored;
        var fields = Array(payload, "fields").Select(MapField).Where(value => value is not null)
            .Cast<AskUserField>().ToArray();
        var choice = Object(payload, "choice") is { } choiceValue ? MapChoice(choiceValue) : null;
        var kind = String(stored, "kind") ?? (fields.Length > 0 && choice is not null
            ? "mixed" : fields.Length > 0 ? "fields" : choice is not null ? "choice" : "confirmation");
        return new AskUserPrompt(
            Prefix + run.RunId, context.ConversationId, context.TurnId,
            String(stored, "tool_call_id") ?? run.RunId, kind, AskUserPromptStatus.Pending,
            String(stored, "title") ?? "需要你的回复",
            String(stored, "message") ?? String(stored, "question") ??
                String(payload, "message") ?? String(payload, "question") ??
                (value.ValueKind == JsonValueKind.String ? value.GetString() : null) ??
                "请提供继续执行所需的信息。",
            Bool(stored, "allow_cancel") ?? true, Int64(stored, "timeout_ms"),
            fields, choice, Date(requested.CreatedAtUnixMs), Date(run.UpdatedAtUnixMs));
    }

    private static AskUserField? MapField(JsonElement field, int index)
    {
        if (field.ValueKind != JsonValueKind.Object) return null;
        var label = String(field, "label")?.Trim();
        var key = String(field, "key") ?? String(field, "name") ?? String(field, "id")
            ?? NormalizeKey(label) ?? $"field_{index + 1}";
        if (key.Length == 0) return null;
        return new(key, string.IsNullOrWhiteSpace(label) ? key : label!,
            String(field, "description"), String(field, "placeholder"),
            String(field, "default_value") ?? String(field, "default") ?? string.Empty,
            Bool(field, "required") ?? false, Bool(field, "multiline") ?? false,
            Bool(field, "secret") ?? false);
    }

    private static AskUserChoice? MapChoice(JsonElement value)
    {
        var options = Array(value, "options").Select(item => new AskUserChoiceOption(
            String(item, "value") ?? string.Empty,
            String(item, "label") ?? String(item, "value") ?? string.Empty,
            String(item, "description"))).Where(item => item.Value.Length > 0).ToArray();
        if (options.Length == 0) return null;
        var multiple = Bool(value, "multiple") ?? false;
        IReadOnlyList<string> defaults = value.TryGetProperty("default", out var selected) &&
            selected.ValueKind == JsonValueKind.Array
            ? selected.EnumerateArray().Where(item => item.ValueKind == JsonValueKind.String)
                .Select(item => item.GetString()).OfType<string>().ToArray()
            : String(value, "default") is { } single ? [single] : [];
        var minimum = Math.Max(0, Int32(value, "min_selections") ?? 0);
        return new(multiple, options, defaults, minimum,
            Math.Max(minimum, Int32(value, "max_selections") ?? (multiple ? options.Length : 1)));
    }

    private static void RejectSecrets(AskUserSubmission submission, AskUserPrompt prompt)
    {
        var keys = prompt.Fields.Where(field => field.IsSecret || Sensitive(field.Key))
            .Select(field => field.Key).ToHashSet(StringComparer.Ordinal);
        if (submission.Values.Any(value => value.Value.Length > 0 &&
            (keys.Contains(value.Key) || Sensitive(value.Key))))
            throw new InvalidOperationException("Secret answers cannot be stored in Local Agent state.");
    }

    private static JsonElement SubmissionInput(string promptId, AskUserSubmission submission) =>
        JsonSerializer.SerializeToElement(new {
            source = "ask_user", prompt_id = promptId, values = submission.Values,
            selection = submission.Selection switch {
                AskUserSelection.Single single => (object)single.Value,
                AskUserSelection.Multiple multiple => multiple.Values,
                _ => null,
            },
        });

    private static string SubmissionMessage(AskUserSubmission submission)
    {
        var lines = submission.Values.OrderBy(value => value.Key)
            .Select(value => $"{value.Key}: {value.Value}").ToList();
        if (submission.Selection is AskUserSelection.Single single) lines.Add(single.Value);
        if (submission.Selection is AskUserSelection.Multiple multiple)
            lines.Add(string.Join(", ", multiple.Values));
        return lines.Count == 0 ? "已提交回复" : string.Join("\n", lines);
    }

    private static RunContext? Context(WindowsLocalAgentRun run)
    {
        var conversation = String(run.Input, run.ProfileKey == "main_chat"
            ? "conversation_id" : "source_conversation_id");
        var turn = String(run.Input, run.ProfileKey == "main_chat" ? "turn_id" : "source_turn_id");
        return conversation is null || turn is null ? null : new(conversation, turn);
    }

    private string RequireOwner() { lock (_gate) return _owner ?? throw new InvalidOperationException(
        "Local Agent Ask User is not configured."); }
    private static bool Sensitive(string key)
    {
        var normalized = new string(key.ToLowerInvariant().Where(char.IsLetter).ToArray());
        return new[] { "apikey", "accesstoken", "password", "passwd", "secret", "credential" }
            .Any(normalized.Contains);
    }
    private static string? NormalizeKey(string? value) => string.IsNullOrWhiteSpace(value) ? null
        : new string(value.ToLowerInvariant().Select(character => char.IsLetterOrDigit(character) || character == '_'
            ? character : '_').ToArray()).Trim('_');
    private static JsonElement? Object(JsonElement value, string name) => value.ValueKind == JsonValueKind.Object &&
        value.TryGetProperty(name, out var child) && child.ValueKind == JsonValueKind.Object ? child : null;
    private static IEnumerable<JsonElement> Array(JsonElement value, string name) =>
        value.ValueKind == JsonValueKind.Object && value.TryGetProperty(name, out var child) &&
        child.ValueKind == JsonValueKind.Array ? child.EnumerateArray().ToArray() : [];
    private static string? String(JsonElement value, string name) => value.ValueKind == JsonValueKind.Object &&
        value.TryGetProperty(name, out var child) && child.ValueKind == JsonValueKind.String ? child.GetString() : null;
    private static bool? Bool(JsonElement value, string name) => value.ValueKind == JsonValueKind.Object &&
        value.TryGetProperty(name, out var child) && child.ValueKind is JsonValueKind.True or JsonValueKind.False
            ? child.GetBoolean() : null;
    private static int? Int32(JsonElement value, string name) => value.ValueKind == JsonValueKind.Object &&
        value.TryGetProperty(name, out var child) && child.ValueKind == JsonValueKind.Number &&
        child.TryGetInt32(out var result) ? result : null;
    private static long? Int64(JsonElement value, string name) => value.ValueKind == JsonValueKind.Object &&
        value.TryGetProperty(name, out var child) && child.ValueKind == JsonValueKind.Number &&
        child.TryGetInt64(out var result) ? result : null;
    private static DateTimeOffset Date(long value) => DateTimeOffset.FromUnixTimeMilliseconds(value);
    private sealed record RunContext(string ConversationId, string TurnId);
    private sealed record Resolved(WindowsLocalAgentRun Run, RunContext Context, AskUserPrompt Prompt);
}
