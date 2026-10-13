# Agent Guide

## Scope And Architecture

This guide applies to the entire repository. Read `README.md` and relevant
source files before making changes. The original handover describes the
game's design intent; the current source and README describe the standalone
implementation, category management, uploads, MCP, and Docker deployment.

- Frontend: Preact, Vite, strict TypeScript.
- Backend: Rust, Axum, Tokio, rusqlite. Local builds bundle SQLite; Docker links the distro library.
- MCP: official Rust `rmcp` SDK, Streamable HTTP at `/mcp`, inside the same backend.
- Production: one Rust process serves `dist`, `/api`, `/uploads`, and `/mcp`.
- Docker Compose: one app container with persistent `/data`.

Do not introduce a separate frontend production server, Node MCP server,
or database service without an explicit request.

## Source Map

- `src/main.tsx`: app shell, sound, parent settings, screen switching.
- `src/screens.tsx`: home, play, celebration, and settings views.
- `src/session.ts`: catalog loading, round timers, taps, and level flow.
- `src/game.ts`: catalog parsing, illustration mappings, round generation, layout, progress.
- `src/audio.ts`: clip sequencing (uploaded sounds, generated voice clips), tones, audio cleanup.
- `src/phrases.json`: fixed spoken phrases; `npm run speech` (`scripts/generate-speech.mjs`) renders them to `public/speech`.
- `src/Management.tsx`: parent category/item CRUD, uploads, audio previews.
- `src/catalog.ts`: frontend CRUD and upload requests.
- `src/style.css`: responsive toy-box design and reduced-motion rules.
- `src/game.test.ts`: catalog and game utility tests.
- `backend/src/main.rs`: routes, startup, shared state, token handling, static serving.
- `backend/src/db.rs`: SQLite operations and transactional migration.
- `backend/src/catalog.sql`: category, item, media, and migration schema.
- `backend/src/animals.sql`: legacy animal seeds used during initialization.
- `backend/src/admin.rs`: authorized CRUD, validation, upload handling.
- `backend/src/mcp.rs`: tool definitions and shared-handler MCP operations.
- `backend/src/speech.rs`: ElevenLabs client and item/category phrase templates.
- `Dockerfile`, `compose.yaml`, `.dockerignore`: build, runtime, persistence, context exclusions.

## Game Invariants

- Show cards, hide them, play the target's sound, then accept a matching tap.
- Up to six levels, with `level + 2` cards; three items enable level 1.
- Three correct finds per level by default; parent settings can change this.
- Reshuffle each round and never request the same item consecutively.
- Wrong guesses are gentle, never a game-over state.
- After two misses, hint at the correct card while keeping its touch target stable.
- Progress is browser-local and separate per category; preserve legacy progress.
- Clear timers and clip playback on navigation and new rounds.
- Disable Play until the selected catalog loads; expose retry on loading failure.
- Uploaded sounds play before spoken prompts. All speech is pre-generated
  ElevenLabs audio; never use browser `speechSynthesis`. Missing or failing
  clips fall through to the next candidate (spoken fallback, fixed question) or silence.

## UI Conventions

Keep the warm cream `#FFF4D6` ground, orange `#E2570F` primary buttons,
blue `#2F6FDB` card backs, rounded typography, and chunky press-down shadows.
Use inline SVG icons, not emoji.

Touch targets must be at least 56 px; cards at least 64 px. Keep layouts usable
on phones, tablets, and rotation without horizontal overflow. Preserve
`prefers-reduced-motion`, visible keyboard focus, dialog keyboard behavior,
and accessible loading/error announcements. Instructions must also be spoken,
using clips from `src/phrases.json` (regenerate with `npm run speech`).

## Data And Security

- Preserve `backend/data`, database records, uploads, and tokens. Compose bind-mounts that directory.
- Never commit or include persistent data or secrets in Docker build contexts.
- Use parameterized SQL and transactional, idempotent migrations.
- Migrations must not overwrite user edits or recreate intentionally deleted categories.
- Keep blocking SQLite/file work off Tokio workers where practical.
- CRUD writes and all MCP requests require the same Bearer admin token.
- Tokens must stay out of logs, frontend bundles, URLs, and committed config.
- Keep MCP loopback Host and Origin validation; do not weaken it silently.
- Item image/audio URLs must refer to registered uploads of the correct kind.
- Accept only supported passive image/audio formats, with upload size limits.
- Use generated filenames; reject arbitrary server paths and remote fetching.
- The only outbound call is ElevenLabs text-to-speech. `ELEVEN_LABS_API_KEY`
  lives in `.env`; keep it out of logs, the frontend bundle, and Docker contexts.
- Item/category voice clips are generated in the shared save operations (UI and MCP),
  stored as registered audio media, and must never make a save fail.
- MCP uploads accept base64 contents, not file paths; decoded limit is 20 MB.
- MCP deletion requires `confirm: true`; obtain user approval before destructive calls.
- UI and MCP must share validation and database operations.
- Uploaded files may be shared; do not delete media automatically with records.

## Commands

Use Node.js 20+ and Rust 1.88+ (the Docker build pins its own versions).
Run commands from the repository root:

```sh
npm ci
npm run dev
cargo run --manifest-path backend/Cargo.toml
npm run build
npm test
cargo test --manifest-path backend/Cargo.toml
cargo clippy --manifest-path backend/Cargo.toml -- -D warnings
```

Development: Vite on port 5173 proxies `/api` and `/uploads` to Rust on port 3001.
Production: build the frontend, then Rust serves it directly on port 3001.

```sh
docker compose config
docker compose up --build -d
docker compose ps
docker compose down
```

Docker exposes localhost:8080 by default. `./backend/data` is bind-mounted at
`/data` and persists SQLite, uploads, and the generated token. Do not delete
that directory without explicit approval to erase data.

## Verification And Change Discipline

Keep changes focused and preserve existing user edits. Update lockfiles when
dependencies change, and update README for API, configuration, or startup changes.
Do not hand-edit generated `dist`, dependencies, or Rust build artifacts.

Run the frontend build/tests and backend tests/lint for relevant changes.
For UI changes, verify actual browser behavior on desktop and phone sizes.
For MCP changes, verify authentication, handshake, tool discovery, CRUD,
upload validation, and destructive-operation confirmation.
Use isolated temporary databases and uploads for integration tests.

When Docker is available, validate Compose and test the real image, static
frontend, health endpoint, and volume persistence. Clearly report checks that
could not run; parsing YAML alone is not a container build verification.
