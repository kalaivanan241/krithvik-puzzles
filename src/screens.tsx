import {
  ArrowRight,
  Check,
  ChevronRight,
  Ear,
  Heart,
  House,
  LockKeyhole,
  Play,
  RotateCcw,
  Sparkles,
  Star,
  Trophy,
  Volume2,
  X,
} from "lucide-preact";
import { useEffect, useRef, useState } from "preact/hooks";
import type { Category } from "./catalog";
import {
  cardLayout,
  isUploaded,
  MAX_LEVEL,
  previewPals,
  type Animal,
  type Settings,
} from "./game";
import type { Card, CatalogStatus, Phase } from "./session";

export function Confetti() {
  return (
    <div class="confetti" aria-hidden="true">
      {Array.from({ length: 32 }, (_, i) => (
        <span
          key={i}
          style={{
            left: `${(i * 37) % 100}%`,
            background: ["#FFD43B", "#E2570F", "#2F6FDB", "#75B898", "#D5A4DD"][
              i % 5
            ],
            animationDelay: `${(i % 8) * 0.12}s`,
            transform: `rotate(${i * 23}deg)`,
          }}
        />
      ))}
    </div>
  );
}

export function HomeScreen(props: {
  categories: Category[];
  categoryId: number;
  catalogStatus: CatalogStatus;
  animals: Animal[];
  cleared: number;
  available: number;
  nextLevel: number;
  onChooseCategory: (id: number) => void;
  onManage: () => void;
  onPlay: (level: number) => void;
  onRetry: () => void;
}) {
  const {
    categories,
    categoryId,
    catalogStatus,
    animals,
    cleared,
    available,
    nextLevel,
    onChooseCategory,
    onManage,
    onPlay,
    onRetry,
  } = props;
  const [left, right] = previewPals(animals);
  return (
    <main class="home-screen">
      <section class="category-picker">
        <label>
          Choose your pals
          <select
            value={categoryId}
            disabled={catalogStatus === "loading"}
            onChange={(e) => onChooseCategory(Number(e.currentTarget.value))}
          >
            {categories.map((c) => (
              <option value={c.id} key={c.id}>
                {c.name} ({c.itemCount} items)
              </option>
            ))}
          </select>
        </label>
        <button class="secondary" onClick={onManage}>
          Manage library
        </button>
      </section>
      <section class="hero">
        <div class="hero-copy">
          <div class="eyebrow">
            <Sparkles size={16} /> A LITTLE GAME FOR GROWING MINDS
          </div>
          <h1>
            Now you see them.
            <br />
            <span>Now you don’t!</span>
            <span class="title-spark">
              <Sparkles />
            </span>
          </h1>
          <p class="hero-description">
            Little ears. Sharp eyes. Happy surprises.
            <br />
            Find your animal friends in a playful game
            <br class="desktop-break" /> of listening and remembering.
          </p>
          <button
            class="primary play-button"
            disabled={catalogStatus !== "ready" || available === 0}
            onClick={() => onPlay(nextLevel)}
          >
            <Play size={25} fill="currentColor" /> Let’s play{" "}
            <ArrowRight size={25} />
          </button>
          {catalogStatus === "ready" && available === 0 && (
            <p role="status">
              Add at least 3 items in management to play this category.
            </p>
          )}
          {catalogStatus === "loading" && (
            <p role="status">Loading your animal friends...</p>
          )}
          {catalogStatus === "error" && (
            <div role="alert">
              <p>
                Could not load your friends. Check that the backend is running.
              </p>
              <button class="secondary" onClick={onRetry}>
                Try again
              </button>
            </div>
          )}
          <div class="play-note">
            <span class="tiny-star">
              <Star size={14} fill="currentColor" />
            </span>{" "}
            {cleared > 0
              ? `Pick up at level ${nextLevel}`
              : "Start with 3 friendly faces"}{" "}
            <span class="note-dot" /> No rush, just fun
          </div>
        </div>
        <div class="hero-art" aria-label="A peek at your animal pals">
          <div class="orbit orbit-one" />
          <div class="orbit orbit-two" />
          <Star class="art-star star-one" fill="currentColor" />
          <Star class="art-star star-two" fill="currentColor" />
          <span class="art-dot dot-one" />
          <span class="art-dot dot-two" />
          <div
            class={`preview-card preview-cat ${isUploaded(left.image) ? "is-upload" : ""}`}
          >
            <img src={left.image} alt={`A friendly ${left.name.toLowerCase()}`} />
            <span>{left.sound || left.name}</span>
          </div>
          <div
            class={`preview-card preview-dog ${isUploaded(right.image) ? "is-upload" : ""}`}
          >
            <img
              src={right.image}
              alt={`A friendly ${right.name.toLowerCase()}`}
            />
            <span>{right.sound || right.name}</span>
          </div>
          <div class="preview-card preview-back">
            <span class="back-question">?</span>
            <span class="back-mini">peekaboo!</span>
          </div>
          <div class="peek-sticker">
            <Ear size={21} />
            <span>Who’s hiding?</span>
          </div>
          <div class="art-caption">Your new friends are waiting!</div>
          <svg
            class="sketch-arrow"
            viewBox="0 0 70 60"
            fill="none"
            aria-hidden="true"
          >
            <path
              d="M5 5C56 0 66 26 37 49M37 49l3-16M37 49l18-1"
              stroke="currentColor"
              stroke-width="2.5"
              stroke-linecap="round"
            />
          </svg>
        </div>
      </section>

      <section class="how-section" aria-label="How to play">
        <div class="section-caption">
          A LITTLE LOOK. A LITTLE LISTEN. A BIG “I DID IT!”
        </div>
        <div class="steps">
          <div class="step">
            <span class="step-number peach">1</span>
            <div>
              <h3>Meet your pals</h3>
              <p>Take a good look at the cards.</p>
            </div>
          </div>
          <ChevronRight class="step-arrow" size={21} />
          <div class="step">
            <span class="step-number lavender">2</span>
            <div>
              <h3>Listen closely</h3>
              <p>Who says “moo” or “meow”?</p>
            </div>
          </div>
          <ChevronRight class="step-arrow" size={21} />
          <div class="step">
            <span class="step-number mint">3</span>
            <div>
              <h3>Find your friend</h3>
              <p>Tap their card. Hello again!</p>
            </div>
          </div>
        </div>
      </section>

      <section class="levels-section">
        <div class="levels-heading">
          <div>
            <h2>
              Your little adventure <span class="heading-line" />
            </h2>
            <p>Six small steps. So many happy discoveries.</p>
          </div>
          <span class="progress-pill">
            <Star size={16} fill="currentColor" /> {cleared} of {MAX_LEVEL} complete
          </span>
        </div>
        <div class="level-grid">
          {Array.from({ length: MAX_LEVEL }, (_, i) => i + 1).map((n) => {
            const complete = n <= cleared;
            const current = n === nextLevel;
            const locked = n > nextLevel || n > available;
            return (
              <button
                key={n}
                class={`level-button ${complete ? "complete" : current ? "current" : "locked"}`}
                disabled={locked || catalogStatus !== "ready"}
                onClick={() => onPlay(n)}
                aria-label={`Level ${n}, ${n + 2} cards${complete ? ", completed" : locked ? ", locked" : ", next level"}`}
              >
                {current && <span class="start-tag">START HERE</span>}
                <span class="level-symbol">
                  {complete ? (
                    <Star size={28} fill="currentColor" />
                  ) : locked ? (
                    <LockKeyhole size={24} />
                  ) : (
                    <Play size={27} fill="currentColor" />
                  )}
                </span>
                <span class="level-name">Level {n}</span>
                <span class="level-count">{n + 2} cards</span>
                <span class="level-bottom">
                  {complete ? (
                    <Check size={15} />
                  ) : current ? (
                    <ArrowRight size={16} />
                  ) : (
                    <span class="lock-dot" />
                  )}
                </span>
              </button>
            );
          })}
        </div>
        <div class="reassurance">
          <Heart size={16} />
          <span>A wrong guess? Just another chance to say hello.</span>
        </div>
      </section>
    </main>
  );
}

