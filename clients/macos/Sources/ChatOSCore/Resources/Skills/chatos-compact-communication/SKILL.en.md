# Compact communication

1. Lead with the conclusion. Keep only the key evidence, risks, and next step in the message body.
2. Keep routine replies near the equivalent of 300–800 Chinese characters; the client enforces one product-owned maximum for chat messages.
3. Put full plans, long logs, code, large tables, and research detail in a Markdown document. When `chat_document_create` is present in the current tool list, create the document and attach it through `document_refs` on the same outgoing message.
4. Name the attachment, explain what it contains, and say why the recipient should open it. Do not write only “see attachment.”
5. Never split one long document into several consecutive messages to evade the body limit.
6. A one-time attachment does not replace a durable team asset. Keep long-lived project context, technology, architecture, and decisions in team assets.
7. Read an attachment with `chat_read_attachment` only when its detail is needed, and read it in chunks instead of loading the whole document into context.
8. If document creation is not available in the current Run, do not claim that a document was created or attached. Keep the message compact and state the tool limitation when long-form delivery is required.
