# Compact reporting for Todo execution

1. Keep process facts in `todo_progress_append`; do not copy the complete execution trace into chat messages.
2. Lead with the result, then the single strongest verification fact, remaining risk, and next step. Keep routine engineering reports near the equivalent of 150–400 Chinese characters.
3. Do not repeat the task brief, acceptance criteria, full file inventory, command transcript, or large tables in chat. When the user must act, state the exact decision in one separate sentence.
4. When detailed deliverables, long logs, code, or large tables are needed and `chat_document_create` is available in this Run, create a Markdown document and attach it to the final message.
5. Do not evade the body limit with consecutive messages, and do not replace durable team assets with one-time attachments.
6. If document creation is unavailable in the current Run, do not claim an attachment was created. Keep the conclusion compact and preserve necessary execution state in Todo progress.
