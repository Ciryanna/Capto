# capto-dsh-plugin

[中文](README.zh.md)

First-class **Capto screen-recording tools for [DeepSeek Harness](https://github.com/deepseek-ai/deepseek-harness) — Everything is a Plugin.**

[![npm version](https://img.shields.io/npm/v/capto-dsh-plugin?style=flat-square)](https://www.npmjs.com/package/capto-dsh-plugin)
[![License](https://img.shields.io/badge/license-MIT-green?style=flat-square)](LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/ciryanna/Capto/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/ciryanna/Capto/actions/workflows/ci.yml)
[![Windows](https://img.shields.io/badge/platform-Windows%2010%2B-0078D4?style=flat-square&logo=windows&logoColor=white)](https://github.com/ciryanna/Capto/releases)

`capto-dsh-plugin` registers a set of typed `capto_*` tools (`capto_status`, `capto_record_start`, `capto_shot`, …) so a Harness agent can drive the **local-only [Capto](https://github.com/ciryanna/Capto) Windows screen recorder** from any conversation — no shelling out, no FFmpeg, no cloud.

- 🎬 **14 typed tools** — status / doctor / open / list / shot / record / config / outputs, each with a strict parameter schema and a canonical JSON output
- 🧭 **Agent-friendly failures** — every CLI exit is normalized to `Error: capto exited <code> (<errorCode>): <message>`; `desktopUnavailable` (exit 2) tells the model exactly what to do next (`capto_open`, wait, retry)
- 🔌 **Zero build step** — plain ESM, works from npm, a tarball, or a git checkout as-is
- 🔒 **Pure local** — talks only to the running Capto desktop over its localhost control plane; never spawns FFmpeg, never uploads anything

## Requirements

- **Windows 10+** with Capto desktop installed — the [installer](https://github.com/ciryanna/Capto/releases) embeds FFmpeg and the `capto` CLI and adds `cli\` to PATH
- A running **DeepSeek Harness** profile (`dsh web`, `dsh --profile headless`, …)
- Node ≥ 18 (the harness runtime)

## Installation

### npm (into a profile)

```bash
npm install --prefix ~/.dsh/profiles/web capto-dsh-plugin
```

Enable it in the profile's `cordis.patch.yml`:

```yaml
- insert:
    - id: capto
      name: capto-dsh-plugin
      config:
        command: ['capto']            # or an absolute path to capto.exe
        timeoutMs: 120000
        noLaunch: false
        autoOpen: false
```

Verify the composed tree, then restart the GUI:

```bash
dsh --profile web --dump-config
# restart `dsh web` — the capto_* tools appear in the agent's toolset
```

### dsh plugin (bundle form)

The package declares a [`dsh.bundle`](cordis.patch.yml) manifest, so the official pnpm-based flow wires everything automatically:

```bash
dsh plugin --profile web add capto-dsh-plugin
dsh --profile web --dump-config   # a "# == capto-dsh-plugin" layer appears
```

The npm-based flow above is the equivalent for npm-managed profiles.

### From a checkout (development)

```bash
npm install --prefix ~/.dsh/profiles/web "file:D:/path/to/Capto/packages/capto-dsh-plugin"
```

npm ≥ 7 links `file:` dependencies, so source edits are picked up on the next GUI restart.

Uninstall: `npm uninstall --prefix ~/.dsh/profiles/web capto-dsh-plugin` (or `dsh plugin --profile web remove capto-dsh-plugin`), then drop the patch row.

## Configuration

| Key | Default | Meaning |
|---|---:|---|
| `command` | `['capto']` | CLI argv prefix: `['capto']` (installed PATH), `['D:\\…\\capto.exe']`, or `['cargo','run','-p','capto-cli','--']` in a dev checkout |
| `timeoutMs` | `120000` | Per-call timeout in ms. Must exceed the CLI's 45 s desktop auto-launch wait so a cold start can finish |
| `noLaunch` | `false` | Always pass `--no-launch` — never auto-start the Capto desktop |
| `autoOpen` | `false` | On exit 2 (`desktopUnavailable`): run `capto open`, wait ~3 s, retry once |

Invalid configuration (empty `command`, non-positive `timeoutMs`, non-boolean flags) fails at plugin load with an actionable error.

## Tools

| Tool | Purpose |
|---|---|
| `capto_status` | Session snapshot `{ state, elapsedMs, outputPath, … }` — check before recording |
| `capto_doctor` | Environment readiness `{ ffmpegOk, controlPlane, … }` |
| `capto_open` | Open the desktop window (recovery entry for exit 2) |
| `capto_list` | Enumerate `displays` / `windows` / `audio` / `encoders` |
| `capto_shot` | Screenshot → `{ path }` (absolute PNG) |
| `capto_record_start` | Start recording (source, display/window/region, format, fps, quality, encoder, mic, loopback, cursor) |
| `capto_record_stop` / `capto_record_pause` / `capto_record_resume` | Recording control |
| `capto_config_get` / `capto_config_set` / `capto_config_path` | Read / patch settings (camelCase keys) |
| `capto_outputs_recent` / `capto_outputs_open` | Recent outputs / open files in Explorer |

Every tool declares a canonical JSON output and a pretty renderer; read-only tools are marked concurrency-safe. The `tool:capto` prompt section teaches the agent: check `capto_status` before starting, never start twice, recover exit 2 with `capto_open`, always end with `capto_record_stop`, and find files with `capto_outputs_recent`.

## Model Experience

- **What the model sees** — the 14 tool schemas plus a short `tool:capto` guidance section: a fixed, small per-request token cost while the plugin is active.
- **Failures** — thrown as `Error: …` carrying the CLI exit code and error code. `desktopUnavailable` messages give the model its next step (`capto_open`, wait ~3–5 s, retry; ask the user if it still fails).
- **Cancellation** — calls observe the tool signal; an aborted call raises `AbortError` and never leaves an orphan CLI child.

## Development and verification

```bash
cd packages/capto-dsh-plugin
npm install            # test deps (@deepseek-ai/dsh-tools, schemastery)
node test/smoke.mjs    # 11 checks: contract, config, arg mapping, failure
                       # normalization, timeout, autoOpen recovery, real CLI
```

`test/fixtures/fake-capto.mjs` is a fake `capto` CLI speaking the JSON envelope contract, so the suite runs without the desktop. When `target/debug/capto.exe` exists, one real-CLI check runs too (accepting both control-plane states). CI runs the suite against the freshly built CLI on Windows and fake-only on Ubuntu, plus `npm pack --dry-run`.

## Known Limitations and Deferred Work

- **Windows-only by design** — Capto is a Windows screen recorder; other platforms gain nothing.
- **One desktop session** — the tools drive the single Capto desktop process; no second capture pipeline (a Capto guarantee, enforced by its control plane).
- **No image preview inside results** — `capto_shot` returns a path; image rendering in tool results is not available in the harness yet.
- **`autoOpen` is best-effort** — it retries once after `capto open`; a wedged desktop (stale process) still needs a manual kill + relaunch per the Capto skill docs.

## License

MIT — part of the [Capto](https://github.com/ciryanna/Capto) project. See [LICENSE](LICENSE).
