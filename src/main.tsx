import { House, Settings2, ShieldCheck, Sparkles, Volume2, VolumeX } from "lucide-preact";
import { render } from "preact";
import { useEffect, useRef, useState } from "preact/hooks";
import { setSound, soundEnabled, unlockAudio } from "./audio";
import {
  hasStoredSettings,
  loadSettings,
  nextUnlockedLevel,
  availableLevels,
  parseSettings,
  readProgress,
  saveSettings,
  type Settings,
} from "./game";
import { Management } from "./Management";
import { Celebration, HomeScreen, PlayScreen, SettingsDialog } from "./screens";
import { useCatalog, useSession, type Screen } from "./session";
import "./style.css";

function App() {
  const [screen, setScreen] = useState<Screen>("home");
  const [catalogAttempt, setCatalogAttempt] = useState(0);
  const catalog = useCatalog(catalogAttempt);
  const [cleared, setCleared] = useState(() => readProgress());
  const [sound, setSoundState] = useState(soundEnabled);
  const [settings, setSettings] = useState<Settings>(loadSettings);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const settingsStored = useRef(hasStoredSettings());
  const settingsButton = useRef<HTMLButtonElement>(null);
  const levels = availableLevels(catalog.animals.length);
  const nextLevel = nextUnlockedLevel(cleared, levels);
  const session = useSession({
    animals: catalog.animals,
    settings,
    categoryId: catalog.categoryId,
    catalogReady: catalog.status === "ready",
    cleared,
    setCleared,
    setScreen,
  });

  useEffect(() => {
    setCleared(readProgress(catalog.categoryId));
  }, [catalog.categoryId]);

  useEffect(() => {
    if (settingsStored.current) return;
    const controller = new AbortController();
    void fetch("/api/config", { signal: controller.signal })
      .then((r) => (r.ok ? r.json() : null))
      .then((config) => {
        const parsed = parseSettings(config);
        if (parsed && !settingsStored.current) setSettings(parsed);
      })
      .catch(() => {});
    return () => controller.abort();
  }, []);

  function updateSettings(next: Settings) {
    settingsStored.current = true;
    setSettings(next);
    saveSettings(next);
  }
  function closeSettings() {
    setSettingsOpen(false);
    settingsButton.current?.focus();
  }
  function toggleSound() {
    const next = !sound;
    setSoundState(next);
    setSound(next);
    if (next) {
      unlockAudio();
      if (screen === "play" && session.phase === "find") session.prompt(session.target);
    }
  }
  function home() {
    session.home();
  }

  return (
    <div class="app-shell">
      <header class="site-header">
        <button class="brand" onClick={home} aria-label="Peekaboo Pals home">
          <span class="brand-mark">
            <span />
            <span />
            <span />
          </span>
          <span>
            peekaboo<span class="brand-pals">pals</span>
            <span class="brand-dot">.</span>
          </span>
        </button>
        <div class="header-right">
          <span class="little-label">
            <span class="online-dot" /> LITTLE FINDS. BIG SMILES.
          </span>
          <button
            class="icon-button"
            aria-label={sound ? "Mute sound" : "Turn sound on"}
            onClick={toggleSound}
          >
            {sound ? <Volume2 size={22} /> : <VolumeX size={22} />}
          </button>
          {screen === "home" && (
            <button
              ref={settingsButton}
              class="icon-button"
              aria-label="Parent settings"
              onClick={() => setSettingsOpen(true)}
            >
              <Settings2 size={22} />
            </button>
          )}
          {screen !== "home" && (
            <button class="icon-button" aria-label="Go home" onClick={home}>
              <House size={22} />
            </button>
          )}
        </div>
      </header>

      {screen === "manage" && (
        <Management
          onClose={() => {
            home();
            setCatalogAttempt((v) => v + 1);
          }}
        />
      )}
      {screen === "home" && (
        <HomeScreen
          categories={catalog.categories}
          categoryId={catalog.categoryId}
          catalogStatus={catalog.status}
          animals={catalog.animals}
          cleared={cleared}
          available={levels}
          nextLevel={nextLevel}
          onChooseCategory={catalog.chooseCategory}
          onManage={() => {
            session.clearAll();
            setScreen("manage");
          }}
          onPlay={session.startLevel}
          onRetry={() => setCatalogAttempt((value) => value + 1)}
        />
      )}
      {screen === "play" && (
        <PlayScreen
          level={session.level}
          round={session.round}
          phase={session.phase}
          cards={session.cards}
          misses={session.misses}
          progress={session.progress}
          settings={settings}
          target={session.target}
          animals={catalog.animals}
          onTap={session.tapCard}
          onReplay={() => session.prompt(session.target)}
        />
      )}
      {(screen === "done" || screen === "win") && (
        <Celebration
          screen={screen}
          level={session.level}
          onNext={() => session.startLevel(screen === "win" ? 1 : session.level + 1)}
          onHome={home}
        />
      )}

      <footer>
        <span>
          <ShieldCheck size={16} /> Made for little ones, with love.
        </span>
        <span>
          No scores. No pressure. Just play.
          <span class="footer-flower">
            <Sparkles size={16} />
          </span>
        </span>
      </footer>

      {settingsOpen && (
        <SettingsDialog
          settings={settings}
          onChange={updateSettings}
          onClose={closeSettings}
        />
      )}
    </div>
  );
}

render(<App />, document.getElementById("app")!);
