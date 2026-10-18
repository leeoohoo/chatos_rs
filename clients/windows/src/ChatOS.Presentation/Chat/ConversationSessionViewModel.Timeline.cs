using ChatOS.Core.Domain;

namespace ChatOS.Presentation.Chat;

public sealed partial class ConversationSessionViewModel
{
    private void ApplyTimelineSnapshot(IReadOnlyList<ConversationTurn> turns)
    {
        var existing = Turns.ToDictionary(static turn => turn.Id, StringComparer.Ordinal);
        // The history store only accepts content changes with a newer revision.
        // Keep unchanged native items, including their expansion and pending reads.
        var desired = turns.Select(turn =>
            existing.TryGetValue(turn.Id, out var item) && item.Revision == turn.Revision
                ? item
                : new ConversationTurnItemViewModel(turn)).ToArray();
        var retained = desired.ToHashSet();

        for (var index = Turns.Count - 1; index >= 0; index--)
        {
            if (retained.Contains(Turns[index])) continue;
            foreach (var reply in Turns[index].Replies) reply.CloseInspector();
            Turns.RemoveAt(index);
        }

        for (var index = 0; index < desired.Length; index++)
        {
            var item = desired[index];
            if (index < Turns.Count && ReferenceEquals(Turns[index], item)) continue;
            var previousIndex = Turns.IndexOf(item);
            if (previousIndex >= 0) Turns.Move(previousIndex, index);
            else Turns.Insert(index, item);
        }
    }
}
