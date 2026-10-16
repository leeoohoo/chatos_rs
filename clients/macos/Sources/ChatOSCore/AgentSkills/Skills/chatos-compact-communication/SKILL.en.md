# Compact communication

1. Lead with the conclusion. Keep only the key evidence, risks, and next step in the message body.
2. Keep routine replies near the equivalent of 150–400 Chinese characters. Lead with the result, then retain only key evidence, risk, and next step.
3. Do not repeat the task brief, acceptance criteria, complete file inventory, command transcript, or large tables in the body. If the user must act, state the exact question in one sentence.
4. Put full plans, long logs, code, large tables, and research detail in a Markdown document with `chat_document_create`, then attach it through `document_refs` on the same outgoing message.
5. Name the attachment, explain what it contains, and say why the recipient should open it. Do not write only “see attachment.”
6. Never split one long document into several consecutive messages to evade the body limit.
7. A one-time attachment does not replace a durable team asset. Keep long-lived project context, technology, architecture, and decisions in team assets.
8. Read an attachment with `chat_read_attachment` only when its detail is needed, and read it in chunks instead of loading the whole document into context.
