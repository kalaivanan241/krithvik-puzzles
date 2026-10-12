# Peekaboo Pals — Handover for Claude

Oct 12, 2026 · @Someone

## Purpose

Peekaboo Pals is a working memory-and-listening game for Kalai's young son, built for phone and tablet first. The child sees a few animal cards, the cards flip face down, the device speaks an animal sound, and he taps the card hiding that animal. It has 6 levels, growing from 3 cards to 8.

This handover lets a fresh Claude session pick up the game without the original conversation. Read it top to bottom, then use the ready-to-paste prompt at the end. Treat everything here as the current source of truth; the code itself lives in the Design canvas named below.

## How the game plays

Each round shows the cards, hides them, speaks one animal's sound, and waits for the child to tap that card; 3 correct finds clear a level.

&#91;embedded content: game loop · one round, repeated until 3 finds\]

Wrong taps never end the game: the card wiggles, the voice names it ("That is the dog."), it flips back and the sound repeats. After 2 misses in a round the right card starts bouncing as a hint. Every round reshuffles and the same animal is never asked twice in a row.

| Level | Cards |
| --- | --- |
| 1 | 3 |
| 2 | 4 |
| 3 | 5 |
| 4 | 6 |
| 5 | 7 |
| 6 | 8 |

**Screens.** Home (title, big Play button that jumps to the next unfinished level, and a row of 6 level buttons: finished = blue with a star, next = yellow and pulsing, later = locked) → Play → Level done (Home or Next level) → after level 6, a trophy screen with Play again. Finished levels can be replayed any time; progress is saved on the device.

## Current build