export function PlayScreen(props: {
  level: number;
  round: number;
  phase: Phase;
  cards: Card[];
  misses: number;
  progress: number;
  settings: Settings;
  target: string;
  animals: Animal[];
  onTap: (key: string) => void;
  onReplay: () => void;
}) {
  const {
    level,
    round,
    phase,
    cards,
    misses,
    progress,
    settings,
    target,
    animals,
    onTap,
    onReplay,
  } = props;
  const [size, setSize] = useState({ width: innerWidth, height: innerHeight });
  useEffect(() => {
    const resize = () => setSize({ width: innerWidth, height: innerHeight });
    window.addEventListener("resize", resize);
    return () => window.removeEventListener("resize", resize);
  }, []);
  const layout = cardLayout(cards.length, size.width, size.height);
  const targetAnimal = animals.find((a) => a.key === target);
  return (
    <main class="game-screen">
      <div class="game-heading">
        <span class="eyebrow">LET’S FIND YOUR FRIENDS</span>
        <h1>
          Level {level} <span class="card-count">{level + 2} cards</span>
        </h1>
        <div
          class="round-stars"
          aria-label={`${round} of ${settings.roundsPerLevel} finds complete`}
        >
          {Array.from({ length: settings.roundsPerLevel }, (_, i) => (
            <Star
              key={i}
              size={30}
              fill={
                i < round || (i === round && phase === "right")
                  ? "#FFD43B"
                  : "none"
              }
            />
          ))}
        </div>
      </div>
      <div class="game-instruction" aria-live="polite">
        <h2>
          {phase === "show"
            ? "Take a little look…"
            : phase === "hide"
              ? "Where did they go?"
              : phase === "right"
                ? "You found your friend!"
                : phase === "oops"
                  ? "A little surprise! Try again."
                  : settings.sayName
                    ? `Where is the ${targetAnimal?.name.toLowerCase()}?`
                    : "Who says that?"}
        </h2>
        {phase === "show" ? (
          <div class="countdown">
            <span style={{ transform: `scaleX(${progress})` }} />
          </div>
        ) : (
          <button
            class="listen-button"
            onClick={onReplay}
            disabled={phase !== "find"}
          >
            <Volume2 size={19} />{" "}
            {phase === "find"
              ? `${targetAnimal?.sound} Listen again`
              : phase === "right"
                ? "Great remembering!"
                : "Little ears, listen closely"}
          </button>
        )}
      </div>
      <div
        class="card-grid"
        style={{
          gridTemplateColumns: `repeat(${layout.columns}, ${layout.size}px)`,
        }}
      >
        {cards.map((card, index) => (
          <button
            key={card.key}
            style={{ width: layout.size, height: layout.size }}
            class={`animal-card ${card.up ? "is-up" : ""} ${card.wrong ? "is-wrong" : ""} ${phase === "right" && card.key === target ? "is-correct" : ""} ${phase === "find" && misses >= 2 && card.key === target ? "is-hint" : ""} ${isUploaded(card.image) ? "is-upload" : ""}`}
            onClick={() => onTap(card.key)}
            disabled={phase !== "find"}
            aria-label={
              card.up ? card.name : `Hidden animal card ${index + 1}`
            }
          >
            <span class="card-inner">
              <span class="card-face card-back">
                <span>?</span>
                <small>peekaboo!</small>
              </span>
              <span class="card-face card-front" style={{ background: card.color }}>
                <img src={card.image} alt="" />
                <span>{card.name}</span>
              </span>
            </span>
          </button>
        ))}
      </div>
      <p class="game-bottom">
        {phase === "show"
          ? "Remember where each friend is hiding."
          : misses >= 2
            ? "Psst… one little card has a hint for you."
            : "Take your time. Your friends aren’t going anywhere."}
      </p>
      {phase === "right" && <Confetti />}
    </main>
  );
}

