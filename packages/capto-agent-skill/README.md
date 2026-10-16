# capto-agent-skill

[Agent Skills](https://agentskills.io) package for **Capto** — a local-only Windows screen recorder.

Agents drive Capto with the `capto` CLI (JSON over a localhost control plane). This package ships skill docs only; install Capto from [Releases](https://github.com/ciryanna/Capto/releases) — the installer includes the CLI and adds it to PATH (`capto` in new terminals).

## Install

```bash
npm install capto-agent-skill
```

Skill entry (agentskills / skills-npm):

```
node_modules/capto-agent-skill/skills/capto/SKILL.md
```

Reference contract: `skills/capto/references/cli.md`.

## Requirements

- Windows Capto desktop with bundled FFmpeg
- `capto` on `PATH`, or in-repo: `cargo run -p capto-cli -- …`
- Dev: set `CAPTO_APP_PATH` to `capto-app.exe` if needed
- If exit code `2`: run `capto open`, or ask the user to open Capto from the Start menu

## Publish checklist (maintainers)

Skill npm version usually tracks Capto app releases when CLI/skill contracts change together (e.g. both `0.5.0`).

> **Publish path:** `package.json` lives in `packages/capto-agent-skill/` (not repo root, not `skills/capto/`).  
> Publishing from the Capto monorepo root hits the private `capto-workspace` package and will fail.

1. Bump skill `version` in `package.json` and `skills/capto/SKILL.md` metadata together (only when the skill docs/contract change).
2. Keep `references/cli.md` aligned with repo [`docs/CLI.md`](../../docs/CLI.md).
3. Dry-run pack from **repo root**:

```bash
npm pack ./packages/capto-agent-skill
# or:
npm run skill:pack
```

4. Publish (pick one):

```bash
# from repo root
npm publish ./packages/capto-agent-skill --access public

# or from the package directory
cd packages/capto-agent-skill
npm publish --access public
```

> npm name `capto` is taken by an unrelated package — publish as **`capto-agent-skill`**.

## Spec

- Format: [Agent Skills specification](https://agentskills.io/specification)
- Distribution: [skills inside npm `skills/`](https://github.com/antfu/skills-npm/blob/main/PROPOSAL.md)
