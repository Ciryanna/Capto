<p align="center">
  <img src="apps/desktop/public/capto-mark.png" alt="Capto" width="220" />
</p>

<h1 align="center">Capto</h1>

<p align="center">
  <strong>Ultra-light Windows screen recorder</strong><br />
  Spiritual successor to <a href="https://github.com/MathewSachin/Captura">Captura</a>.
</p>

<p align="center">
  <a href="https://github.com/ciryanna/Capto/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/ciryanna/Capto/ci.yml?branch=main&style=flat-square&label=CI" alt="CI" /></a>
  <a href="https://github.com/ciryanna/Capto/releases/latest"><img src="https://img.shields.io/github/v/release/ciryanna/Capto?include_prereleases&style=flat-square&label=release" alt="Release" /></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green?style=flat-square" alt="License" /></a>
  <a href="https://github.com/ciryanna/Capto/releases"><img src="https://img.shields.io/github/downloads/ciryanna/Capto/total?style=flat-square" alt="Downloads" /></a>
  <a href="https://www.npmjs.com/package/capto-agent-skill"><img src="https://img.shields.io/npm/v/capto-agent-skill?style=flat-square&label=capto-agent-skill" alt="npm" /></a>
  <a href="https://www.npmjs.com/package/capto-dsh-plugin"><img src="https://img.shields.io/npm/v/capto-dsh-plugin?style=flat-square&label=capto-dsh-plugin" alt="npm" /></a>
  <img src="https://img.shields.io/badge/platform-Windows%2010%2B-0078D4?style=flat-square&logo=windows&logoColor=white" alt="Windows" />
  <img src="https://img.shields.io/badge/stack-Tauri%202%20%7C%20Rust%20%7C%20React-9B7BFF?style=flat-square" alt="Stack" />
</p>

<p align="center">
  <a href="README.zh-CN.md">中文</a> ·
  <a href="https://ciryanna.github.io/Capto/">Website</a> ·
  <a href="https://github.com/ciryanna/Capto/releases">Releases</a>
</p>

## Why Capto

