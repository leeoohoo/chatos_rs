# Compact reporting for Todo execution

1. Keep process facts in `todo_progress_append`; do not copy the complete execution trace into chat messages.
2. Lead the final report with the result, completion evidence, risks, and next step, staying near the equivalent of 300–800 Chinese characters.
3. When detailed deliverables, long logs, code, or large tables are needed and `chat_document_create` is available in this Run, create a Markdown document and attach it to the final message.
4. Do not evade the body limit with consecutive messages, and do not replace durable team assets with one-time attachments.
5. If document creation is unavailable in the current Run, do not claim an attachment was created. Keep the conclusion compact and preserve necessary execution state in Todo progress.
