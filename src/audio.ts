import type phrases from "./phrases.json";

let context: AudioContext | undefined;
const SOUND_KEY = "peekaboo-pals-sound";

function storedSound() {
  try {
    return localStorage.getItem(SOUND_KEY) !== "0";
  } catch {
    return true;
  }
}

let enabled = storedSound();

export function soundEnabled() {
  return enabled;
}
let current: HTMLAudioElement | undefined;
let playbackVersion = 0;

export type PhraseId = keyof typeof phrases;
/** A fixed game phrase, pre-generated into public/speech by `npm run speech`. */
export const phraseUrl = (id: PhraseId) => `/speech/${id}.mp3`;

export function stopAudio() {
  playbackVersion++;
  if (current) {
    current.onended = null;
    current.onerror = null;
    current.pause();
    current = undefined;
  }
}

/**
 * Plays steps in order. Each step lists candidate clip URLs; the first one
 * that plays is used, and missing or failing candidates fall through.
 */
export function playSequence(steps: (string | undefined)[][]) {
  stopAudio();
  if (!enabled) return;
  const version = playbackVersion;
  const queue = steps
    .map((step) => step.filter((url): url is string => Boolean(url)))
    .filter((step) => step.length > 0);
  const play = (step: number, candidate: number) => {
    if (version !== playbackVersion) return;
    if (step >= queue.length) {
      current = undefined;
      return;
    }
    const url = queue[step][candidate];
    if (url === undefined) return play(step + 1, 0);
    const audio = new Audio(url);
    current = audio;
    let settled = false;
    const advance = (nextStep: number, nextCandidate: number) => {
      if (settled) return;
      settled = true;
      play(nextStep, nextCandidate);
    };
    audio.onended = () => advance(step + 1, 0);
    audio.onerror = () => advance(step, candidate + 1);
    void audio.play().catch(() => advance(step, candidate + 1));
  };
  play(0, 0);
}

export function playClip(url: string | undefined) {
  playSequence([[url]]);
}

export function sayPhrase(id: PhraseId) {
  playClip(phraseUrl(id));
}

export function setSound(value: boolean) {
  enabled = value;
  try {
    localStorage.setItem(SOUND_KEY, value ? "1" : "0");
  } catch {
    void 0;
  }
  if (!value) stopAudio();
}

export function unlockAudio() {
  try {
    context ??= new AudioContext();
    void context.resume();
  } catch {
    /* The game remains playable without audio support. */
  }
}

export function tones(kind: "right" | "wrong" | "flip") {
  if (!enabled) return;
  try {
    unlockAudio();
    if (!context) return;
    const notes =
      kind === "right"
        ? [523, 659, 784]
        : kind === "wrong"
          ? [294, 262]
          : [440];
    notes.forEach((frequency, i) => {
      const oscillator = context!.createOscillator();
      const gain = context!.createGain();
      const start = context!.currentTime + i * 0.12;
      oscillator.type = "sine";
      oscillator.frequency.value = frequency;
      gain.gain.setValueAtTime(0, start);
      gain.gain.linearRampToValueAtTime(0.12, start + 0.015);
      gain.gain.exponentialRampToValueAtTime(0.001, start + 0.22);
      oscillator.connect(gain);
      gain.connect(context!.destination);
      oscillator.start(start);
      oscillator.stop(start + 0.24);
    });
  } catch {
    /* Audio is optional. */
  }
}
