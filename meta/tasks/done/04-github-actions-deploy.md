# Task: GitHub Actions — Build & Deploy

Set up `.github/workflows/deploy.yml` to build the static site and deploy to GitHub Pages on every push to `main`.

## Steps

1. Trigger: `push` to `main`
2. Install dependencies (site generator)
3. Build the site from `data/programs/*.toml`
4. Deploy built output to GitHub Pages (`gh-pages` branch or Pages via Actions)
