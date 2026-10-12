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
let recording: HTMLAudioElement | undefined;
let playbackVersion = 0;

export function stopAudio() {
  playbackVersion++;
  if (recording) {
    recording.onended = null;
    recording.onerror = null;
    recording.pause();
    recording = undefined;
  }
  cancelSpeech();
}

export function playSound(url: string, fallback: string, question: string) {
  stopAudio();
  if (!enabled) return;
  if (!url) {
    say(`${fallback} ${question}`);
    return;
  }
  const version = playbackVersion;
  const audio = new Audio(url);
  recording = audio;
  audio.onended = () => {
    if (version === playbackVersion) {
      recording = undefined;
      say(question);
    }
  };
  const failed = () => {
    if (version !== playbackVersion) return;
    stopAudio();
    say(`${fallback} ${question}`);
  };
  audio.onerror = failed;
  void audio.play().catch(failed);
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

export function cancelSpeech() {
  try {
    window.speechSynthesis?.cancel();
  } catch {
    /* Unsupported browser. */
  }
}

export function say(text: string) {
  if (!enabled) return;
  try {
    cancelSpeech();
    const utterance = new SpeechSynthesisUtterance(text);
    const voices = speechSynthesis.getVoices();
    for (const language of ["en-IN", "en-GB", "en-US", "en-AU"]) {
      const voice = voices.find((v) => v.lang === language);
      if (voice) {
        utterance.voice = voice;
        break;
      }
    }
    utterance.lang = utterance.voice?.lang ?? "en-US";
    utterance.rate = 0.85;
    utterance.pitch = 1.2;
    speechSynthesis.speak(utterance);
  } catch {
    /* Visual instructions are always available too. */
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
