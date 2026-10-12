export type Animal = {
  key: string;
  name: string;
  sound: string;
  color: string;
  image: string;
  audioUrl?: string;
};
const illustration = (animal: string, background: string) =>
  `https://coresg-normal.trae.ai/api/ide/v1/text_to_image?prompt=${encodeURIComponent(`Children's picture book illustration of one adorable smiling ${animal}, centered full body, simple flat vector style, thick soft rounded outlines, clean ${background} background, warm pastel colors, charming friendly face, no text, no letters, no border, high quality polished educational game artwork`)}&image_size=square`;

export const ANIMAL_ART: Record<string, Pick<Animal, "color" | "image">> = {
  cat: {
    color: "#ffe7b8",
    image: illustration("orange tabby kitten", "pale peach"),
  },
  dog: {
    color: "#e1ecff",
    image: illustration("golden brown puppy with floppy ears", "pale blue"),
  },
  cow: {
    color: "#f3e4fa",
    image: illustration("black and white baby cow", "pale lavender"),
  },
  duck: {
    color: "#fff0ba",
    image: illustration("yellow duckling", "pale yellow"),
  },
  lion: {
    color: "#ffe3cb",
    image: illustration("baby lion with a round fluffy mane", "pale peach"),
  },
  frog: {
    color: "#dcecd2",
    image: illustration("green frog with big eyes", "pale mint green"),
  },
  pig: { color: "#ffe0e5", image: illustration("pink piglet", "pale pink") },
  sheep: {
    color: "#e5edf6",
    image: illustration("fluffy white lamb", "pale sky blue"),
  },
};

export function parseAnimals(data: unknown): Animal[] {
  if (!Array.isArray(data)) throw new Error("The item catalog is invalid.");
  const seen = new Set<string>();
  return data.map((row) => {
    if (
      !row ||
      typeof row.key !== "string" ||
      typeof row.name !== "string" ||
      !row.name.trim() ||
      typeof row.sound !== "string" ||
      seen.has(row.key) ||
      (!row.image && !Object.hasOwn(ANIMAL_ART, row.key))
    ) {
      throw new Error("The animal catalog is invalid.");
    }
    seen.add(row.key);
    return {
      key: row.key,
      name: row.name,
      sound: row.sound,
      image: row.image || ANIMAL_ART[row.key]?.image,
      color: row.color || ANIMAL_ART[row.key]?.color || "#ffe7b8",
      audioUrl: row.audioUrl || "",
    };
  });
}

export async function loadAnimals(
  signal?: AbortSignal,
  categoryId?: number,
): Promise<Animal[]> {
  const response = await fetch(
    categoryId ? `/api/items?categoryId=${categoryId}` : "/api/animals",
    { signal },
  );
  if (!response.ok) throw new Error("Could not load your animal friends.");
  return parseAnimals(await response.json());
}

export const MAX_LEVEL = 6;
export const STORE_KEY = "peekaboo-pals-cleared";
export type Settings = {
  sayName: boolean;
  lookSeconds: number;
  roundsPerLevel: number;
};
export const DEFAULT_SETTINGS: Settings = {
  sayName: true,
  lookSeconds: 3,
  roundsPerLevel: 3,
};

export function shuffle<T>(items: readonly T[]): T[] {
  const result = [...items];
  for (let i = result.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [result[i], result[j]] = [result[j], result[i]];
  }
  return result;
}

export function makeRound(
  catalog: readonly Animal[],
  level: number,
  lastTarget: string | null,
) {
  if (catalog.length < level + 2)
    throw new Error("Not enough animals for this level.");
  const animals = shuffle(catalog).slice(
    0,
    Math.min(8, Math.max(3, level + 2)),
  );
  const candidates = animals.filter((a) => a.key !== lastTarget);
  if (candidates.length === 0)
    throw new Error("Not enough animals for this level.");
  return {
    animals,
    target: candidates[Math.floor(Math.random() * candidates.length)].key,
  };
}

