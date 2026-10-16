# Runbook: Security incidents

Signals handled: `secret-scan.yml` (Gitleaks) and `codeql.yml` alerts, GitHub
security-advisory notifications, and any "new secret found" Secret Scanning
push protection block.

Severity ladder for secret leaks: **Public repo + since-pushed commit** is the
worst case (assume compromise). A working-tree-only detection on a PR is low.

## 1. Gitleaks fires (`secret-scan.yml` failed, SARIF uploaded)

1. Open the failing run → redacted finding shows file, commit, and rule.
2. Confirm whether it is a real secret (a test fixture like
   `TEST_TOKEN=abcdef` in a test file can be reworked; a real API key is not a
   false positive).
3. **Rotate first, remove later.** For every real secret: revoke/rotate it at
   the provider (Cloudflare/updater signing keys are in
   `.github/workflows/release.yml` + GitHub Actions secrets — regenerated via
   Actions UI; any OpenAI/other keys used during development must be revoked
   at the provider, not just deleted from git).
4. Remove the secret from the working tree and rewrite history only if
   publishing it caused real exposure and the repo is public:
   - `git filter-repo` (not `filter-branch`) to strip the blob, then `--force`
     push. Coordinate with any collaborators to re-clone afterward.
   - The secret still counts as exposed — rotation in step 3 is the only real
     fix.
5. If the value appeared in a GitHub Actions log, purge the run logs
   (Settings → Actions → general → disable log retention quirks) after
   rotating; note that sometimes only the secret rotation matters.

## 2. CodeQL fires (`codeql.yml` → Security tab alerts)

1. Open the alert: location, source, and sink are linked in the Security tab.
2. Confirm the flow. Most alerts here are in the React frontend (`src/`).
   - `TaurCommand`/`invoke` argument string-building — check the argument can
     never carry shell metacharacters (Capto uses structured commands, not
     shell interpolation; an alert here is usually a false positive on the
     command name).
   - DOM XSS — Capto renders localized strings via `react-i18next` (escaped
     by default); only `dangerouslySetInnerHTML` (none in `src/`) would be real.
3. Fix the root cause, link the fix PR to the alert ("fixes #alert"), or mark
   as planned for anything requiring a deliberate decision (e.g., Rust side
   not yet analyzed by CodeQL).

## 3. Secrets / supply chain hardening (standing policy)

- Keep **dependabot** and **Renovate** enabled: `.github/dependabot.yml` and
  `renovate.json` (minimumReleaseAge 3 days) update Rust + npm deps.
- All npm deps: use `npm ci`; lockfiles are committed.
- FFmpeg sidecar: shipped via `ciryanna/capto-ffmpeg` release with SHA-256 +
  attestation checks (`CAPTO_FFMPEG_VERIFY_ATTESTATION=1`). Never vendor
  unverified binaries.
- `GH_TOKEN` / signing keys exist only as GitHub Actions secrets, never in the
  repo or `.env.example`.
- **Branch protection** on `main`: force-pushes are blocked and admins are
  enforced. No required status checks are configured — direct push is
  available and PRs merge without a checks gate (CI/CodeQL/Secret scanning
  still run, but they do not block merges).

## Owner

Maintainer (`@ciryanna` per `CODEOWNERS`). Any alert that blocks a release is a
P1 for the release owner.
