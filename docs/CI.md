# Capto CI / Release

## Workflows

| Workflow | File | Trigger | Purpose |
|----------|------|---------|---------|
| **CI** | [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) | push/PR → `main` | Rust test + clippy (`cargo build --timings` uploaded), frontend `tsc` + lint + jscpd + knip + bundle-size, `cargo check` for **x64 + ARM64**, FFmpeg pin download + attestation, devcontainer build |
| **Release** | [`.github/workflows/release.yml`](../.github/workflows/release.yml) | tag `v*` or manual | NSIS installers for both arches with **embedded** FFmpeg + CLI (`cli/capto.exe`); signed updater artifacts + rolling `updater` manifest |
| **Pages** | [`.github/workflows/pages.yml`](../.github/workflows/pages.yml) | push → `main`, or manual | Deploy **one** GitHub Pages artifact: `website/` at the site root (`https://ciryanna.github.io/Capto/`) + workspace rustdoc under `/docs/`; Cloudflare Pages (`https://capto.elwina.work/`) deploys the same `website/` via the CF dashboard Git integration |
| **CI alert** | [`.github/workflows/ci-alert.yml`](../.github/workflows/ci-alert.yml) | `workflow_run` on CI/Release/Pages failures | Open/update a `[build-health]` GitHub issue when a required workflow fails (alerting + error→issue) |

CI and Release are intentionally separate: green CI does not publish; Release does not replace day-to-day checks.

## Versioning

- App / workspace version today: **1.0.0** (`Cargo.toml` workspace + `tauri.conf.json`)
- Git tags: `v1.0.0`, … — `v1.*` releases are **stable** (`v0.*` were prerelease)

## In-app updates (GitHub)

