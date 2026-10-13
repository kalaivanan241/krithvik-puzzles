import { useEffect, useRef, useState } from "preact/hooks";
import {
  phraseUrl,
  playClip,
  playSequence,
  sayPhrase,
  stopAudio,
  tones,
  unlockAudio,
} from "./audio";
import { api, type Category } from "./catalog";
import {
  availableLevels,
  levelCompletePhrase,
  levelStartPhrase,
  lookDuration,
  loadAnimals,
  makeRound,
  praisePhrase,
  soundOnlyPhrase,
  writeProgress,
  type Animal,
  type Settings,
} from "./game";

export type Screen = "home" | "play" | "done" | "win" | "manage";
export type Phase = "show" | "hide" | "find" | "oops" | "right";
export type Card = Animal & { up: boolean; wrong: boolean };
export type CatalogStatus = "loading" | "ready" | "error";

export function useCatalog(attempt: number) {
  const [categoryId, setCategoryId] = useState(0);
  const [categories, setCategories] = useState<Category[]>([]);
  const [animals, setAnimals] = useState<Animal[]>([]);
  const [status, setStatus] = useState<CatalogStatus>("loading");

  useEffect(() => {
    const controller = new AbortController();
    let timedOut = false;
    const timeout = setTimeout(() => {
      timedOut = true;
      controller.abort();
    }, 10000);
    let active = true;
    setStatus("loading");
    void api<Category[]>("/categories", { signal: controller.signal })
      .then(async (list) => {
        if (!active) return null;
        setCategories(list);
        const selected = list.some((c) => c.id === categoryId)
          ? categoryId
          : (list[0]?.id ?? 0);
        if (selected !== categoryId) {
          setCategoryId(selected);
          return null;
        }
        if (!selected) return [];
        return loadAnimals(controller.signal, selected);
      })
      .then((catalog) => {
        if (!catalog || !active) return;
        setAnimals(catalog);
        setStatus("ready");
      })
      .catch((error: { name?: string }) => {
        if (!active || (error?.name === "AbortError" && !timedOut)) return;
        setStatus("error");
      })
      .finally(() => clearTimeout(timeout));
    return () => {
      active = false;
      clearTimeout(timeout);
      controller.abort();
    };
  }, [attempt, categoryId]);

  function chooseCategory(id: number) {
    setAnimals([]);
    setStatus("loading");
    setCategoryId(id);
  }

  return { categoryId, categories, animals, status, chooseCategory };
}

export function useSession(input: {
  animals: Animal[];
  settings: Settings;
  categoryId: number;
  catalogReady: boolean;
  cleared: number;
  setCleared: (value: number) => void;
  setScreen: (screen: "home" | "play" | "done" | "win") => void;
}) {
  const {
    animals,
    settings,
    categoryId,
    catalogReady,
    cleared,
    setCleared,
    setScreen,
  } = input;
  const [level, setLevel] = useState(1);
  const [round, setRound] = useState(0);
  const [phase, setPhase] = useState<Phase>("show");
  const [cards, setCards] = useState<Card[]>([]);
  const [target, setTarget] = useState("");
  const [misses, setMisses] = useState(0);
  const [progress, setProgress] = useState(1);
  const timers = useRef<ReturnType<typeof setTimeout>[]>([]);
  const lastTarget = useRef<string | null>(null);
  const tapLock = useRef(false);

  const later = (fn: () => void, ms: number) => {
    timers.current.push(setTimeout(fn, ms));
  };
  function cancelPending() {
    timers.current.forEach(clearTimeout);
    timers.current = [];
    stopAudio();
  }
  function clearAll() {
    cancelPending();
    tapLock.current = true;
  }
  useEffect(() => cancelPending, []);
  useEffect(() => {
    lastTarget.current = null;
  }, [categoryId]);

  function updateCards(update: (card: Card) => Card, key?: string) {
    setCards((current) =>
      current.map((c) => (key === undefined || c.key === key ? update(c) : c)),
    );
  }

  function prompt(key: string, again = false) {
    const animal = animals.find((a) => a.key === key);
    if (!animal) return;
    // The uploaded sound (or its spoken fallback), then the question. A missing
    // named prompt falls back to the fixed sound-only question.
    const named = again ? animal.speech.retry : animal.speech.prompt;
    playSequence([
      [animal.audioUrl, animal.speech.sound],
      [settings.sayName ? named : undefined, phraseUrl(soundOnlyPhrase(again))],
    ]);
  }

  function newRound(nextLevel: number, nextRound: number) {
    clearAll();
    const next = makeRound(animals, nextLevel, lastTarget.current);
    lastTarget.current = next.target;
    setRound(nextRound);
    setTarget(next.target);
    setCards(next.animals.map((a) => ({ ...a, up: true, wrong: false })));
    setMisses(0);
    setPhase("show");
    setProgress(1);
    const duration = lookDuration(settings.lookSeconds, next.animals.length);
    const start = Date.now();
    const tick = () => {
      setProgress(Math.max(0, 1 - (Date.now() - start) / duration));
      if (Date.now() - start < duration) later(tick, 50);
    };
    later(tick, 50);
    later(() => {
      setPhase("hide");
      updateCards((c) => ({ ...c, up: false }));
      tones("flip");
      later(() => {
        setPhase("find");
        tapLock.current = false;
        prompt(next.target);
      }, 900);
    }, duration);
  }

  function startLevel(nextLevel: number) {
    if (!catalogReady || animals.length < nextLevel + 2) return;
    clearAll();
    unlockAudio();
    setLevel(nextLevel);
    setScreen("play");
    newRound(nextLevel, 0);
    sayPhrase(levelStartPhrase(nextLevel));
  }

  function finishLevel() {
    clearAll();
    const nextCleared = Math.max(cleared, level);
    setCleared(nextCleared);
    writeProgress(categoryId, nextCleared);
    const won = level === availableLevels(animals.length);
    setScreen(won ? "win" : "done");
    sayPhrase(won ? "win" : levelCompletePhrase(level));
  }

  function tapCard(key: string) {
    if (phase !== "find" || tapLock.current) return;
    tapLock.current = true;
    stopAudio();
    if (key === target) {
      setPhase("right");
      updateCards((c) => ({ ...c, up: true }), key);
      tones("right");
      sayPhrase(praisePhrase(round));
      later(() => {
        if (round + 1 >= settings.roundsPerLevel) finishLevel();
        else {
          newRound(level, round + 1);
          sayPhrase("look-carefully");
        }
      }, 2300);
    } else {
      const named = animals.find((a) => a.key === key);
      setPhase("oops");
      setMisses((value) => value + 1);
      updateCards((c) => ({ ...c, up: true, wrong: true }), key);
      tones("wrong");
      playClip(named?.speech.reveal);
      later(() => {
        updateCards((c) => ({ ...c, up: false, wrong: false }));
        setPhase("find");
        tapLock.current = false;
        prompt(target, true);
      }, 1600);
    }
  }

  function home() {
    clearAll();
    setScreen("home");
  }

  return {
    level,
    round,
    phase,
    cards,
    target,
    misses,
    progress,
    startLevel,
    tapCard,
    home,
    prompt,
    clearAll,
  };
}