export function Celebration(props: {
  screen: "done" | "win";
  level: number;
  onNext: () => void;
  onHome: () => void;
}) {
  const { screen, level, onNext, onHome } = props;
  return (
    <main class="celebration">
      <Confetti />
      <div class="trophy-circle">
        {screen === "win" ? (
          <Trophy size={80} />
        ) : (
          <Star size={80} fill="currentColor" />
        )}
      </div>
      <div class="eyebrow">
        {screen === "win"
          ? "A WHOLE ADVENTURE OF HAPPY FINDS"
          : `LEVEL ${level} COMPLETE`}
      </div>
      <h1>
        {screen === "win"
          ? "You found all your pals!"
          : "Little finds. Big high five!"}
      </h1>
      <p>
        {screen === "win"
          ? "What a wonderful listener and rememberer you are."
          : "Your animal friends are so happy to see you."}
      </p>
      <button class="primary" onClick={onNext}>
        {screen === "win" ? (
          <RotateCcw size={24} />
        ) : (
          <Play size={24} fill="currentColor" />
        )}
        {screen === "win" ? "Play again" : "Next level"}
        <ArrowRight size={24} />
      </button>
      <button class="secondary" onClick={onHome}>
        <House size={21} /> Back home
      </button>
    </main>
  );
}