| | |
|:---:|:---|
| 🪟 | **Capture modes + Windows-tuned stack** — Display / window / region, with a Windows-first path (DXGI / WASAPI and friends) instead of a generic lowest-common-denominator backend. |
| 🎬 | **MP4 & GIF** — Record to MP4 (auto NVENC / QSV / AMF / libx264) or export GIF. |
| ✨ | **Overlays** — Mouse-click highlights, keystroke overlay, webcam PiP, include-cursor, and live preview. |
| 🎞️ | **Rebuilt FFmpeg sidecar (`capto-ffmpeg`)** — Capto-owned, pinned, attested FFmpeg embedded in the app. Encoding goes only through this bundle — not whatever is on `PATH`. |
| 🤖 | **CLI + Agent integrations** — Full `capto` control plane (JSON, stable exit codes) plus [`capto-agent-skill`](https://www.npmjs.com/package/capto-agent-skill) (Agent Skills) and [`capto-dsh-plugin`](https://www.npmjs.com/package/capto-dsh-plugin) (DeepSeek Harness tools) so agents can doctor → record → stop → collect outputs. |
| 🔒 | **Open source · local · safe** — MIT, no upload SDKs, files stay on your machine. |

## Install

Download the NSIS setup for your CPU (**x64** / **arm64**) from [Releases](https://github.com/ciryanna/Capto/releases). Installers embed verified FFmpeg from [`capto-ffmpeg`](https://github.com/ciryanna/capto-ffmpeg) and the `capto` CLI at `<install>\cli\capto.exe`, and add that `cli` folder to your user **PATH** so `capto` works in new terminals (not a separate download).

### AI / agent support

Capto ships two agent integrations. Both talk to the same local `capto` CLI control plane, so install the desktop app first (above), then pick your agent:

**Agent Skill — [`capto-agent-skill`](https://www.npmjs.com/package/capto-agent-skill)**

Skill docs for any [Agent Skills](https://agentskills.io)-compatible agent (Claude Code, Cursor, …):

```bash
npm install capto-agent-skill
```

The skill is auto-discovered from `node_modules/capto-agent-skill/skills/capto/SKILL.md` (agentskills / skills-npm convention) and teaches the agent the doctor → record → stop → collect-outputs workflow.

**DeepSeek Harness plugin — [`capto-dsh-plugin`](https://www.npmjs.com/package/capto-dsh-plugin)**

First-class `capto_*` tools (status / record / shot / config / outputs …) for a dsh profile:

```bash
# npm-managed profile: install …
npm install --prefix ~/.dsh/profiles/web capto-dsh-plugin
```

…then enable it in the profile's `cordis.patch.yml`:

```yaml
- insert:
    - id: capto
      name: capto-dsh-plugin
      config:
        command: ['capto']            # or the absolute path to capto.exe
        timeoutMs: 120000
        noLaunch: false
        autoOpen: false
```

…or use the official pnpm/bundle flow, which wires everything automatically via `dsh.bundle`:

```bash
dsh plugin --profile web add capto-dsh-plugin
dsh --profile web --dump-config
```

Restart `dsh web` and the agent's toolset includes the `capto_*` tools. Config reference and tool list: [`packages/capto-dsh-plugin`](packages/capto-dsh-plugin).

## Develop
One command gets you from a fresh clone to a runnable `tauri dev`:

```powershell
.\scripts\setup-dev.ps1     # npm install + FFmpeg sidecar + CLI build/stage
```

Step-by-step (what setup-dev.ps1 does):

```bash
npm install --prefix apps/desktop
.\scripts\download-ffmpeg.ps1   # or copy-ffmpeg.ps1
cargo build -p capto-cli --release
.\scripts\copy-cli.ps1          # required for tauri build / package
npm run tauri --prefix apps/desktop -- dev
cargo test --workspace
cargo run -p capto-cli -- status
```

Environment variables (all optional; see `.env.example`):

```powershell
$env:CAPTO_APP_PATH = "D:\AIWorkspace\Capto\target\debug\capto-app.exe"  # CLI->desktop auto-launch
$env:RUST_LOG       = "debug"   # Rust tracing filter for CLI/app
```

If the CLI cannot find the desktop in dev, point `CAPTO_APP_PATH` at a
prebuilt `capto-app.exe` (see above).

Repo-hygiene checks (also enforced in CI):

```powershell
.\scripts\check-file-size.ps1   # fails on oversized source files
.\scripts\scan-tech-debt.ps1    # fails on TODO/FIXME/HACK/XXX in source
.\scripts\check-version-drift.ps1  # package.json and tauri.conf.json in lockstep
.\scripts\scan-dead-flags.ps1   # fails on declared-but-unused feature flags
```

Frontend build-quality gates (also enforced in CI):

```powershell
npm run duplicate:check --prefix apps/desktop   # jscpd copy-paste detection
npm run build:analyze --prefix apps/desktop     # bundle visualization (dist/analyze.html)
npm run bundle-size --prefix apps/desktop       # bundle-size budget gate
```

Interactive QA of a running desktop (agent-followable):

```powershell
.\scripts\qa-smoke.ps1                  # doctor/status/config/outputs round-trip
.\scripts\qa-smoke.ps1 -RunRecordRoundtrip  # + a real record → stop → outputs
```

See [docs/QA.md](docs/QA.md) for the full interactive path,
[docs/PRIVACY.md](docs/PRIVACY.md) for data handling, and
[docs/feature-flags.md](docs/feature-flags.md) for feature toggles.
## Author

**Elwina Vardal** · [elwina.work](https://www.elwina.work) · [GitHub](https://github.com/elwina)

## License

MIT (clean-room implementation — not a Captura fork)
