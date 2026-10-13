import { useEffect, useState } from "preact/hooks";
import {
  api,
  uploadMedia,
  type Category,
  type Item,
  type ItemData,
  type Saved,
} from "./catalog";
import { DEFAULT_CARD_COLOR, itemImage } from "./game";

const emptyItem = (categoryId: number): ItemData => ({
  categoryId,
  key: "",
  name: "",
  sound: "",
  image: "",
  audioUrl: "",
  color: DEFAULT_CARD_COLOR,
});

const itemData = ({ id: _id, speech: _speech, ...data }: Item): ItemData =>
  data;

function savedNotice(what: string, { speech }: Saved) {
  if (speech === "failed")
    return `${what} saved, but its voice clips could not be generated. Save again to retry.`;
  if (speech === "disabled")
    return `${what} saved. Voice clips are off until ELEVEN_LABS_API_KEY is set.`;
  return `${what} saved with voice clips.`;
}

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
  const [draft, setDraft] = useState<ItemData>(() => emptyItem(selected));
  const [imageFile, setImageFile] = useState<File>();
  const [audioFile, setAudioFile] = useState<File>();
  const [formVersion, setFormVersion] = useState(0);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);

  const loadItems = (id: number, signal?: AbortSignal) =>
    api<Item[]>(`/items?categoryId=${id}`, { signal });

  async function refresh(id = selected) {
    const list = await api<Category[]>("/categories");
    setCategories(list);
    const next = list.some((c) => c.id === id) ? id : (list[0]?.id ?? 0);
    if (next === selected) {
      setItems(next ? await loadItems(next) : []);
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
      void loadItems(selected, controller.signal)
        .then((rows) => {
          if (active) setItems(rows);
        })
        .catch((e: { name?: string; message?: string }) => {
          if (!active || e?.name === "AbortError") return;
          setError(e.message ?? "Could not load items");
        });
    resetItem();
    return () => {
      active = false;
      controller.abort();
    };
  }, [selected]);
  function editItem(id: number | null, data: ItemData) {
    setItemId(id);
    setDraft(data);
    setImageFile(undefined);
    setAudioFile(undefined);
    setFormVersion((v) => v + 1);
  }
  function resetItem() {
    editItem(null, emptyItem(selected));
  }
  function editCategory(id: number | null, name = "", description = "") {
    setCategoryId(id);
    setName(name);
    setDescription(description);
  }
  function resetCategory() {
    editCategory(null);
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
              await api("/admin/session", { token });
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
                        resetCategory();
                      }}
                    >
                      {c.name} ({c.itemCount})
                    </button>
                    <button
                      disabled={busy}
                      onClick={() =>
                        editCategory(c.id, c.name, c.description)
                      }
                    >
                      Edit
                    </button>
                    <button
                      disabled={busy}
                      onClick={() => {
                        if (confirm(`Delete "${c.name}" and all its items?`))
                          void action(async () => {
                            await api(`/categories/${c.id}`, {
                              method: "DELETE",
                              token,
                            });
                            await refresh();
                            resetCategory();
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
                    const saved = await api<Saved>(
                      categoryId ? `/categories/${categoryId}` : "/categories",
                      {
                        method: categoryId ? "PUT" : "POST",
                        body: { name, description },
                        token,
                      },
                    );
                    await refresh();
                    resetCategory();
                    setNotice(savedNotice("Category", saved));
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
                <div class="form-actions">
                  <button class="secondary" disabled={busy}>
                    Save category
                  </button>
                  {categoryId && (
                    <button
                      type="button"
                      class="ghost"
                      disabled={busy}
                      onClick={resetCategory}
                    >
                      Cancel edit
                    </button>
                  )}
                </div>
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
                    {itemImage(item) && (
                      <img src={itemImage(item)} alt={item.name} />
                    )}
                    <div>
                      <strong>{item.name}</strong>
                      <small>{item.key}</small>
                      {item.audioUrl ? (
                        <audio controls preload="none" src={item.audioUrl} />
                      ) : (
                        <small>
                          No recorded sound: upload audio to replace the
                          spoken fallback.
                        </small>
                      )}
                      <small>
                        {item.speech?.prompt
                          ? "Voice clips ready"
                          : "Voice clips missing: save again to generate"}
                      </small>
                    </div>
                    <div class="row-actions">
                      <button
                        disabled={busy}
                        onClick={() => editItem(item.id, itemData(item))}
                      >
                        Edit
                      </button>
                      <button
                        disabled={busy}
                        onClick={() => {
                          if (confirm(`Delete "${item.name}"?`))
                            void action(async () => {
                              await api(`/items/${item.id}`, {
                                method: "DELETE",
                                token,
                              });
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
                      const image = imageFile
                        ? await uploadMedia(imageFile, "image", token)
                        : draft.image;
                      const audioUrl = audioFile
                        ? await uploadMedia(audioFile, "audio", token)
                        : draft.audioUrl;
                      const saved = await api<Saved>(
                        itemId ? `/items/${itemId}` : "/items",
                        {
                          method: itemId ? "PUT" : "POST",
                          body: { ...draft, categoryId: selected, image, audioUrl },
                          token,
                        },
                      );
                      await refresh();
                      resetItem();
                      setNotice(savedNotice("Item", saved));
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
                  <div class="form-actions">
                    <button class="secondary" disabled={busy}>
                      {busy ? "Saving..." : "Save item"}
                    </button>
                    {itemId && (
                      <button
                        type="button"
                        class="ghost"
                        disabled={busy}
                        onClick={resetItem}
                      >
                        Cancel edit
                      </button>
                    )}
                  </div>
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
