using ChatOS.Core.Abstractions;
using ChatOS.Core.State;
using ChatOS.Presentation.Settings;
using ChatOS.Presentation.Threading;

namespace ChatOS.Presentation.Chat;

public sealed class ConversationSessionFactory(
    IConversationHistoryService historyService,
    IConversationCacheStore cacheStore,
    IConversationCommandService commandService,
    IConversationRuntimeSettingsService runtimeService,
    IAskUserPromptService askUserService,
    IRealtimeClient realtimeClient,
    ConversationHistoryStore historyStore,
    IUiDispatcher dispatcher,
    LocalizationViewModel? localization = null)
{
    public ConversationSessionViewModel Create() => new(
        historyService,
        cacheStore,
        commandService,
        runtimeService,
        askUserService,
        realtimeClient,
        historyStore,
        dispatcher,
        localization);
}
