// Keep verification clients on the same public MCP contract as model callers.
// The process owns the session ID and selects the actual browser mode.
export async function withChromeSession(callTool, sessionName, work) {
  const session = await callTool('browser_session_open', {session_name: sessionName});
  try {
    if (session?.mode !== 'chrome_extension') {
      throw new Error('Paired Chrome is unavailable; verification did not operate the managed fallback');
    }
    return await work(session);
  } finally {
    await callTool('browser_session_close', {});
  }
}

export async function createVerificationTabs(callTool) {
  const first = await callTool('browser_tab_new', {
    url: 'https://example.com/?browser-mcp-verification=one'
  });
  const second = await callTool('browser_tab_new', {
    url: 'https://example.com/?browser-mcp-verification=two'
  });
  return [first, second];
}
