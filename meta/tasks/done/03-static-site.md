# Task: Static Site

Build the static site that renders all entries from `data/programs/*.toml`.

## Requirements

- Reads all TOML files at build time and generates HTML
- Each program gets its own page (or expandable card) showing all fields:
  - URL (linked), CMR, jets + counts, reserved words + counts, parameters, witnesses, compiled output (if present)
- Index page lists all programs
- **Search** — client-side filter by name, CMR, jet, reserved word

## Notes

- Static site generator TBD (Eleventy, Astro, or a simple build script are all fine)
- Must work with the GitHub Actions deploy workflow