The game is one interactive artboard in a Claude Design canvas titled **Peekaboo Pals**: [open the canvas](https://claude.ai/code/artifact/37dde0be-d9c6-440b-8381-feb8b2c53adb). It launches in focused view, so opening the link shows the game, not the canvas grid.

| File | What it holds |
| --- | --- |
| `project/canvas.json` | Canvas index: one board `Main.dc.html`, 820 × 1180 px frame, `expand: fill`, `is_interactive: true`, launch = focused on `Main.dc.html` |
| `project/Main.dc.html` | The whole game: markup for 4 screens, helmet CSS (keyframes, 3D flip classes), and the `Component` logic class |

**Format.** The artboard is a Design Component (`.dc.html`): markup inside `<x-dc>`, `{{holes}}` that are dotted lookups only, `<sc-if>` / `<sc-for>` for branches and loops, `onClick="{{handler}}"` events, and a `class Component extends DCLogic` script whose `renderVals()` returns everything the template reads. UI must be markup, never built from script. Read the canvas type's `format.md` before editing.

**Animal pictures.** Eight flat SVG illustrations drawn for this game, uploaded as canvas assets. Reference them only by these exact URLs:

| Animal | Spoken sound | Asset URL |
| --- | --- | --- |
| Cat | Meow! Meow! | `/_blob/012179314641aa08be74fdfb83ef36a9` |
| Dog | Woof! Woof! | `/_blob/a86951a98226415cebb0aec2ebee9620` |
| Cow | Moo! Moooo! | `/_blob/246a3c698aad50b99aee5502810f8915` |
| Duck | Quack! Quack! | `/_blob/cf97b83d1d63c71fde266df0377f06eb` |
| Lion | Roar! Roooar! | `/_blob/0e7bda895e0ca0d7960e5482b0242763` |
| Frog | Ribbit! Ribbit! | `/_blob/7bbfee3732ba1357f64cd652876f1142` |
| Pig | Oink! Oink! | `/_blob/c2db347acca62615e72c25fe3a81aaf0` |
| Sheep | Baa! Baaa! | `/_blob/4ea7a5e90579fc2278f85dba9f19a33d` |

Each SVG is a 200 × 200 viewBox with a pastel background circle behind the animal, no `<style>` or animation.

**Sound.** There are no audio files. Animal sounds and prompts are spoken with the browser's `speechSynthesis` (English voice preferred in the order en-IN, en-GB, en-US, en-AU; rate 0.85, pitch 1.2). Correct, wrong and flip effects are short tones from the Web Audio API. The first speech call happens inside the Start tap so iOS unlocks audio.

**Layout.** Card size is computed in JS from the window size: it tries every column count from 2 to n, picks the one giving the largest square card that fits the width (max 1040 px) and the height left after the header (window height − 270 px), then clamps cards to 64–230 px. A resize listener keeps it live for rotation.

## Visual design

The look is "toy box": a warm cream ground with a soft dot pattern, chunky rounded buttons that sit on a solid drop shadow and press down 4 px when tapped, and one rounded display font. Keep new screens in this vocabulary.

**Font.** Baloo 2 (weights 600, 700, 800) from Google Fonts, falling back to Trebuchet MS, then system sans.

| Role | Color | Notes |
| --- | --- | --- |
| Page ground | `#FFF4D6` | Dot pattern `#F3D995`, 30 px grid |
| Text | `#22304A` | Secondary text `#4A5670` |
| Primary action (Play, Next level) | `#E2570F`, shadow `#A63D06` | White text, 30–44 px only, so contrast passes as large text |
| Secondary action / card backs | `#2F6FDB`, shadow `#1D4C9E` | White text; card backs carry a white dot pattern and a big "?" |
| Neutral buttons | `#FFFFFF`, shadow `#E8D6A6` | Home, card-count badge |
| Stars / current level | `#FFD43B`, shadow `#D9A80F` | Countdown bar uses `#F2A516` on `#EEDDB0` |
| Correct card ring | `#2E9E4F` | Plus a pop animation |
| Wrong card ring | `#E2570F` | Plus a shake animation, so color is never the only cue |

**Kid-friendly rules in force**

- Every touch target is at least 56 px; cards are at least 64 px; main buttons are 72–88 px tall.
- Icons are inline stroke SVGs; no emoji anywhere.
- No text the child must read to play: every instruction is also spoken.
- Wrong answers are gentle (wiggle, soft tone, "Try again!"), never a fail or game-over state.
- Motion: shake, bounce hint, pop, pulse on the next action, falling confetti on success. `prefers-reduced-motion` turns off the looping animations.
- `touch-action: manipulation` and no text selection, so fast taps don't zoom or highlight.

## Code reference

All logic is in the `Component` class at the bottom of `Main.dc.html`, with module-level constants above it: `ANIMALS` (key, name, sound, img), `MAX_LEVEL = 6`, `STORE_KEY = 'peekaboo-pals-cleared'`, `PRAISE` phrases and `CONFETTI_COLORS`.

**State**

| Field | Meaning |
| --- | --- |
| `screen` | `home` \| `play` \| `done` \| `win` |
| `level`, `round` | Current level (1–6) and round within it (0-based) |
| `phase` | `show` \| `hide` \| `find` \| `oops` \| `right` |
| `cards` | `[{id, key, up, wrong}]` for the current round |
| `target`, `lastTarget` | Animal key to find; last one, so the same animal isn't asked twice in a row |
| `progress` | 0–1 countdown during `show` |
| `misses` | Wrong taps this round; 2 or more turns on the bounce hint |
| `cleared` | Highest level finished, loaded from and saved to `localStorage` |
| `vw`, `vh` | Window size for the card layout |
| `confetti` | Generated confetti pieces for the current celebration |

**Key methods**

| Method | Does |
| --- | --- |
| `startLevel(level)` | Unlocks audio, speaks "Level N. Look carefully!", starts round 0 |
| `newRound(level, round)` | Picks `level + 2` random animals, shows them for `lookSeconds × 1000 + 600 ms per card`, flips them down, then after 900 ms picks a target and speaks the prompt |
| `tapCard(id)` | Ignored unless phase is `find`. Right: confetti, chime, praise, next round after 2.3 s. Wrong: flip up, shake, say "That is the dog.", flip back after 1.6 s and repeat the prompt |
| `finishLevel(level)` | Saves progress, then shows `done` or, after level 6, `win` |
| `promptFor(key, again)` | Speaks the sound, then "Where is the cow?" or "Who says that?" |
| `say(text)` / `tones(notes)` | Speech and Web Audio helpers, every call wrapped in try/catch |
| `later(fn, ms)` / `clearAll()` | Tracked timers, cleared on Home, on a new round and on unmount |
| `layout(n)` | Card size and grid width (see Current build) |

**Tweaks** (the canvas's Tweaks panel, declared in `data-props`)

| Prop | Default | Effect |
| --- | --- | --- |
| `sayName` | true | Off = sound only, "Who says that?" (harder mode) |
| `lookSeconds` | 3 (range 2–8) | Base memorise time; 0.6 s per card is added |
| `roundsPerLevel` | 3 (1–6) | Finds needed to clear a level |

## Limitations and next steps

The build has not been tested on a real phone or tablet yet; that is the first thing to do.

**Known limitations**

- Sounds are text-to-speech, not real animal recordings, and the voice varies by device.
- On iPhone/iPad the silent switch can mute both speech and tones.
- Progress is saved per browser in `localStorage`; it does not sync between devices.
- It runs inside the Claude Design canvas. To play it on the child's tablet, the canvas must be shared or the parent signed in on that device. There is no standalone URL, offline mode or home-screen install yet.
- Only 8 animals, so level 6 (8 cards) always uses the full set.

**Open questions for Kalai**

- [ ] Real animal sound recordings: will he supply audio files, or keep speech?
- [ ] Should the game move out of the canvas into a standalone PWA (installable, offline), possibly as a module of his planned kids learning app?
- [ ] More picture sets (vehicles, fruits, family members' photos) or more animals?
- [ ] Prompts in another language besides English?

**Suggested next steps, in order**

1. Play-test on his phone and tablet; fix any layout, speech or flip glitches.
2. Swap in real sounds if provided (upload as canvas assets, play with an `Audio` element, keep speech as the fallback).
3. Add more animals so higher levels vary more.
4. If he wants it standalone, port `Main.dc.html` to a single-file HTML/JS PWA with a manifest and service worker, keeping the same rules, colors and assets.

## Prompt for the next session

Paste this into a new Claude session together with a link to this doc.

```text
You are taking over "Peekaboo Pals", a kids' memory-and-listening game I'm building for my young son (phone and tablet first). The full handover is in the attached doc: read it end to end before doing anything.

The game lives in my Claude Design canvas: https://claude.ai/code/artifact/37dde0be-d9c6-440b-8381-feb8b2c53adb
It is one interactive artboard, project/Main.dc.html, plus project/canvas.json. Read both from the canvas before editing, and follow the canvas type's format rules (format.md).

Game rules to keep: show N animal cards, flip them face down, speak an animal sound, child taps the matching card. 6 levels, 3 to 8 cards (level + 2). 3 finds per level. Gentle wrong answers, bounce hint after 2 misses, progress saved per device.

Keep the existing look (Baloo 2, cream #FFF4D6 ground, orange #E2570F primary, blue #2F6FDB card backs, chunky press-down buttons, no emoji) and reuse the 8 animal asset URLs listed in the doc.

What I want next: [DESCRIBE THE CHANGE]

Change only what I ask, update the canvas in place, and tell me in a line or two what changed.
```
