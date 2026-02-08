const API_BASE = import.meta.env.VITE_CODE_API || '';

export async function apiGet<T>(path: string): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    method: 'GET',
    headers: { 'Accept': 'application/json' }
  });
  const data = await safeJson(res);
  if (!res.ok) {
    throw new Error((data && data.error) || `Request failed (${res.status})`);
  }
  if (data && data.ok === false) {
    throw new Error(data.error || 'Request failed');
  }
  return data as T;
}

async function safeJson(res: Response): Promise<any> {
  const text = await res.text();
  if (!text) return {};
  try {
    return JSON.parse(text);
  } catch {
    return { error: 'Invalid JSON response' };
  }
}
