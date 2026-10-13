// Generates the game's fixed voice clips with ElevenLabs into public/speech.
// Usage: npm run speech            (only new or changed phrases)
//        npm run speech -- --force (regenerate everything)
// Reads ELEVEN_LABS_API_KEY (and optional ELEVEN_LABS_VOICE_ID/MODEL_ID) from .env.
// Keep voice settings in sync with backend/src/speech.rs.
import { existsSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";

const root = new URL("..", import.meta.url);
const outputDir = new URL("public/speech/", root);
const manifestFile = new URL("manifest.json", outputDir);

const key = process.env.ELEVEN_LABS_API_KEY?.trim();
const voice = process.env.ELEVEN_LABS_VOICE_ID?.trim() || "XrExE9yKIg1WjnnlVkGX";
const model = process.env.ELEVEN_LABS_MODEL_ID?.trim() || "eleven_multilingual_v2";
const force = process.argv.includes("--force");

if (!key) {
  console.error("Set ELEVEN_LABS_API_KEY in .env to generate voice clips.");
  process.exit(1);
}

const phrases = JSON.parse(await readFile(new URL("src/phrases.json", root), "utf8"));
const manifest = existsSync(manifestFile)
  ? JSON.parse(await readFile(manifestFile, "utf8"))
  : {};
await mkdir(outputDir, { recursive: true });

async function synthesize(text) {
  const response = await fetch(
    `https://api.elevenlabs.io/v1/text-to-speech/${encodeURIComponent(voice)}?output_format=mp3_44100_128`,
    {
      method: "POST",
      headers: { "xi-api-key": key, "Content-Type": "application/json", Accept: "audio/mpeg" },
      body: JSON.stringify({
        text,
        model_id: model,
        voice_settings: { stability: 0.5, similarity_boost: 0.75, speed: 0.9 },
      }),
    },
  );
  if (!response.ok) throw new Error(`ElevenLabs returned ${response.status}`);
  const audio = Buffer.from(await response.arrayBuffer());
  const isMp3 = audio.subarray(0, 3).toString() === "ID3" || (audio[0] === 0xff && (audio[1] & 0xe0) === 0xe0);
  if (!isMp3) throw new Error("ElevenLabs did not return MP3 audio");
  return audio;
}

let generated = 0;
for (const [id, text] of Object.entries(phrases)) {
  const file = new URL(`${id}.mp3`, outputDir);
  if (!force && manifest[id] === text && existsSync(file)) continue;
  process.stdout.write(`${id}: ${text}\n`);
  await writeFile(file, await synthesize(text));
  manifest[id] = text;
  generated++;
}
for (const id of Object.keys(manifest)) if (!(id in phrases)) delete manifest[id];
await writeFile(manifestFile, `${JSON.stringify(manifest, Object.keys(manifest).sort(), 2)}\n`);
console.log(`Generated ${generated} clip(s); ${Object.keys(phrases).length} phrases total.`);
