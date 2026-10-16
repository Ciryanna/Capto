# Capto website

Static product landing page (`index.html`).

Open locally:

```bash
start website/index.html
# or any static server:
npx --yes serve website
```

Live site: [https://ciryanna.github.io/Capto/](https://ciryanna.github.io/Capto/) (API docs at [https://ciryanna.github.io/Capto/docs/](https://ciryanna.github.io/Capto/docs/))

Deployed as part of [`.github/workflows/pages.yml`](../.github/workflows/pages.yml)
(GitHub Pages → **GitHub Actions**, single artifact: this page at the site root
+ rustdoc under `/docs/`). Push to `main`, or run that workflow manually.
Cloudflare Pages (`https://capto.elwina.work/`) deploys the same `website/` via
the CF dashboard Git integration.

Assets under `assets/` (mark + optional donate QR). Brand color is Capto purple on purpose.
