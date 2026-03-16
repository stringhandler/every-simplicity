# Problem Statement: Every Simplicity

## Overview

A public, searchable website that catalogs every `.simplicityhl` file on the internet. Entries are manually curated but automatically generated via a CLI command. Hosted on GitHub Pages.

## Goals

- Provide a browsable, searchable index of Simplicity programs found publicly on the internet
- For each program, surface rich metadata extracted directly from the source file
- Make it trivial to add new entries: run one command with a URL, get a new entry in the database

## Per-Entry Data

Each catalogued program should display:

| Field | Description |
|---|---|
| **Link** | URL to the original `.simplicityhl` file |
| **CMR** | The Commitment Merkle Root of the program |
| **Jets used** | List of jets referenced, with count of each |
| **Reserved words** | Language keywords used (e.g. `match`, `fn`), with count of each |
| **Parameters** | List of expected parameters (if any) |
| **Witness values** | Expected witness values (if any) |
| **Compiled output** | The compiled Simplicity program — only shown if the program takes no parameters |

## Site Features

- **Search** — filter entries by name, jet, reserved word, CMR, or any metadata field
- **Static** — no backend; all data lives in a JSON/YAML database committed to the repo
- **GitHub Pages hosted** — built as a static site from the repo

## Workflow: Adding an Entry

```
simplicity-catalog add <url>
```

This command should:

1. Fetch the `.simplicityhl` file at the given URL
2. Parse and extract all metadata (CMR, jets, reserved words, parameters, witnesses)
3. Attempt compilation — if the program has no parameters, include the compiled output
4. Append a new entry to the database file (e.g. `data/programs.json`)
5. Optionally regenerate the static site

The goal is that adding a new program takes seconds and requires no manual editing of data files.

## Tooling

- **Compiler**: `simc` from simplicityHL — used to compile programs and extract CMR, jets, reserved words, parameters, and witnesses

## Open Questions

- **Database**: one TOML file per program under `data/programs/`
- **CLI**: local tool — run `simplicity-catalog add <url>` on your machine, commit the generated TOML, push; Actions handles the rest
- **Build & deploy**: GitHub Actions builds and deploys to GitHub Pages on every push to `main`
- Static site generator TBD — no strong preference, as long as it fits the Actions-based workflow