Desktop uses [`tauri-plugin-updater`](https://v2.tauri.app/plugin/updater/) with minisign verification.

| Piece | Location |
|-------|----------|
| Public key | `apps/desktop/src-tauri/tauri.conf.json` → `plugins.updater.pubkey` |
| Private key | GitHub Actions secret **`TAURI_SIGNING_PRIVATE_KEY`** (optional empty **`TAURI_SIGNING_PRIVATE_KEY_PASSWORD`**) |
| Check URL | `https://github.com/ciryanna/Capto/releases/download/updater/latest.json` |
| Installer URLs | Inside `latest.json`, pointing at the versioned `v*` release assets |

Each Release mirrors `latest.json` onto a rolling tag **`updater`** (manifest only), keeping the in-app check URL stable across releases. CDN / CF Worker mirrors can be added later as extra `endpoints` entries.

Local key (gitignored): `.secrets/capto.key` — do not commit. Rotate only if the private key is lost or leaked (existing installs cannot verify new signatures after a pubkey change unless you ship a bridge release).

Settings UI: **关于** tab — version, manual update check, bundled FFmpeg version, developer links.

### Updater mirror (Cloudflare Worker)

A free Cloudflare Worker ([`cloudflare/`](../cloudflare/)) proxies the GitHub
update metadata and installer downloads for a faster CDN edge. Tauri updater
`endpoints` are tried **in order** and only fall through on a non-2XX response,
so we list the Worker first and GitHub second.

- Worker routes: `GET /updates/latest.json` (rebuilds each `url` from the
  manifest `version` into a `github.com/.../releases/download/<tag>/<file>`
  browser URL pointed at the worker download route) and `GET /updates/download/*`
  (streams a release asset). Rebuilding avoids `api.github.com`, which is
  rate-limited (60/hr/IP); the browser URL has no rate limit.
- Deploy: `cd cloudflare && npx wrangler login && npx wrangler deploy`, then put
  the returned `*.workers.dev` URL first in `tauri.conf.json` → `plugins.updater.endpoints`.
- Current endpoints (see [`tauri.conf.json`](../apps/desktop/src-tauri/tauri.conf.json)):

```json
"endpoints": [
  "https://capto-update-proxy.elwina-vardal.workers.dev/updates/latest.json",
  "https://github.com/ciryanna/Capto/releases/download/updater/latest.json"
]
```

If the Worker is down/non-2XX, updates still work via the GitHub fallback.

### Canary channel (staged rollout)

Progressive rollout for new releases:

1. Publish an experimental build to a rolling `canary` release tag (with its
   own `latest.json`) — e.g. `gh release create canary --prerelease` + upload
   `latest.json`.
2. Point testers/pilot agents at
   `https://capto-update-proxy.elwina-vardal.workers.dev/updates/canary.json`
   (the worker serves it from the `canary` tag).
3. Validate the canary (record/QA + crash reports), then **promote**: publish
   the same version's `latest.json` to the `updater` tag so stable users move
   off canary automatically; close the canary tag when superseded.

A bad canary never reaches stable users and can be pulled by deleting the
`canary` tag / manifest.

## Alerting on CI failure

[`.github/workflows/ci-alert.yml`](../.github/workflows/ci-alert.yml) listens
for failed `CI` / `Release` / `Deploy site + docs` runs and opens (or bumps) a
`[build-health]` issue with the failing run URL — GitHub-native alerting that
turns red CI into tracked, deduplicated work (no external service needed).


## Site + API docs hosting (GitHub Pages + Cloudflare Pages)

The static landing page [`website/`](../website/) is deployed to **both** GitHub
Pages and Cloudflare Pages (dual-active). GitHub Pages is driven by this repo's
[`pages.yml`](../.github/workflows/pages.yml) on every push to `main` (or
manual), and it deploys **one** artifact containing **both**:

- site root `https://ciryanna.github.io/Capto/` → the `website/` landing page
- `https://ciryanna.github.io/Capto/docs/` → rustdoc for the workspace lib crates
  + CLI (`cargo doc --workspace --no-deps --exclude capto-app`)

`pages.yml` is the *only* workflow that writes to the `github-pages`
environment. Cloudflare Pages is driven by the **CF dashboard Git integration**
(not by a workflow), so no `CLOUDFLARE_API_TOKEN` secret is needed.

| Host | URL | Driver |
|------|-----|--------|
| GitHub Pages | `https://ciryanna.github.io/Capto/` (site root) + `/docs/` (rustdoc) | [`pages.yml`](../.github/workflows/pages.yml) `deploy` job (`actions/deploy-pages`) |
| Cloudflare Pages | `https://capto.elwina.work/` (primary) / `https://capto.pages.dev/` | CF dashboard → Workers & Pages → **Connect to Git** |

### Cloudflare Pages project (Git integration)

Because this is a **monorepo**, the Pages project must be pointed at the
`website/` subdirectory:

- Project name: `capto`, production branch `main`
- In CF dashboard → Workers & Pages → **Create** → **Connect to Git**, authorize
  GitHub and select `ciryanna/Capto`
- **Build configurations → Root directory → `website`** (otherwise CF deploys the
  whole repo as a static site)
- No build command needed (`website/` is already static), no output directory
- Custom domain: `capto.elwina.work` — the `elwina.work` zone is hosted on
  Cloudflare DNS, so Pages **Custom domains → Set up** provisions the record
  and certificate automatically

Cloudflare Pages is the primary entry; GitHub Pages remains as the fallback if
Cloudflare is unreachable.

## FFmpeg (security)

Sidecar comes only from [`ciryanna/capto-ffmpeg`](https://github.com/ciryanna/capto-ffmpeg) Releases — never from PATH at runtime.

Pin: [`.github/capto-ffmpeg.env`](../.github/capto-ffmpeg.env)

```powershell
.\scripts\download-ffmpeg.ps1 -TargetTriple x86_64-pc-windows-msvc -VerifyAttestation
.\scripts\download-ffmpeg.ps1 -TargetTriple aarch64-pc-windows-msvc -VerifyAttestation
```

Checks:

1. Download matching asset (`ffmpeg-windows-x86_64.exe` / `ffmpeg-windows-aarch64.exe`)
2. Verify **SHA-256** against release `SHA256SUMS`
3. In CI/Release: **`gh attestation verify`** (Sigstore / Artifact Attestations)
4. Copy to `apps/desktop/src-tauri/binaries/ffmpeg-<triple>.exe` for Tauri `externalBin`

## Architectures

| Display | Rust target (internal) | Installer | Bundled CLI |
|---------|------------------------|-----------|-------------|
| **x64** | `x86_64-pc-windows-msvc` | NSIS `.exe` | `<install>\cli\capto.exe` (+ user PATH) |
| **arm64** | `aarch64-pc-windows-msvc` | NSIS `.exe` | `<install>\cli\capto.exe` (+ user PATH) |

Install location is fixed to `%LOCALAPPDATA%\Capto` (no directory chooser). Custom template: `apps/desktop/src-tauri/windows/installer.nsi`.

Job titles and release assets use **x64 / arm64**. The `*-windows-msvc` strings are only Rust target triples (Windows ABI). **MSI is not built** — NSIS setup exe only. The CLI is **not** published as a separate Release asset (`capto-windows-*.exe` retired).

## GitHub Actions versions

Aligned with Tauri’s current pipeline docs + Node 24 runners:

- `actions/checkout@v7`
- `actions/setup-node@v6` with `node-version: "24"`
- `tauri-apps/tauri-action@v1`

```powershell
cargo test --workspace
npm ci --prefix apps/desktop
npx --prefix apps/desktop tsc --noEmit -p apps/desktop
.\scripts\download-ffmpeg.ps1 -VerifyAttestation
cargo check -p capto-app -p capto-cli --target aarch64-pc-windows-msvc
```
