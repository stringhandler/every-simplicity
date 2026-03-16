# Every Simplicity

A searchable catalog of every `.simplicityhl` file on the internet.

> **Disclaimer:** This is an independent, community project and is not officially endorsed by, affiliated with, or associated with Blockstream. Simplicity programs indexed here are sourced from public third-party repositories and have not been audited. This is not financial advice. Use at your own risk. See [DISCLAIMER.md](DISCLAIMER.md) for full details.

## Adding a program

Requires: Docker (running), Rust/Cargo.

```sh
# build the CLI once
cargo build --release --manifest-path cli/Cargo.toml

# step 1: add a file (no Docker needed — just records the URL and tags)
./cli/target/release/simplicity-catalog add \
  https://github.com/BlockstreamResearch/SimplicityHL/blob/master/examples/checkSigHashAll.simf \
  --tag examples --tag bitcoin

# step 2: compile — runs Docker, clones the repo, runs simc, updates the TOML
./cli/target/release/simplicity-catalog compile blockstreamresearch-simplicityhl-checksigHashAll
```

## Recompiling

```sh
# one entry by slug
simplicity-catalog compile <slug>

# all entries with a tag
simplicity-catalog compile --tag bitcoin

# everything
simplicity-catalog compile --all
```

Files from the same repo are cloned once per `compile` run. On first compile the CLI builds a Docker image (`simplicity-catalog-simc`) with asdf + simc installed. Commit and push after compiling — GitHub Actions rebuilds and deploys automatically.

## Building locally

```sh
npm install
npm run build
# output is in dist/index.html
```

## How it works

- `data/programs/*.toml` — one file per catalogued program
- `site/build.js` — reads all TOML files, generates `dist/index.html`
- `cli/` — Rust CLI (`cargo build --release`)
- `.github/workflows/deploy.yml` — builds and deploys to GitHub Pages on every push to `main`
