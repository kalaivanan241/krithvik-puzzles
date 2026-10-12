import { useEffect, useState } from "preact/hooks";
import {
  api,
  uploadFile,
  type Category,
  type Item,
  type ItemData,
} from "./catalog";
import { ANIMAL_ART } from "./game";

export function Management({ onClose }: { onClose: () => void }) {
  const [token, setToken] = useState("");
  const [authenticated, setAuthenticated] = useState(false);
  const [categories, setCategories] = useState<Category[]>([]);
  const [selected, setSelected] = useState(0);
  const [items, setItems] = useState<Item[]>([]);
  const [categoryId, setCategoryId] = useState<number | null>(null);
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [itemId, setItemId] = useState<number | null>(null);
  const blank = (): ItemData => ({
    categoryId: selected,
    key: "",
    name: "",
    sound: "",
    image: "",
    audioUrl: "",
    color: "#ffe7b8",
  });
  const [draft, setDraft] = useState<ItemData>(blank);
  const [imageFile, setImageFile] = useState<File>();
  const [audioFile, setAudioFile] = useState<File>();
  const [formVersion, setFormVersion] = useState(0);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);

  async function refresh(id = selected) {
    const list = await api<Category[]>("/categories");
    setCategories(list);
    const next = list.some((c) => c.id === id) ? id : (list[0]?.id ?? 0);
    if (next === selected) {
      setItems(next ? await api<Item[]>(`/items?categoryId=${next}`) : []);
      return;
    }
    setItems([]);
    setSelected(next);
  }
  useEffect(() => {
    const controller = new AbortController();
    let active = true;
    if (!selected) setItems([]);
    else
      void api<Item[]>(
        `/items?categoryId=${selected}`,
        "GET",
        undefined,
        undefined,
        controller.signal,
      )
        .then((rows) => {
          if (active) setItems(rows);
        })
        .catch((e: { name?: string; message?: string }) => {
          if (!active || e?.name === "AbortError") return;
          setError(e.message ?? "Could not load items");
        });
    setItemId(null);
    setDraft({
      categoryId: selected,
      key: "",
      name: "",
      sound: "",
      image: "",
      audioUrl: "",
      color: "#ffe7b8",
    });
    setImageFile(undefined);
    setAudioFile(undefined);
    setFormVersion((v) => v + 1);
    return () => {
      active = false;
      controller.abort();
    };
  }, [selected]);
  function resetItem() {
    setItemId(null);
    setDraft(blank());
    setImageFile(undefined);
    setAudioFile(undefined);
    setFormVersion((v) => v + 1);
  }
  async function action(fn: () => Promise<void>) {
    setBusy(true);
    setError("");
    setNotice("");
    try {
      await fn();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Operation failed");
    } finally {
      setBusy(false);
    }
  }
  return (
    <main class="management">
      <div class="management-heading">
        <div>
          <span class="eyebrow">PARENT LIBRARY</span>
          <h1>Categories & items</h1>
        </div>
        <button class="secondary" onClick={onClose}>
          Back to game
        </button>
      </div>
      {!authenticated ? (
        <form
          class="admin-card"
          onSubmit={(e) => {
            e.preventDefault();
            void action(async () => {
              await api("/admin/session", "GET", undefined, token);
              await refresh();
              setAuthenticated(true);
            });
          }}
        >
          <h2>Unlock management</h2>
          <p>
            Enter ADMIN_TOKEN, or the token from
            backend/data/peekaboo.admin-token. It is kept only in memory.
          </p>
          <label>
            Admin token
            <input
              type="password"
              required
              value={token}
              onInput={(e) => setToken(e.currentTarget.value)}
              autoComplete="off"
            />
          </label>
          <button class="secondary" disabled={busy}>
            Unlock
          </button>
        </form>
      ) : (
        <>
          <p class="management-help">
            Upload an image and a recorded sound for every new item. A category
            needs at least 3 items to play; 8 items unlock all six levels.
          </p>
          <div class="admin-grid">
            <section class="admin-card">
              <h2>Categories</h2>
              <div class="category-list">
                {categories.map((c) => (
                  <div
                    class={selected === c.id ? "selected-row" : ""}
                    key={c.id}
                  >
                    <button
                      class="secondary"
                      disabled={busy}
                      onClick={() => {
                        setSelected(c.id);
                        setCategoryId(null);
                        setName("");
                        setDescription("");
                      }}
                    >
                      {c.name} ({c.itemCount})
                    </button>
                    <button
                      disabled={busy}
                      onClick={() => {
                        setCategoryId(c.id);
                        setName(c.name);
                        setDescription(c.description);
                      }}
                    >
                      Edit
                    </button>
                    <button
                      disabled={busy}
                      onClick={() => {
                        if (confirm(`Delete "${c.name}" and all its items?`))
                          void action(async () => {
                            await api(
                              `/categories/${c.id}`,
                              "DELETE",
                              undefined,
                              token,
                            );
                            await refresh();
                            setCategoryId(null);
                            setName("");
                            setDescription("");
                            setNotice("Category deleted.");
                          });
                      }}
                    >
                      Delete
                    </button>
                  </div>
                ))}
              </div>
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  void action(async () => {
                    await api(
                      categoryId ? `/categories/${categoryId}` : "/categories",
                      categoryId ? "PUT" : "POST",
                      { name, description },
                      token,
                    );
                    await refresh();
                    setName("");
                    setDescription("");
                    setCategoryId(null);
                    setNotice("Category saved.");
                  });
                }}
              >
                <h3>{categoryId ? "Edit category" : "New category"}</h3>
                <label>
                  Name
                  <input
                    required
                    maxLength={100}
                    value={name}
                    onInput={(e) => setName(e.currentTarget.value)}
                  />
                </label>
                <label>
                  Description
                  <textarea
                    maxLength={1000}
                    value={description}
                    onInput={(e) => setDescription(e.currentTarget.value)}
                  />
                </label>
                <button class="secondary" disabled={busy}>
                  Save category
                </button>
                {categoryId && (
                  <button
                    type="button"
                    onClick={() => {
                      setCategoryId(null);
                      setName("");
                      setDescription("");
                    }}
                  >
                    Cancel edit
                  </button>
                )}
              </form>
            </section>
            <section class="admin-card">
              <h2>
                {categories.find((c) => c.id === selected)?.name ??
                  "Select a category"}{" "}
                items
              </h2>
              <div class="item-list">
                {items.map((item) => (
                  <article key={item.id}>
                    {(item.image || ANIMAL_ART[item.key]?.image) && (
                      <img
                        src={item.image || ANIMAL_ART[item.key]?.image}
                        alt={item.name}
                      />
                    )}
                    <div>
                      <strong>{item.name}</strong>
                      <small>{item.key}</small>
                      {item.audioUrl ? (
                        <audio controls preload="none" src={item.audioUrl} />
                      ) : (
                        <small>
                          Legacy speech: upload audio to replace it.
                        </small>
                      )}
                    </div>
                    <div class="row-actions">
                      <button
                        disabled={busy}
                        onClick={() => {
                          setItemId(item.id);
                          setDraft({
                            categoryId: item.categoryId,
                            key: item.key,
                            name: item.name,
                            sound: item.sound,
                            image: item.image,
                            audioUrl: item.audioUrl,
                            color: item.color,
                          });
                          setImageFile(undefined);
                          setAudioFile(undefined);
                          setFormVersion((v) => v + 1);
                        }}
                      >
                        Edit
                      </button>
                      <button
                        disabled={busy}
                        onClick={() => {
                          if (confirm(`Delete "${item.name}"?`))
                            void action(async () => {
                              await api(
                                `/items/${item.id}`,
                                "DELETE",
                                undefined,
                                token,
                              );
                              await refresh();
                              resetItem();
                              setNotice("Item deleted.");
                            });
                        }}
                      >
                        Delete
                      </button>
                    </div>
                  </article>
                ))}
              </div>
              {selected > 0 && (
                <form
                  key={formVersion}
                  onSubmit={(e) => {
                    e.preventDefault();
                    void action(async () => {
                      let image = draft.image,
                        audioUrl = draft.audioUrl;
                      if (imageFile) {
                        const uploaded = await uploadFile(imageFile, token);
                        if (uploaded.kind !== "image")
                          throw new Error("Upload a PNG, JPEG, or WebP image.");
                        image = uploaded.url;
                      }
                      if (audioFile) {
                        const uploaded = await uploadFile(audioFile, token);
                        if (uploaded.kind !== "audio")
                          throw new Error("Upload an MP3, WAV, OGG, or M4A sound.");
                        audioUrl = uploaded.url;
                      }
                      await api(
                        itemId ? `/items/${itemId}` : "/items",
                        itemId ? "PUT" : "POST",
                        { ...draft, categoryId: selected, image, audioUrl },
                        token,
                      );
                      await refresh();
                      resetItem();
                      setNotice("Item saved.");
                    });
                  }}
                >
                  <h3>{itemId ? "Edit item" : "New item"}</h3>
                  <label>
                    Name
                    <input
                      required
                      maxLength={100}
                      value={draft.name}
                      onInput={(e) =>
                        setDraft({ ...draft, name: e.currentTarget.value })
                      }
                    />
                  </label>
                  <label>
                    Key (unique in category)
                    <input
                      required
                      pattern="[a-z0-9-]+"
                      maxLength={80}
                      value={draft.key}
                      onInput={(e) =>
                        setDraft({ ...draft, key: e.currentTarget.value })
                      }
                      placeholder="fire-engine"
                    />
                  </label>
                  <label>
                    Image (PNG, JPEG, WebP)
                    <input
                      type="file"
                      accept=".png,.jpg,.jpeg,.webp"
                      required={!draft.image}
                      onChange={(e) => setImageFile(e.currentTarget.files?.[0])}
                    />
                  </label>
                  <label>
                    Sound (MP3, WAV, OGG, M4A)
                    <input
                      type="file"
                      accept=".mp3,.wav,.ogg,.m4a"
                      required={!draft.audioUrl}
                      onChange={(e) => setAudioFile(e.currentTarget.files?.[0])}
                    />
                  </label>
                  <label>
                    Spoken fallback (optional)
                    <input
                      maxLength={500}
                      value={draft.sound}
                      onInput={(e) =>
                        setDraft({ ...draft, sound: e.currentTarget.value })
                      }
                    />
                  </label>
                  <label>
                    Card color
                    <input
                      type="color"
                      value={draft.color}
                      onInput={(e) =>
                        setDraft({ ...draft, color: e.currentTarget.value })
                      }
                    />
                  </label>
                  <button class="secondary" disabled={busy}>
                    {busy ? "Saving..." : "Save item"}
                  </button>
                  {itemId && (
                    <button type="button" disabled={busy} onClick={resetItem}>
                      Cancel edit
                    </button>
                  )}
                </form>
              )}
            </section>
          </div>
          <button
            class="secondary"
            disabled={busy}
            onClick={() => {
              setToken("");
              setAuthenticated(false);
            }}
          >
            Lock management
          </button>
        </>
      )}
      {error && (
        <p role="alert" class="admin-error">
          {error}
        </p>
      )}
      {notice && (
        <p role="status" class="admin-notice">
          {notice}
        </p>
      )}
    </main>
  );
}