export function progressKey(categoryId?: number): string {
  return categoryId === 1 || categoryId === undefined
    ? STORE_KEY
    : `${STORE_KEY}-category-${categoryId}`;
}

export function readProgress(categoryId?: number): number {
  try {
    const value = Number(localStorage.getItem(progressKey(categoryId)));
    return Number.isInteger(value)
      ? Math.min(MAX_LEVEL, Math.max(0, value))
      : 0;
  } catch {
    return 0;
  }
}

export function availableLevels(itemCount: number) {
  return Math.max(0, Math.min(MAX_LEVEL, itemCount - 2));
}

export function nextUnlockedLevel(cleared: number, levels: number) {
  return Math.max(1, Math.min(Math.max(levels, 0), cleared + 1));
}

export function lookDuration(lookSeconds: number, cardCount: number) {
  return lookSeconds * 1000 + cardCount * 600;
}

export function promptText(name: string, sayName: boolean, again = false) {
  const question = sayName
    ? `Where is the ${name.toLowerCase()}?`
    : "Who says that?";
  return `${again ? "Try again! " : ""}${question}`;
}

const PRAISE = [
  "You found it! Well done!",
  "Amazing! You did it!",
  "Great remembering!",
];

export function praise(round: number) {
  return PRAISE[round % PRAISE.length];
}

export function writeProgress(categoryId: number | undefined, cleared: number) {
  try {
    localStorage.setItem(progressKey(categoryId), String(cleared));
  } catch {
    return;
  }
}

export const SETTINGS_KEY = "peekaboo-pals-settings";

export function parseSettings(data: unknown): Settings | null {
  if (!data || typeof data !== "object") return null;
  const config = data as Record<string, unknown>;
  const { sayName, lookSeconds, roundsPerLevel } = config;
  if (
    typeof sayName !== "boolean" ||
    typeof lookSeconds !== "number" ||
    lookSeconds < 2 ||
    lookSeconds > 8 ||
    typeof roundsPerLevel !== "number" ||
    !Number.isInteger(roundsPerLevel) ||
    roundsPerLevel < 1 ||
    roundsPerLevel > 6
  ) {
    return null;
  }
  return { sayName, lookSeconds, roundsPerLevel };
}

export function hasStoredSettings() {
  try {
    return localStorage.getItem(SETTINGS_KEY) !== null;
  } catch {
    return false;
  }
}

export function loadSettings(): Settings {
  try {
    return (
      parseSettings(JSON.parse(localStorage.getItem(SETTINGS_KEY) ?? "")) ??
      DEFAULT_SETTINGS
    );
  } catch {
    return DEFAULT_SETTINGS;
  }
}

export function saveSettings(settings: Settings) {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
  } catch {
    return;
  }
}

export function isUploaded(url: string) {
  return url.startsWith("/uploads/");
}

export function previewPals(animals: readonly Animal[]): [Animal, Animal] {
  const fallback = (key: "cat" | "dog", name: string): Animal => ({
    key,
    name,
    sound: name,
    color: ANIMAL_ART[key].color,
    image: ANIMAL_ART[key].image,
  });
  const withArt = animals.filter((animal) => animal.image);
  return [
    withArt[0] ?? fallback("cat", "Cat"),
    withArt[1] ?? fallback("dog", "Dog"),
  ];
}

export function cardLayout(n: number, width: number, height: number) {
  const availableWidth = Math.min(1040, width - 40);
  const availableHeight = Math.max(180, height - 310);
  let best = { size: 64, columns: 2 };
  for (let columns = 2; columns <= n; columns++) {
    const size = Math.min(
      (availableWidth - (columns - 1) * 16) / columns,
      (availableHeight - (Math.ceil(n / columns) - 1) * 16) /
        Math.ceil(n / columns),
      230,
    );
    if (size > best.size) best = { size, columns };
  }
  return best;
}
