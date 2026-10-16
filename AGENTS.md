# Capto — Agent Guide

Capto is a **purely local** screen capture app (Captura spiritual successor).  
Stack: **Tauri 2 + Rust + React/TypeScript**. No C#. No cloud upload.

## Non-negotiables

1. **No upload SDKs** — no Imgur, YouTube, OAuth, or remote sharing features.
2. **Encoding only via `capto-encode`** — never spawn FFmpeg ad-hoc from UI crates.
3. **New capture backends must implement `CaptureBackend`** in `capto-capture`.
4. **Windows first** — ship WGC/WASAPI path; macOS/Linux backends are stubs until implemented.
5. **UI does not process frames** — React only sends intents and renders state.
6. **CLI controls the desktop session** — binary `capto` (crate `capto-cli`) must not create a second `RecordingSession`; use the localhost control plane ([docs/CLI.md](docs/CLI.md)).

## Binaries

| Binary | Crate / package | Role |
|--------|-----------------|------|
| `capto` | `capto-cli` | Agent/CLI control plane client |
| `capto-app` | `capto-app` (Tauri) | Desktop UI; product name **Capto** |

Do not name both `capto` — they would clash in `target/debug` and on case-insensitive Windows paths.

## Repo layout

| Path | Role |
|------|------|
| `apps/desktop` | Tauri shell + React UI |
| `crates/capto-core` | Session orchestration, settings |
| `crates/capto-capture` | Capture traits + platform backends |
| `crates/capto-audio` | Mic + loopback listing/mixing intents |
| `crates/capto-encode` | FFmpeg sidecar + encoder probe |
| `crates/capto-overlay` | Overlay layout / compositor helpers |
| `crates/capto-hooks` | Hotkey / input hook abstractions |
| `crates/capto-ipc` | Local CLI↔desktop control-plane types + lockfile |
| `crates/capto-cli` | CLI client (`capto` binary) |
| `packages/capto-agent-skill` | Publishable Agent Skills npm package |
| `docs/ARCHITECTURE.md` | Pipeline + control-plane contracts |
| `docs/CLI.md` | CLI / agent JSON contract (EN + 中文) |
| `website/` | Product landing page |
| `cloudflare/` | Updater mirror Worker (check + download proxy) |

## Cloud vs local Windows

| Task | Cloud agent | Local Windows |
|------|-------------|---------------|
| Traits, UI, i18n, CLI client code, unit tests | Yes | Optional |
| WGC / WASAPI / NVENC real recording | No | **Required** |
| Package installers | CI Windows runner | Verify locally |
| Live `capto` against desktop | No (no display session) | Yes |

## Driving Capto from an agent

1. Read [docs/CLI.md](docs/CLI.md) (envelope, exit codes, workflows).
2. Prefer skill **`capto`** from npm package [`capto-agent-skill`](packages/capto-agent-skill) (`skills/capto`).
3. Typical loop: `doctor` / `list` → `record start` → poll `status` → `record stop` → `outputs recent`.
4. Parse JSON stdout; branch on exit codes (`2` = desktop unavailable).
5. Dev auto-launch: `CAPTO_APP_PATH` → `target/debug/capto-app.exe`.

```bash
capto status
# or from repo:
cargo run -p capto-cli -- status
```

## Dev commands

Fresh clone → runnable dev env in one command (npm deps + FFmpeg sidecar + staged CLI):

```powershell
.\scripts\setup-dev.ps1       # or -Local to prefer a local ffmpeg.exe
```

```bash
npm install --prefix apps/desktop
cargo test --workspace
cargo fmt --all --check              # Rust rustfmt (must pass in CI)
npm run lint --prefix apps/desktop   # frontend ESLint (must pass in CI)
npm run format:check --prefix apps/desktop  # frontend Prettier (must pass in CI)
npm test --prefix apps/desktop       # frontend Vitest unit tests
npm run test:coverage --prefix apps/desktop  # unit tests + enforced coverage gate (CI)
npm run duplicate:check --prefix apps/desktop # jscpd copy-paste gate (CI)
npm run build:analyze --prefix apps/desktop   # bundle report (dist/analyze.html + stats.json)
npm run bundle-size --prefix apps/desktop     # bundle-size budget gate (CI)
npm run tauri --prefix apps/desktop -- dev   # builds capto-app
cargo run -p capto-cli -- doctor             # runs `capto`
```

Interactive QA of a real desktop session (agent-followable):

```powershell
.\scripts\qa-smoke.ps1                  # doctor/status/config/outputs loop
.\scripts\qa-smoke.ps1 -RunRecordRoundtrip  # + real record→stop→outputs
```

See [docs/QA.md](docs/QA.md) for the full path and [docs/security-testing.md](docs/security-testing.md) for the control-plane DAST probe.

Place local FFmpeg + stage the CLI into the app bundle (required for `tauri build`):

```bash
.\scripts\copy-ffmpeg.ps1   # or download-ffmpeg.ps1
cargo build -p capto-cli --release
.\scripts\copy-cli.ps1
```

Optional local env vars (see `.env.example`): `CAPTO_APP_PATH`, `RUST_LOG`,
`RUST_BACKTRACE`.

Repo-hygiene guards (run when touching many files; enforced in CI):

```powershell
.\scripts\check-file-size.ps1   # no oversized source files
.\scripts\scan-tech-debt.ps1    # no TODO/FIXME/HACK/XXX in source
.\scripts\check-version-drift.ps1  # package.json vs tauri.conf.json version in lockstep
.\scripts\scan-dead-flags.ps1   # every declared feature flag is still used
```

Observability (local-only, privacy-first): the desktop exposes a loopback
`GET /v1/metrics` (auth required) and writes `crash-*.json` reports on panic;
`capto_*` logging is scrubbed via `capto_ipc::redact`. See [docs/PRIVACY.md](docs/PRIVACY.md) and [docs/feature-flags.md](docs/feature-flags.md).

Optional local pre-commit hooks (recommended; opt-in per clone):

```powershell
.\scripts\install-hooks.ps1     # sets core.hooksPath to .githooks
```

On every commit the hook runs file-size + tech-debt + `cargo fmt --check`, plus
frontend lint/format/knip when `apps/desktop` sources change. Bypass once with
`git commit --no-verify`. Hooks never replace CI — they only fail fast.

Installer embeds CLI at `<install>\cli\capto.exe` and adds that folder to user **PATH** (NSIS hook). Not a separate Release asset. See `apps/desktop/src-tauri/binaries/README.md` and `windows/hooks.nsh`.

## Feature matrix

See root `README.md`. P0 = MVP, P1 = next, cut list is permanent.

## CI / Release

See [docs/CI.md](docs/CI.md). Summary:

- **CI** (`.github/workflows/ci.yml`): tests on push/PR — not a publisher
- **Release** (`.github/workflows/release.yml`): tag `v*` → Windows NSIS for x64 + ARM64 with FFmpeg from `ciryanna/capto-ffmpeg` (pin in `.github/capto-ffmpeg.env`); signed updater artifacts; rolling `updater` release hosts `latest.json`
- **Pages** (`.github/workflows/pages.yml`): push to `main` → single GitHub Pages artifact: `website/` at `https://ciryanna.github.io/Capto/` + workspace rustdoc at `/docs/`. Only this workflow may write the `github-pages` environment (do not add another deploy-pages workflow).
- Current app version **1.0.0**; `v1.*` releases are **stable** (v0.* were prerelease)

Also see:

- `docs/runbooks/` — incident response (security, release, CI failure triage)
- `docs/tech-debt.md` — tracked, deliberate refactor backlog (grandfathered
  complexity entries); new code must pass the complexity gate
