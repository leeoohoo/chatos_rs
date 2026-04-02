export const buildWsUrl = (
  baseUrl: string,
  path: string,
  accessToken?: string | null,
): string => {
  const cleanedBase = baseUrl.endsWith('/') ? baseUrl.slice(0, -1) : baseUrl;
  const cleanedPath = path.startsWith('/') ? path : `/${path}`;
  const rawUrl =
    cleanedBase.startsWith('http://') || cleanedBase.startsWith('https://')
      ? cleanedBase.replace(/^http/, 'ws') + cleanedPath
      : (() => {
          const { protocol, host } = window.location;
          const wsProtocol = protocol === 'https:' ? 'wss:' : 'ws:';
          return `${wsProtocol}//${host}${cleanedBase}${cleanedPath}`;
        })();

  const wsUrl = new URL(rawUrl);
  const token = (accessToken || '').trim();
  if (token) {
    wsUrl.searchParams.set('access_token', token);
  }
  return wsUrl.toString();
};
