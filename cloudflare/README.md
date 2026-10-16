# Capto updater mirror (Cloudflare Worker)

A free Cloudflare Worker that mirrors the Capto GitHub Release so in-app
update checks and installer downloads get a faster CDN edge (helpful where
`github.com` is slow, e.g. some networks in China).

## What it does

| Route | Behaviour |
|-------|-----------|
| `GET /updates/latest.json` | **Stable channel**: fetches `ciryanna/Capto` release tag `updater/latest.json`, rebuilds each `url` from the manifest `version` into a `github.com/.../releases/download/<tag>/<file>` browser URL, points it at this worker's download route, returns the JSON. |
| `GET /updates/canary.json` | **Canary channel (progressive rollout)**: same as above but reads the rolling `canary` release tag, so a small tester cohort can opt in to new versions before stable promotion. |
| `GET /updates/download/*`  | Streams a GitHub release asset through this worker (CF CDN caches it). |
| `OPTIONS` | CORS preflight. |

### Canary / staged rollout

Publish an experimental build to a `canary` release tag (with its own
`latest.json`), point beta testers or pilot agents at
`https://capto-update-proxy.elwina-vardal.workers.dev/updates/canary.json`,
then **promote** the same version to stable by publishing `latest.json` to the
`updater` tag. Full procedure in `docs/CI.md`.

### Why we rebuild the download URL

Tauri's generated `latest.json` points `url` at `api.github.com/repos/.../
releases/assets/<id>`, which is **rate-limited** (60 requests/hr/IP, anonymous).
The installer filename is fixed by the Release workflow (`Capto_<version>_
<arch>-setup.exe`, tag `v<version>`), so the Worker rebuilds each `url` from
`version` into a `github.com/.../releases/download/<tag>/<file>` browser URL —
**no API rate limit**, and it redirects to the signed CDN asset.

Tauri's updater `endpoints` are tried in order and only fall through on a
non-2XX response, so we list the Worker first and GitHub second — if the
Worker is ever down, updates still work via GitHub.

## Deploy (manual)

```bash
cd cloudflare
npx wrangler login        # once
npx wrangler deploy
```

Take the returned `*.workers.dev` URL (e.g. `https://capto-update-proxy.elwina-vardal.workers.dev`).

## Wire it into the app

Edit `apps/desktop/src-tauri/tauri.conf.json` → `plugins.updater.endpoints`:

```json
"endpoints": [
  "https://capto-update-proxy.elwina-vardal.workers.dev/updates/latest.json",
  "https://github.com/ciryanna/Capto/releases/download/updater/latest.json"
]
```

The Worker-first / GitHub-second order gives the fallback automatically.

## Test locally

```bash
cd cloudflare
npx wrangler dev
```

```bash
curl http://localhost:8787/updates/latest.json
# -> JSON with url fields pointing at /updates/download/https%3A%2F%2Fgithub.com%2F...

# Grab one url, decode it, and hit it:
curl -I "http://localhost:8787/updates/download/https%3A%2F%2Fgithub.com%2Fciryanna%2FCapto%2Freleases%2Fdownload%2Fv0.4.0%2FCapto_0.4.0_x64-setup.exe"
# -> 200, content-length (the real installer size), cache-control: immutable
```

## Notes

- GitHub releases are public, so no token is required. If one is ever needed, set
  `GH_TOKEN` via `npx wrangler secret put GH_TOKEN` (and `.dev.vars` locally).
- Free plan: `100,000` requests/day, CDN cache object limit `512 MB` — installers
  are only tens of MB, so this is plenty.
- Do not commit `.dev.vars`; only `.dev.vars.example` is tracked.