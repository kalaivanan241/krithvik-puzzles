export type Category = { id: number; name: string; description: string; itemCount: number };
export type ItemData = { categoryId: number; key: string; name: string; sound: string; image: string; audioUrl: string; color: string };
export type Item = ItemData & { id: number };

export async function api<T>(path: string, method = 'GET', data?: unknown, token?: string, signal?: AbortSignal): Promise<T> {
  const response = await fetch(`/api${path}`, {
    method,
    signal,
    headers: { ...(data === undefined ? {} : { 'Content-Type': 'application/json' }), ...(token ? { Authorization: `Bearer ${token}` } : {}) },
    body: data === undefined ? undefined : JSON.stringify(data),
  });
  if (!response.ok) {
    const message = await response.json().catch(() => null);
    throw new Error(message?.error ?? `Request failed (${response.status})`);
  }
  return response.json();
}

const MAX_UPLOAD_BYTES = 20 * 1024 * 1024;

export async function uploadFile(file: File, token: string): Promise<{ url: string; kind: string }> {
  if (file.size > MAX_UPLOAD_BYTES) throw new Error('Maximum file size is 20 MB.');
  const response = await fetch('/api/uploads', { method: 'POST', headers: { Authorization: `Bearer ${token}` }, body: file });
  if (!response.ok) {
    const message = await response.json().catch(() => null);
    throw new Error(message?.error ?? 'Upload failed. Maximum file size is 20 MB.');
  }
  return response.json();
}