export function SettingsDialog(props: {
  settings: Settings;
  onChange: (settings: Settings) => void;
  onClose: () => void;
}) {
  const { settings, onChange, onClose } = props;
  const dialog = useRef<HTMLDivElement>(null);
  useEffect(() => {
    dialog.current?.querySelector<HTMLButtonElement>("button")?.focus();
  }, []);
  return (
    <div class="modal-backdrop" onClick={onClose}>
      <div
        ref={dialog}
        class="settings-panel"
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-title"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          if (e.key === "Escape") onClose();
          if (e.key !== "Tab" || !dialog.current) return;
          const controls = Array.from(
            dialog.current.querySelectorAll<HTMLElement>("button, input"),
          );
          const first = controls[0];
          const last = controls[controls.length - 1];
          if (!first || !last) return;
          if (e.shiftKey && document.activeElement === first) {
            e.preventDefault();
            last.focus();
          } else if (!e.shiftKey && document.activeElement === last) {
            e.preventDefault();
            first.focus();
          }
        }}
      >
        <div class="modal-heading">
          <h2 id="settings-title">A grown-up corner</h2>
          <button class="icon-button" aria-label="Close settings" onClick={onClose}>
            <X />
          </button>
        </div>
        <p>Make their little adventure just right.</p>
        <label class="setting-row">
          <span>
            Say the animal’s name
            <small>Turn off for sound-only clues.</small>
          </span>
          <input
            type="checkbox"
            checked={settings.sayName}
            onChange={(e) =>
              onChange({ ...settings, sayName: e.currentTarget.checked })
            }
          />
        </label>
        <label class="setting-range">
          Time to look <strong>{settings.lookSeconds} seconds</strong>
          <input
            type="range"
            min="2"
            max="8"
            value={settings.lookSeconds}
            onInput={(e) =>
              onChange({
                ...settings,
                lookSeconds: Number(e.currentTarget.value),
              })
            }
          />
          <small>Plus a little extra time for every card.</small>
        </label>
        <label class="setting-range">
          Finds per level <strong>{settings.roundsPerLevel}</strong>
          <input
            type="range"
            min="1"
            max="6"
            value={settings.roundsPerLevel}
            onInput={(e) =>
              onChange({
                ...settings,
                roundsPerLevel: Number(e.currentTarget.value),
              })
            }
          />
        </label>
        <p class="settings-note">
          Progress and these settings are saved in this browser. Speech and
          tones depend on your device’s sound settings.
        </p>
        <button class="primary" onClick={onClose}>
          All set <Check size={22} />
        </button>
      </div>
    </div>
  );
}
