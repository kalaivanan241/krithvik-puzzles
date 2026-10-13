import { existsSync } from "node:fs";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  ANIMAL_ART,
  availableLevels,
  cardLayout,
  loadAnimals,
  makeRound,
  nextUnlockedLevel,
  parseAnimals,
  parseSettings,
  levelCompletePhrase,
  levelStartPhrase,
  MAX_LEVEL,
  parseSpeech,
  praisePhrase,
  promptText,
  shuffle,
  soundOnlyPhrase,
} from "./game";
import phrases from "./phrases.json";

const rows = Object.keys(ANIMAL_ART).map((key) => ({
  key,
  name: `DB ${key}`,
  sound: `DB ${key} sound`,
}));
const animalsCatalog = parseAnimals(rows);

afterEach(() => vi.unstubAllGlobals());

describe("database catalog loading", () => {
  it("fetches names and sounds from the backend", async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(new Response(JSON.stringify(rows)));
    vi.stubGlobal("fetch", fetchMock);
    const animals = await loadAnimals();
    expect(fetchMock).toHaveBeenCalledWith("/api/animals", {
      signal: undefined,
    });
    expect(animals[0].name).toBe("DB cat");
    expect(animals[0].sound).toBe("DB cat sound");
    expect(animals[0].image).toBe(ANIMAL_ART.cat.image);
  });
  it("reports API failures instead of falling back to hardcoded animals", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(new Response("", { status: 500 })),
    );
    await expect(loadAnimals()).rejects.toThrow("Could not load");
  });
  it("accepts smaller categories and uploaded custom items", () => {
    expect(parseAnimals(rows.slice(0, 3))).toHaveLength(3);
    expect(
      parseAnimals([
        {
          key: "car",
          name: "Car",
          sound: "",
          image: "/uploads/car.png",
          audioUrl: "/uploads/car.wav",
        },
      ])[0].audioUrl,
    ).toBe("/uploads/car.wav");
    expect(parseAnimals([])).toEqual([]);
  });
  it("rejects duplicated, missing-artwork, and malformed catalogs", () => {
    expect(() => parseAnimals([...rows.slice(0, 7), rows[0]])).toThrow();
    expect(() =>
      parseAnimals([{ ...rows[0], key: "unknown" }, ...rows.slice(1)]),
    ).toThrow();
    expect(() =>
      parseAnimals([{ ...rows[0], name: "" }, ...rows.slice(1)]),
    ).toThrow();
  });
});

describe("round generation", () => {
  it("uses 3 to 8 unique cards across the six levels", () => {
    for (let level = 1; level <= 6; level++) {
      const { animals, target } = makeRound(animalsCatalog, level, null);
      expect(animals).toHaveLength(level + 2);
      expect(new Set(animals.map((a) => a.key)).size).toBe(level + 2);
      expect(animals.some((a) => a.key === target)).toBe(true);
    }
  });
  it("never asks for the same animal in consecutive rounds", () => {
    let previous: string | null = null;
    for (let i = 0; i < 200; i++) {
      const { target } = makeRound(animalsCatalog, (i % 6) + 1, previous);
      expect(target).not.toBe(previous);
      previous = target;
    }
  });
  it("does not mutate the animal catalog", () => {
    const before = animalsCatalog.map((a) => a.key);
    shuffle(animalsCatalog);
    expect(animalsCatalog.map((a) => a.key)).toEqual(before);
  });
});

describe("levels and prompts", () => {
  it("unlocks a level for every animal after the first two, up to six", () => {
    expect(availableLevels(2)).toBe(0);
    expect(availableLevels(3)).toBe(1);
    expect(availableLevels(8)).toBe(6);
    expect(availableLevels(12)).toBe(6);
    expect(nextUnlockedLevel(0, 6)).toBe(1);
    expect(nextUnlockedLevel(6, 6)).toBe(6);
    expect(nextUnlockedLevel(4, 2)).toBe(2);
  });
  it("speaks the same question the child can see", () => {
    expect(promptText("Cow", true)).toBe("Where is the cow?");
    expect(promptText("Cow", true, true)).toBe("Try again! Where is the cow?");
    expect(promptText("Cow", false)).toBe("Who says that?");
    expect(phrases[praisePhrase(0)]).toBe("You found it! Well done!");
    expect(praisePhrase(3)).toBe(praisePhrase(0));
    expect(phrases[soundOnlyPhrase()]).toBe(promptText("Cow", false));
    expect(phrases[soundOnlyPhrase(true)]).toBe(promptText("Cow", false, true));
  });
  it("has a pre-generated clip for every fixed phrase the game plays", () => {
    const used = [
      "look-carefully",
      "win",
      soundOnlyPhrase(),
      soundOnlyPhrase(true),
      ...Array.from({ length: MAX_LEVEL }, (_, i) => levelStartPhrase(i + 1)),
      // The last level always ends in the win celebration instead.
      ...Array.from({ length: MAX_LEVEL - 1 }, (_, i) => levelCompletePhrase(i + 1)),
      ...[0, 1, 2].map(praisePhrase),
    ];
    for (const id of used) {
      expect(phrases).toHaveProperty(id);
      expect(existsSync(`public/speech/${id}.mp3`), id).toBe(true);
    }
  });
  it("keeps only uploaded clip URLs for item speech", () => {
    expect(
      parseSpeech({
        prompt: "/uploads/a.mp3",
        retry: "https://example.com/x.mp3",
        reveal: 3,
        extra: "/uploads/b.mp3",
      }),
    ).toEqual({ prompt: "/uploads/a.mp3" });
    expect(parseSpeech(null)).toEqual({});
  });
  it("accepts only parent settings inside the game limits", () => {
    expect(
      parseSettings({ sayName: false, lookSeconds: 8, roundsPerLevel: 1 }),
    ).toEqual({ sayName: false, lookSeconds: 8, roundsPerLevel: 1 });
    expect(parseSettings({ sayName: true, lookSeconds: 1, roundsPerLevel: 3 })).toBeNull();
    expect(parseSettings({ sayName: true, lookSeconds: 3, roundsPerLevel: 1.5 })).toBeNull();
    expect(parseSettings(null)).toBeNull();
  });
});

describe("responsive card layout", () => {
  for (const [width, height] of [
    [375, 667],
    [390, 844],
    [768, 1024],
    [1024, 768],
    [1440, 900],
  ]) {
    it(`fits eight cards in ${width} by ${height}`, () => {
      const { columns, size } = cardLayout(8, width, height);
      expect(size).toBeGreaterThanOrEqual(64);
      expect(size).toBeLessThanOrEqual(230);
      expect(columns * size + (columns - 1) * 16).toBeLessThanOrEqual(
        Math.min(1040, width - 40),
      );
    });
  }
});
