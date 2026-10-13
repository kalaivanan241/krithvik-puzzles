/** Generated voice clip URLs, keyed by slot (`name` for categories; `prompt`, `retry`, `reveal`, `sound` for items). */
export type SpeechUrls = Partial<Record<string, string>>;
export type Category = { id: number; name: string; description: string; itemCount: number; speech?: SpeechUrls };
export type ItemData = { categoryId: number; key: string; name: string; sound: string; image: string; audioUrl: string; color: string };
export type Item = ItemData & { id: number; speech?: SpeechUrls };
/** Save response; `speech` reports ElevenLabs voice clip generation. */
export type Saved = { id: number; speech: 'ready' | 'failed' | 'disabled' };
export type MediaKind = 'image' | 'audio';

type RequestOptions = { method?: string; body?: unknown; token?: string; signal?: AbortSignal };

const MAX_UPLOAD_BYTES = 20 * 1024 * 1024;

const authorization = (token?: string): Record<string, string> => (token ? { Authorization: `Bearer ${token}` } : {});

async function failure(response: Response, fallback: string): Promise<Error> {
  const message = await response.json().catch(() => null);
  return new Error(message?.error ?? fallback);
}

export async function api<T>(path: string, { method = 'GET', body, token, signal }: RequestOptions = {}): Promise<T> {
  const response = await fetch(`/api${path}`, {
    method,
    signal,
    headers: { ...(body === undefined ? {} : { 'Content-Type': 'application/json' }), ...authorization(token) },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!response.ok) throw await failure(response, `Request failed (${response.status})`);
  return response.json();
}

export async function uploadFile(file: File, token: string): Promise<{ url: string; kind: string }> {
  if (file.size > MAX_UPLOAD_BYTES) throw new Error('Maximum file size is 20 MB.');
  const response = await fetch('/api/uploads', { method: 'POST', headers: authorization(token), body: file });
  if (!response.ok) throw await failure(response, 'Upload failed. Maximum file size is 20 MB.');
  return response.json();
}

const KIND_HINT: Record<MediaKind, string> = {
  image: 'Upload a PNG, JPEG, or WebP image.',
  audio: 'Upload an MP3, WAV, OGG, or M4A sound.',
};

/** Uploads a file and checks the server detected the expected kind. Returns its URL. */
export async function uploadMedia(file: File, kind: MediaKind, token: string): Promise<string> {
  const uploaded = await uploadFile(file, token);
  if (uploaded.kind !== kind) throw new Error(KIND_HINT[kind]);
  return uploaded.url;
}
