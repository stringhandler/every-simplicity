import { readFileSync, readdirSync, statSync, writeFileSync, mkdirSync, copyFileSync, existsSync, cpSync } from "fs";
import { join, relative, dirname, basename } from "path";
import { fileURLToPath } from "url";
import { parse } from "smol-toml";

const __dirname = dirname(fileURLToPath(import.meta.url));
const DATA_DIR = join(__dirname, "../data/programs");
const OUT_DIR = join(__dirname, "../dist");

function collectTomls(dir) {
  const results = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      results.push(...collectTomls(full));
    } else if (entry.endsWith(".toml")) {
      results.push(full);
    }
  }
  return results;
}

function loadPrograms() {
  return collectTomls(DATA_DIR).map((file) => {
    const raw = readFileSync(file, "utf8");
    const data = parse(raw);
    data._slug = relative(DATA_DIR, file).replace(/\.toml$/, "").replace(/\\/g, "/");

    // Base .simb
    const simbSrc = file.replace(/\.toml$/, ".simb");
    data._simb = existsSync(simbSrc) ? data._slug + ".simb" : null;

    // Witness .simb files: <stem>.<witnessname>.simb alongside the TOML
    const dir = dirname(file);
    const stem = basename(file, ".toml");
    const slugDir = relative(DATA_DIR, dir).replace(/\\/g, "/");
    const prefix = slugDir ? slugDir + "/" : "";
    const witnessSimbs = readdirSync(dir)
      .filter((e) => e.startsWith(stem + ".") && e.endsWith(".simb") && e !== stem + ".simb")
      .map((e) => {
        const witnessName = e.slice(stem.length + 1, -5); // strip "<stem>." prefix and ".simb" suffix
        return {
          name: witnessName,
          path: prefix + e,
          src: join(dir, e),
        };
      });
    data._witness_simbs = witnessSimbs.map(({ name, path }) => ({ name, path }));

    // Derive base64 versions of hex prefixes
    if (data.program_prefix) {
      data.program_prefix_b64 = Buffer.from(data.program_prefix, "hex").toString("base64");
    }
    if (data.canonical_prefix) {
      data.canonical_prefix_b64 = Buffer.from(data.canonical_prefix, "hex").toString("base64");
    }
    if (data.debug_program_prefix) {
      data.debug_program_prefix_b64 = Buffer.from(data.debug_program_prefix, "hex").toString("base64");
    }
    if (data.debug_canonical_prefix) {
      data.debug_canonical_prefix_b64 = Buffer.from(data.debug_canonical_prefix, "hex").toString("base64");
    }

    return {
      _simbSrc: simbSrc,
      _witnessSimbs: witnessSimbs,
      ...data,
    };
  });
}

function buildHtml(programs) {
  const dataJson = JSON.stringify(programs);
  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>Every Simplicity</title>
  <style>
    *, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

    body {
      font-family: system-ui, -apple-system, sans-serif;
      background: #0d1117;
      color: #e6edf3;
      min-height: 100vh;
    }

    header {
      border-bottom: 1px solid #21262d;
      padding: 1.5rem 2rem;
    }

    header h1 {
      font-size: 1.4rem;
      font-weight: 600;
      color: #f0f6fc;
    }

    header p {
      color: #8b949e;
      font-size: 0.875rem;
      margin-top: 0.25rem;
    }

    .disclaimer {
      margin-top: 0.75rem;
      padding: 0.6rem 0.9rem;
      background: #161b22;
      border: 1px solid #30363d;
      border-left: 3px solid #d29922;
      border-radius: 4px;
      font-size: 0.775rem;
      color: #8b949e;
      max-width: 780px;
    }

    .disclaimer a {
      color: #58a6ff;
      text-decoration: none;
    }

    .disclaimer a:hover { text-decoration: underline; }

    .search-bar {
      padding: 1.25rem 2rem;
      border-bottom: 1px solid #21262d;
    }

    .search-bar input {
      width: 100%;
      max-width: 480px;
      padding: 0.5rem 0.75rem;
      background: #161b22;
      border: 1px solid #30363d;
      border-radius: 6px;
      color: #e6edf3;
      font-size: 0.875rem;
      outline: none;
    }

    .search-bar input:focus {
      border-color: #388bfd;
    }

    .count {
      font-size: 0.8rem;
      color: #8b949e;
      margin-top: 0.5rem;
    }

    .programs {
      padding: 1.5rem 2rem;
      display: grid;
      gap: 1rem;
    }

    .card {
      background: #161b22;
      border: 1px solid #21262d;
      border-radius: 8px;
      padding: 1.25rem 1.5rem;
    }

    .card-header {
      display: flex;
      align-items: baseline;
      gap: 0.75rem;
      margin-bottom: 0.75rem;
    }

    .program-name {
      font-weight: 600;
      font-size: 1rem;
      color: #f0f6fc;
    }

    .source-link {
      font-size: 0.75rem;
      color: #58a6ff;
      text-decoration: none;
    }

    .source-link:hover { text-decoration: underline; }

    .cmr {
      font-family: monospace;
      font-size: 0.75rem;
      color: #8b949e;
      background: #0d1117;
      padding: 0.1rem 0.4rem;
      border-radius: 4px;
      border: 1px solid #21262d;
    }

    .meta-grid {
      display: grid;
      grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
      gap: 0.75rem;
      margin-top: 0.75rem;
    }

    .meta-section h3 {
      font-size: 0.7rem;
      text-transform: uppercase;
      letter-spacing: 0.05em;
      color: #8b949e;
      margin-bottom: 0.4rem;
    }

    .tag-list {
      display: flex;
      flex-wrap: wrap;
      gap: 0.3rem;
    }

    .tag {
      font-size: 0.75rem;
      font-family: monospace;
      background: #21262d;
      border: 1px solid #30363d;
      border-radius: 4px;
      padding: 0.1rem 0.4rem;
      color: #c9d1d9;
    }

    .tag .count-badge {
      color: #8b949e;
      margin-left: 0.25rem;
    }

    .compiled-toggle {
      margin-top: 0.75rem;
    }

    .compiled-toggle summary {
      cursor: pointer;
      font-size: 0.75rem;
      color: #8b949e;
      user-select: none;
    }

    .compiled-toggle summary:hover { color: #e6edf3; }

    .compiled-output {
      margin-top: 0.5rem;
      background: #0d1117;
      border: 1px solid #21262d;
      border-radius: 4px;
      padding: 0.75rem;
      font-family: monospace;
      font-size: 0.75rem;
      color: #a5d6ff;
      overflow-x: auto;
      white-space: pre;
    }

    .simb-link {
      font-size: 0.8rem;
      font-family: monospace;
      color: #58a6ff;
      text-decoration: none;
    }
    .simb-link:hover { text-decoration: underline; }

    .empty {
      padding: 3rem 2rem;
      color: #8b949e;
      text-align: center;
    }

    .hidden { display: none; }

    .ext-modal {
      display: none;
      position: fixed;
      inset: 0;
      background: rgba(0,0,0,0.75);
      z-index: 1000;
      align-items: center;
      justify-content: center;
    }
    .ext-modal.open { display: flex; }

    .ext-modal-inner {
      background: #161b22;
      border: 1px solid #30363d;
      border-radius: 8px;
      padding: 1.75rem 2rem;
      max-width: 480px;
      width: 90vw;
    }

    .ext-modal-inner h2 {
      font-size: 1rem;
      font-weight: 600;
      color: #f0f6fc;
      margin-bottom: 0.5rem;
    }

    .ext-modal-inner p {
      font-size: 0.875rem;
      color: #8b949e;
      line-height: 1.5;
      margin-bottom: 0.25rem;
    }

    .ext-modal-url {
      font-family: monospace;
      font-size: 0.75rem;
      color: #58a6ff;
      word-break: break-all;
      background: #0d1117;
      border: 1px solid #21262d;
      border-radius: 4px;
      padding: 0.4rem 0.6rem;
      margin: 0.75rem 0 1.25rem;
      display: block;
    }

    .ext-modal-actions {
      display: flex;
      gap: 0.75rem;
      justify-content: flex-end;
    }

    .ext-btn-cancel {
      background: none;
      border: 1px solid #30363d;
      border-radius: 6px;
      color: #8b949e;
      cursor: pointer;
      font-size: 0.875rem;
      padding: 0.4rem 1rem;
    }
    .ext-btn-cancel:hover { color: #e6edf3; border-color: #8b949e; }

    .ext-btn-proceed {
      background: #21262d;
      border: 1px solid #30363d;
      border-radius: 6px;
      color: #f85149;
      cursor: pointer;
      font-size: 0.875rem;
      padding: 0.4rem 1rem;
    }
    .ext-btn-proceed:hover { background: #2d1b1b; border-color: #f85149; }

    footer {
      border-top: 1px solid #21262d;
      padding: 1.25rem 2rem;
      font-size: 0.8rem;
      color: #8b949e;
      line-height: 1.6;
    }

    .tabs {
      display: flex;
      gap: 0;
      border-bottom: 1px solid #21262d;
      padding: 0 2rem;
    }

    .tab-btn {
      background: none;
      border: none;
      border-bottom: 2px solid transparent;
      color: #8b949e;
      cursor: pointer;
      font-size: 0.875rem;
      padding: 0.75rem 1rem;
      margin-bottom: -1px;
    }

    .tab-btn:hover { color: #e6edf3; }
    .tab-btn.active { color: #f0f6fc; border-bottom-color: #f78166; }

    .tab-panel { display: none; }
    .tab-panel.active { display: block; }

    .jets-table {
      width: 100%;
      border-collapse: collapse;
      font-size: 0.875rem;
    }

    .jets-table th {
      text-align: left;
      font-size: 0.7rem;
      text-transform: uppercase;
      letter-spacing: 0.05em;
      color: #8b949e;
      padding: 0.5rem 1rem;
      border-bottom: 1px solid #21262d;
    }

    .jets-table td {
      padding: 0 1rem;
      border-bottom: 1px solid #161b22;
      vertical-align: top;
    }

    .jet-row-toggle {
      cursor: pointer;
      width: 100%;
      text-align: left;
      background: none;
      border: none;
      color: inherit;
      font-family: monospace;
      font-size: 0.875rem;
      padding: 0.6rem 0;
      display: flex;
      align-items: center;
      gap: 0.5rem;
    }

    .jet-row-toggle:hover { color: #58a6ff; }

    .jet-chevron { font-size: 0.65rem; color: #8b949e; transition: transform 0.15s; }
    .jet-row-toggle[aria-expanded="true"] .jet-chevron { transform: rotate(90deg); }

    .jet-scripts {
      display: none;
      padding: 0.4rem 0 0.75rem 1.5rem;
    }

    .jet-script-row {
      display: flex;
      align-items: center;
      gap: 0.35rem;
      padding: 0.15rem 0;
    }

    .jet-script-link {
      background: none;
      border: none;
      cursor: pointer;
      font-size: 0.8rem;
      color: #58a6ff;
      padding: 0;
      text-align: left;
    }
    .jet-script-link:hover { text-decoration: underline; }

    .jet-script-ext {
      font-size: 0.7rem;
      color: #8b949e;
      text-decoration: none;
      line-height: 1;
      padding: 0.1rem 0.3rem;
      border: 1px solid #30363d;
      border-radius: 3px;
      flex-shrink: 0;
    }
    .jet-script-ext:hover { color: #e6edf3; border-color: #8b949e; }

    .card-highlight {
      outline: 2px solid #388bfd;
      transition: outline-color 0.8s ease;
    }
    .card-highlight-fade {
      outline: 2px solid transparent;
      transition: outline-color 0.8s ease;
    }

    .badge {
      font-size: 0.7rem;
      font-family: monospace;
      background: #21262d;
      border: 1px solid #30363d;
      border-radius: 10px;
      padding: 0.1rem 0.5rem;
      color: #8b949e;
    }

    .bar-cell {
      width: 40%;
      padding: 0 1rem 0 0;
      vertical-align: middle;
    }

    .bar-track {
      background: #21262d;
      border-radius: 3px;
      height: 6px;
      overflow: hidden;
    }

    .bar-fill {
      height: 100%;
      background: #388bfd;
      border-radius: 3px;
      transition: width 0.2s;
    }

    .filter-btn {
      background: none;
      border: 1px solid #30363d;
      border-radius: 6px;
      color: #8b949e;
      cursor: pointer;
      font-size: 0.8rem;
      padding: 0.25rem 0.75rem;
    }
    .filter-btn:hover { color: #e6edf3; border-color: #8b949e; }
    .filter-btn.active { color: #f0f6fc; border-color: #58a6ff; background: #1c2d3f; }

    .tools-field { padding: 0.6rem 0.85rem; background: #161b22; border: 1px solid #21262d; border-radius: 6px; }
    .tools-field-label { font-size: 0.7rem; text-transform: uppercase; letter-spacing: 0.05em; color: #8b949e; margin-bottom: 0.35rem; }
    .tools-field-value { font-size: 0.8rem; color: #e6edf3; font-family: monospace; }
    .tools-copy-btn { background: none; border: 1px solid #30363d; border-radius: 4px; color: #8b949e; cursor: pointer; font-size: 0.7rem; padding: 0.15rem 0.5rem; flex-shrink: 0; }
    .tools-copy-btn:hover { color: #e6edf3; border-color: #8b949e; }

    .cloud-section { border-bottom: 1px solid #21262d; }

    .cloud-label {
      padding: 0.75rem 2rem 0;
      font-size: 0.7rem;
      text-transform: uppercase;
      letter-spacing: 0.05em;
      color: #8b949e;
    }

    .word-cloud {
      position: relative;
      overflow: hidden;
      margin: 0 2rem;
    }

    .word-cloud-word {
      position: absolute;
      background: none;
      border: none;
      cursor: pointer;
      font-family: monospace;
      padding: 0;
      line-height: 1;
      white-space: nowrap;
      transition: opacity 0.15s;
    }

    .word-cloud-word:hover { opacity: 0.6; }

    .prog-tag {
      font-size: 0.7rem;
      font-family: monospace;
      background: #1c2d3f;
      border: 1px solid #388bfd40;
      border-radius: 10px;
      padding: 0.1rem 0.5rem;
      color: #79c0ff;
    }
  </style>
</head>
<body>
  <header>
    <h1>Every Simplicity</h1>
    <p>A catalog of every SimplicityHL program found on the internet</p>
    <p class="disclaimer"><strong>Disclaimer:</strong> This is an independent, community project and is not officially endorsed by, affiliated with, or associated with Blockstream. Simplicity programs indexed here are sourced from public third-party repositories and have not been audited. This is not financial advice. Use at your own risk. See <a href="DISCLAIMER.md">DISCLAIMER.md</a> for full details.</p>
  </header>

  <div class="tabs">
    <button class="tab-btn active" data-tab="programs">Programs</button>
    <button class="tab-btn" data-tab="jets">Jets</button>
    <button class="tab-btn" data-tab="builtins">Built-ins</button>
    <button class="tab-btn" data-tab="types">Types</button>
    <button class="tab-btn" data-tab="macros">Macros</button>
    <button class="tab-btn" data-tab="reserved">Reserved Words</button>
    <button class="tab-btn" data-tab="tags">Tags</button>
    <button class="tab-btn" data-tab="tools">Tools</button>
  </div>

  <div id="tab-programs" class="tab-panel active">
    <div class="search-bar">
      <input type="search" id="search" placeholder="Search by name, CMR, jet, keyword…" autocomplete="off">
      <div style="display:flex;gap:0.5rem;margin-top:0.75rem;flex-wrap:wrap;">
        <button class="filter-btn active" data-filter="all">All</button>
        <button class="filter-btn" data-filter="compiled">✓ Compiled</button>
        <button class="filter-btn" data-filter="error">✗ Error</button>
        <button class="filter-btn" data-filter="uncompiled">! Not compiled</button>
      </div>
      <div class="count" id="count"></div>
    </div>
    <div class="programs" id="programs"></div>
    <div class="empty hidden" id="empty">No programs match your search.</div>
  </div>

  <div id="tab-jets" class="tab-panel">
    <div class="cloud-section">
      <div class="cloud-label">Word Cloud</div>
      <div id="jets-cloud" class="word-cloud"></div>
    </div>
    <div style="padding:1.25rem 2rem 0.5rem">
      <input type="search" id="jet-search" placeholder="Filter jets…" autocomplete="off"
        style="width:100%;max-width:320px;padding:0.5rem 0.75rem;background:#161b22;border:1px solid #30363d;border-radius:6px;color:#e6edf3;font-size:0.875rem;outline:none;">
    </div>
    <div style="padding:0 2rem 2rem">
      <table class="jets-table">
        <thead><tr><th>Jet</th><th></th><th>Scripts</th></tr></thead>
        <tbody id="jets-tbody"></tbody>
      </table>
    </div>
  </div>

  <div id="tab-builtins" class="tab-panel">
    <div class="cloud-section">
      <div class="cloud-label">Word Cloud</div>
      <div id="builtins-cloud" class="word-cloud"></div>
    </div>
    <div style="padding:1.25rem 2rem 0.5rem">
      <input type="search" id="builtins-search" placeholder="Filter built-ins…" autocomplete="off"
        style="width:100%;max-width:320px;padding:0.5rem 0.75rem;background:#161b22;border:1px solid #30363d;border-radius:6px;color:#e6edf3;font-size:0.875rem;outline:none;">
    </div>
    <div style="padding:0 2rem 2rem">
      <table class="jets-table">
        <thead><tr><th>Built-in</th><th></th><th>Scripts</th></tr></thead>
        <tbody id="builtins-tbody"></tbody>
      </table>
    </div>
  </div>

  <div id="tab-types" class="tab-panel">
    <div class="cloud-section">
      <div class="cloud-label">Word Cloud</div>
      <div id="types-cloud" class="word-cloud"></div>
    </div>
    <div style="padding:1.25rem 2rem 0.5rem">
      <input type="search" id="types-search" placeholder="Filter types…" autocomplete="off"
        style="width:100%;max-width:320px;padding:0.5rem 0.75rem;background:#161b22;border:1px solid #30363d;border-radius:6px;color:#e6edf3;font-size:0.875rem;outline:none;">
    </div>
    <div style="padding:0 2rem 2rem">
      <table class="jets-table">
        <thead><tr><th>Type</th><th></th><th>Scripts</th></tr></thead>
        <tbody id="types-tbody"></tbody>
      </table>
    </div>
  </div>

  <div id="tab-macros" class="tab-panel">
    <div class="cloud-section">
      <div class="cloud-label">Word Cloud</div>
      <div id="macros-cloud" class="word-cloud"></div>
    </div>
    <div style="padding:1.25rem 2rem 0.5rem">
      <input type="search" id="macros-search" placeholder="Filter macros…" autocomplete="off"
        style="width:100%;max-width:320px;padding:0.5rem 0.75rem;background:#161b22;border:1px solid #30363d;border-radius:6px;color:#e6edf3;font-size:0.875rem;outline:none;">
    </div>
    <div style="padding:0 2rem 2rem">
      <table class="jets-table">
        <thead><tr><th>Macro</th><th></th><th>Scripts</th></tr></thead>
        <tbody id="macros-tbody"></tbody>
      </table>
    </div>
  </div>

  <div id="tab-reserved" class="tab-panel">
    <div class="cloud-section">
      <div class="cloud-label">Word Cloud</div>
      <div id="reserved-cloud" class="word-cloud"></div>
    </div>
    <div style="padding:1.25rem 2rem 0.5rem">
      <input type="search" id="reserved-search" placeholder="Filter reserved words…" autocomplete="off"
        style="width:100%;max-width:320px;padding:0.5rem 0.75rem;background:#161b22;border:1px solid #30363d;border-radius:6px;color:#e6edf3;font-size:0.875rem;outline:none;">
    </div>
    <div style="padding:0 2rem 2rem">
      <table class="jets-table">
        <thead><tr><th>Word</th><th></th><th>Scripts</th></tr></thead>
        <tbody id="reserved-tbody"></tbody>
      </table>
    </div>
  </div>

  <div id="tab-tags" class="tab-panel">
    <div class="cloud-section">
      <div class="cloud-label">Word Cloud</div>
      <div id="tags-cloud" class="word-cloud"></div>
    </div>
    <div style="padding:1.25rem 2rem 0.5rem">
      <input type="search" id="tags-search" placeholder="Filter tags…" autocomplete="off"
        style="width:100%;max-width:320px;padding:0.5rem 0.75rem;background:#161b22;border:1px solid #30363d;border-radius:6px;color:#e6edf3;font-size:0.875rem;outline:none;">
    </div>
    <div style="padding:0 2rem 2rem">
      <table class="jets-table">
        <thead><tr><th>Tag</th><th></th><th>Scripts</th></tr></thead>
        <tbody id="tags-tbody"></tbody>
      </table>
    </div>
  </div>

  <div id="tab-tools" class="tab-panel">
    <div style="padding:1.5rem 2rem;max-width:860px">
      <h2 style="font-size:1rem;font-weight:600;color:#f0f6fc;margin-bottom:0.5rem">Canonicalize</h2>
      <p style="font-size:0.875rem;color:#8b949e;margin-bottom:1.25rem;line-height:1.6">
        Paste a compiled Simplicity program (base64, e.g. from a <code style="font-size:0.8rem">.simb</code> file)
        to obtain its canonical form — all embedded Word constants zeroed — giving a stable
        CMR and prefix shared by every instantiation of the same template.
      </p>
      <div id="tools-wasm-unavailable" style="display:none;margin-bottom:1rem;padding:0.6rem 0.85rem;background:#2d1b1b;border:1px solid #f8514940;border-radius:6px;font-size:0.8rem;color:#f85149">
        WASM module not available. Run the catalog command once to build and extract it automatically
        (the Docker image will rebuild on first run), then rebuild the site.
      </div>
      <textarea id="tools-source"
        style="width:100%;height:220px;background:#161b22;border:1px solid #30363d;border-radius:6px;color:#e6edf3;font-family:monospace;font-size:0.8rem;padding:0.75rem;resize:vertical;outline:none;line-height:1.5;"
        placeholder="Paste base64-encoded compiled program here…" spellcheck="false"></textarea>
      <div style="margin-top:0.75rem;display:flex;gap:0.75rem;align-items:center;flex-wrap:wrap">
        <button id="tools-btn"
          style="background:#1c2d3f;border:1px solid #388bfd;border-radius:6px;color:#58a6ff;cursor:pointer;font-size:0.875rem;padding:0.45rem 1.25rem;font-family:inherit"
          onclick="runCanonicalizer()">Canonicalize</button>
        <span id="tools-status" style="font-size:0.8rem;color:#8b949e"></span>
      </div>
      <div id="tools-output" style="margin-top:1.25rem;display:none;display:flex;flex-direction:column;gap:0.6rem">
        <div id="tools-error" style="display:none;padding:0.6rem 0.85rem;background:#2d1b1b;border:1px solid #f8514940;border-radius:6px;font-size:0.8rem;color:#f85149;font-family:monospace;white-space:pre-wrap"></div>
        <div id="tools-fields" style="display:none;display:flex;flex-direction:column;gap:0.6rem">
          <div class="tools-field">
            <div class="tools-field-label">Canonical program (base64)</div>
            <div style="display:flex;gap:0.5rem;align-items:flex-start">
              <code id="tools-program" class="tools-field-value" style="word-break:break-all;flex:1"></code>
              <button class="tools-copy-btn" onclick="toolsCopy('tools-program',this)">copy</button>
            </div>
          </div>
          <div class="tools-field">
            <div class="tools-field-label">CMR</div>
            <div style="display:flex;gap:0.5rem;align-items:center">
              <code id="tools-cmr" class="tools-field-value"></code>
              <button class="tools-copy-btn" onclick="toolsCopy('tools-cmr',this)">copy</button>
            </div>
          </div>
          <div class="tools-field">
            <div class="tools-field-label">Canonical prefix (hex)</div>
            <div style="display:flex;gap:0.5rem;align-items:center">
              <code id="tools-prefix-hex" class="tools-field-value"></code>
              <button class="tools-copy-btn" onclick="toolsCopy('tools-prefix-hex',this)">copy</button>
            </div>
          </div>
          <div class="tools-field">
            <div class="tools-field-label">Canonical prefix (base64)</div>
            <div style="display:flex;gap:0.5rem;align-items:center">
              <code id="tools-prefix-b64" class="tools-field-value"></code>
              <button class="tools-copy-btn" onclick="toolsCopy('tools-prefix-b64',this)">copy</button>
            </div>
          </div>
          <div id="tools-jets-field" class="tools-field" style="display:none">
            <div class="tools-field-label">Jets used</div>
            <div id="tools-jets" class="tools-field-value" style="display:flex;flex-wrap:wrap;gap:0.3rem;margin-top:0.25rem"></div>
          </div>
          <div id="tools-readable-field" class="tools-field" style="display:none">
            <div class="tools-field-label">Disassembly</div>
            <pre id="tools-readable" class="tools-field-value" style="white-space:pre;overflow-x:auto;margin:0;font-size:0.75rem;line-height:1.5;max-height:300px;overflow-y:auto"></pre>
          </div>
          <div id="tools-matches" style="display:none">
            <div class="tools-field-label" style="margin-bottom:0.4rem">Matching scripts</div>
            <div id="tools-matches-list" style="display:flex;flex-direction:column;gap:0.35rem"></div>
          </div>
        </div>
      </div>
    </div>
  </div>

  <footer>
    <strong style="color:#c9d1d9">Disclaimer:</strong>
    This is an independent, community project and is <strong style="color:#c9d1d9">not officially endorsed by, affiliated with, or associated with Blockstream</strong>.
    Programs indexed here are sourced from public third-party repositories and have not been audited by this project.
    This is not financial advice. Do not use these contracts with real funds without independent professional review. Use at your own risk.
  </footer>

  <div class="ext-modal" id="ext-modal">
    <div class="ext-modal-inner">
      <h2>External link</h2>
      <p>This link has not been verified and may be unsafe.</p>
      <code class="ext-modal-url" id="ext-modal-url"></code>
      <div class="ext-modal-actions">
        <button class="ext-btn-cancel" onclick="closeExtModal()">Cancel</button>
        <button class="ext-btn-proceed" id="ext-btn-proceed">Open anyway</button>
      </div>
    </div>
  </div>

  <script>
    const PROGRAMS = ${dataJson.replace(/<\/script>/gi, '<\\/script>')};

    // Pre-compute combined tag list on each program for the Tags tab index.
    for (const p of PROGRAMS) {
      p._all_tags = [...new Set([...(p.autodetected_tags || []), ...(p.manual_tags || [])])];
    }

    function tags(obj) {
      if (!obj) return [];
      return Object.entries(obj).map(([k, v]) => ({ name: k, count: v }));
    }

    function renderCard(p) {
      const jets = tags(p.jets);
      const builtins = tags(p.builtins);
      const macros = tags(p.macros);
      const words = tags(p.reserved_words);
      const types = tags(p.types);
      const params = p.params || [];
      const witnesses = p.witnesses || [];
      const comments = p.comments || [];
      const ptags = [...new Set([...(p.autodetected_tags || []), ...(p.manual_tags || [])])];
      const compileError = p.compile_error || null;

      const none = '<span style="color:#8b949e;font-size:0.75rem">none</span>';
      const tagHtml = (items) => items.length
        ? items.map(t => \`<span class="tag">\${escHtml(t.name)}<span class="count-badge">×\${t.count}</span></span>\`).join("")
        : none;

      const witnessSimbs = p._witness_simbs || [];
      const compiledHtml = (p._simb || witnessSimbs.length)
        ? \`<div style="margin-top:0.75rem;display:flex;flex-wrap:wrap;gap:0.5rem;align-items:center;">
            \${p._simb ? \`<a class="simb-link" href="\${escAttr(p._simb)}" download>↓ compiled (.simb)</a>\` : ""}
            \${witnessSimbs.map(w => \`<a class="simb-link" href="\${escAttr(w.path)}" download>↓ \${escHtml(w.name)} (.simb)</a>\`).join("")}
          </div>\`
        : "";

      const slugParts = p._slug.split("/");
      const orgRepo = slugParts.length >= 2 ? slugParts.slice(0, -1).join(" / ") : "";

      const compiled = !!(p.cmr || p._simb);
      const statusBadge = compileError
        ? \`<span title="\${escAttr(compileError)}" style="color:#f85149;font-size:1rem;line-height:1" aria-label="compile error">✗</span>\`
        : compiled
          ? \`<span title="compiled" style="color:#3fb950;font-size:1rem;line-height:1" aria-label="compiled">✓</span>\`
          : \`<span title="not yet compiled" style="color:#d29922;font-size:1rem;line-height:1" aria-label="not compiled">!</span>\`;

      return \`<div class="card" data-slug="\${escAttr(p._slug)}" data-search="\${escAttr(searchText(p))}">
        <div class="card-header">
          \${statusBadge}
          <span class="program-name">\${escHtml(p.name || p._slug)}</span>
          \${orgRepo ? \`<span style="font-size:0.75rem;color:#8b949e;font-family:monospace">\${escHtml(orgRepo)}</span>\` : ""}
          <a class="source-link ext-link" href="\${escAttr(p.url)}" data-url="\${escAttr(p.url)}">source ↗</a>
          \${ptags.map(t => \`<span class="prog-tag">\${escHtml(t)}</span>\`).join("")}
          \${p.cmr ? \`<span class="cmr" title="CMR">CMR: \${escHtml(p.cmr)}</span>\` : ""}
          \${p.canonical_cmr ? \`<span class="cmr" title="Canonical CMR — same for all programs sharing the same template">cCMR: \${escHtml(p.canonical_cmr)}</span>\` : ""}
          \${p.amr ? \`<span class="cmr" title="AMR">AMR: \${escHtml(p.amr)}</span>\` : ""}
          \${p.imr ? \`<span class="cmr" title="IMR">IMR: \${escHtml(p.imr)}</span>\` : ""}
          \${(params.length === 0 && p.program_prefix) ? \`<span class="cmr" title="First 8 bytes of compiled program (hex) — use to identify this program on-chain">prefix: \${escHtml(p.program_prefix)}</span>\` : ""}
          \${(params.length === 0 && p.program_prefix_b64) ? \`<span class="cmr" title="First 8 bytes of compiled program (base64)">prefix b64: \${escHtml(p.program_prefix_b64)}</span>\` : ""}
          \${p.canonical_prefix ? \`<span class="cmr" title="First 8 bytes of canonical program (hex) — same for all programs sharing the same template">cPrefix: \${escHtml(p.canonical_prefix)}</span>\` : ""}
          \${p.canonical_prefix_b64 ? \`<span class="cmr" title="First 8 bytes of canonical program (base64) — same for all programs sharing the same template">cPrefix b64: \${escHtml(p.canonical_prefix_b64)}</span>\` : ""}
          \${p.debug_cmr ? \`<span class="cmr" title="CMR of debug-symbol build">dbg CMR: \${escHtml(p.debug_cmr)}</span>\` : ""}
          \${p.debug_canonical_cmr ? \`<span class="cmr" title="Canonical CMR of debug-symbol build">dbg cCMR: \${escHtml(p.debug_canonical_cmr)}</span>\` : ""}
          \${p.debug_program_prefix ? \`<span class="cmr" title="First 8 bytes of debug-symbol program (hex)">dbg prefix: \${escHtml(p.debug_program_prefix)}</span>\` : ""}
          \${p.debug_program_prefix_b64 ? \`<span class="cmr" title="First 8 bytes of debug-symbol program (base64)">dbg prefix b64: \${escHtml(p.debug_program_prefix_b64)}</span>\` : ""}
          \${p.debug_canonical_prefix ? \`<span class="cmr" title="First 8 bytes of canonical debug-symbol program (hex)">dbg cPrefix: \${escHtml(p.debug_canonical_prefix)}</span>\` : ""}
          \${p.debug_canonical_prefix_b64 ? \`<span class="cmr" title="First 8 bytes of canonical debug-symbol program (base64)">dbg cPrefix b64: \${escHtml(p.debug_canonical_prefix_b64)}</span>\` : ""}
        </div>
        <div class="meta-grid">
          <div class="meta-section">
            <h3>Jets</h3>
            <div class="tag-list">\${tagHtml(jets)}</div>
          </div>
          <div class="meta-section">
            <h3>Built-ins</h3>
            <div class="tag-list">\${tagHtml(builtins)}</div>
          </div>
          <div class="meta-section">
            <h3>Types</h3>
            <div class="tag-list">\${tagHtml(types)}</div>
          </div>
          <div class="meta-section">
            <h3>Macros</h3>
            <div class="tag-list">\${tagHtml(macros)}</div>
          </div>
          <div class="meta-section">
            <h3>Reserved words</h3>
            <div class="tag-list">\${tagHtml(words)}</div>
          </div>
          <div class="meta-section">
            <h3>Params</h3>
            <div class="tag-list">\${params.length ? params.map(p => \`<span class="tag">\${escHtml(p)}</span>\`).join("") : none}</div>
          </div>
          <div class="meta-section">
            <h3>Witnesses</h3>
            <div class="tag-list">\${witnesses.length ? witnesses.map(w => \`<span class="tag">\${escHtml(w)}</span>\`).join("") : none}</div>
          </div>
        </div>
        \${compileError ? \`<div style="margin-top:0.75rem;padding:0.5rem 0.75rem;background:#2d1b1b;border:1px solid #f8514940;border-radius:4px;font-size:0.8rem;color:#f85149;font-family:monospace;line-height:1.5;white-space:pre-wrap;overflow-x:auto;">⚠ compile error: \${escHtml(compileError)}</div>\` : ""}
        \${comments.length ? \`<div style="margin-top:0.75rem;"><h3 style="font-size:0.7rem;text-transform:uppercase;letter-spacing:0.05em;color:#8b949e;margin-bottom:0.4rem;">Comments found in file</h3><div style="padding:0.6rem 0.75rem;background:#0d1117;border:1px solid #21262d;border-radius:4px;font-size:0.8rem;color:#8b949e;line-height:1.5;">\${comments.map(c => escHtml(c)).join('<br>')}</div></div>\` : ""}
        \${compiledHtml}
      </div>\`;
    }

    function searchText(p) {
      const parts = [
        p.name, p._slug, p.cmr, p.canonical_cmr,
        p.program_prefix, p.program_prefix_b64, p.canonical_prefix, p.canonical_prefix_b64,
        p.debug_cmr, p.debug_canonical_cmr,
        p.debug_program_prefix, p.debug_program_prefix_b64,
        p.debug_canonical_prefix, p.debug_canonical_prefix_b64,
        p.url,
        ...Object.keys(p.jets || {}),
        ...Object.keys(p.builtins || {}),
        ...Object.keys(p.macros || {}),
        ...Object.keys(p.types || {}),
        ...Object.keys(p.reserved_words || {}),
        ...(p.params || []),
        ...(p.witnesses || []),
        ...(p.autodetected_tags || []),
        ...(p.manual_tags || []),
        ...(p.comments || []),
        p.compile_error || "",
      ];
      return parts.filter(Boolean).join(" ").toLowerCase();
    }

    function escHtml(s) {
      return String(s)
        .replace(/&/g, "&amp;").replace(/</g, "&lt;")
        .replace(/>/g, "&gt;").replace(/"/g, "&quot;");
    }

    function escAttr(s) {
      return String(s || "").replace(/"/g, "&quot;");
    }

    const container = document.getElementById("programs");
    const emptyMsg = document.getElementById("empty");
    const countEl = document.getElementById("count");
    const searchEl = document.getElementById("search");

    container.innerHTML = PROGRAMS.map(renderCard).join("");
    const cards = Array.from(container.querySelectorAll(".card"));

    function updateCount(n) {
      countEl.textContent = n === PROGRAMS.length
        ? \`\${n} program\${n !== 1 ? "s" : ""}\`
        : \`\${n} of \${PROGRAMS.length} programs\`;
    }

    updateCount(PROGRAMS.length);

    let activeFilter = "all";

    function applyFilters() {
      const q = searchEl.value.toLowerCase().trim();
      let visible = 0;
      cards.forEach((card, i) => {
        const p = PROGRAMS[i];
        const compiled = !!(p.cmr || p._simb);
        const hasError = !!p.compile_error;
        const filterMatch =
          activeFilter === "all" ||
          (activeFilter === "compiled" && compiled && !hasError) ||
          (activeFilter === "error" && hasError) ||
          (activeFilter === "uncompiled" && !compiled && !hasError);
        const searchMatch = !q || card.dataset.search.includes(q);
        const show = filterMatch && searchMatch;
        card.classList.toggle("hidden", !show);
        if (show) visible++;
      });
      emptyMsg.classList.toggle("hidden", visible > 0);
      updateCount(visible);
    }

    searchEl.addEventListener("input", applyFilters);

    document.querySelectorAll(".filter-btn").forEach(btn => {
      btn.addEventListener("click", () => {
        document.querySelectorAll(".filter-btn").forEach(b => b.classList.remove("active"));
        btn.classList.add("active");
        activeFilter = btn.dataset.filter;
        applyFilters();
      });
    });

    // --- Tabs ---
    function activateTab(tabName, pushState) {
      document.querySelectorAll(".tab-btn").forEach(b => b.classList.remove("active"));
      document.querySelectorAll(".tab-panel").forEach(p => p.classList.remove("active"));
      const btn = document.querySelector(\`.tab-btn[data-tab="\${tabName}"]\`);
      const panel = document.getElementById("tab-" + tabName);
      if (!btn || !panel) return;
      btn.classList.add("active");
      panel.classList.add("active");
      const cloudEl = panel.querySelector('.word-cloud');
      if (cloudEl) _placeCloud(cloudEl.id);
      if (pushState) {
        history.pushState({ tab: tabName }, "", "#" + tabName);
      }
    }

    document.querySelectorAll(".tab-btn").forEach(btn => {
      btn.addEventListener("click", () => activateTab(btn.dataset.tab, true));
    });

    window.addEventListener("popstate", e => {
      const tab = (e.state && e.state.tab) || "programs";
      activateTab(tab, false);
    });

    // Initialise from URL hash on load, without pushing a new history entry.
    {
      const initial = location.hash.replace("#", "") || "programs";
      const valid = document.querySelector(\`.tab-btn[data-tab="\${initial}"]\`);
      activateTab(valid ? initial : "programs", false);
      history.replaceState({ tab: valid ? initial : "programs" }, "");
    }

    // --- Indexed tabs (Jets / Built-ins / Types / Macros) ---
    function buildIndex(field) {
      const idx = {};
      for (const p of PROGRAMS) {
        for (const key of Object.keys(p[field] || {})) {
          if (!idx[key]) idx[key] = [];
          idx[key].push(p);
        }
      }
      return Object.entries(idx).sort((a, b) => b[1].length - a[1].length);
    }

    function renderIndexRows(tbodyEl, rows) {
      const max = rows.length ? rows[0][1].length : 1;
      tbodyEl.innerHTML = rows.map(([label, progs]) => {
        const pct = Math.round(progs.length / max * 100);
        return \`
        <tr>
          <td>
            <button class="jet-row-toggle" aria-expanded="false" onclick="toggleJet(this)">
              <span class="jet-chevron">▶</span>
              <span style="font-family:monospace">\${escHtml(label)}</span>
            </button>
            <div class="jet-scripts">
              \${progs.map(p => \`<div class="jet-script-row">
                <button class="jet-script-link" onclick="goToProgram('\${escAttr(p._slug)}')">\${escHtml(p.name || p._slug)}</button>
                <a class="jet-script-ext" href="\${escAttr(p.url)}" target="_blank" rel="noopener" title="Open source">↗</a>
              </div>\`).join("")}
            </div>
          </td>
          <td class="bar-cell">
            <div class="bar-track"><div class="bar-fill" style="width:\${pct}%"></div></div>
          </td>
          <td style="padding-top:0.6rem;white-space:nowrap"><span class="badge">\${progs.length}</span></td>
        </tr>\`;
      }).join("");
    }

    window.toggleJet = function(btn) {
      const expanded = btn.getAttribute("aria-expanded") === "true";
      btn.setAttribute("aria-expanded", String(!expanded));
      btn.nextElementSibling.style.display = expanded ? "none" : "block";
    };

    window.goToProgram = function(slug) {
      activateTab("programs", true);

      const card = document.querySelector(\`.card[data-slug="\${CSS.escape(slug)}"]\`);
      if (!card) return;

      // Unhide in case a filter is active
      card.classList.remove("hidden");

      card.scrollIntoView({ behavior: "smooth", block: "center" });

      // Flash highlight
      card.classList.remove("card-highlight-fade");
      card.classList.add("card-highlight");
      setTimeout(() => {
        card.classList.replace("card-highlight", "card-highlight-fade");
        setTimeout(() => card.classList.remove("card-highlight-fade"), 900);
      }, 600);
    };

    // --- Word Cloud (spiral placement) ---
    const _cloudRegistry = {};

    function _wordHash(s) {
      let h = 5381;
      for (let i = 0; i < s.length; i++) h = ((h * 33) ^ s.charCodeAt(i)) >>> 0;
      return h;
    }

    function _placeCloud(cloudId) {
      const entry = _cloudRegistry[cloudId];
      if (!entry || entry.rendered) return;
      const { rows, searchId } = entry;
      const cloud = document.getElementById(cloudId);
      const W = cloud.offsetWidth;
      if (W < 10) return;
      const searchEl = document.getElementById(searchId);
      const counts = rows.map(([, p]) => p.length);
      const minC = Math.min(...counts), maxC = Math.max(...counts);
      const minPx = 11, maxPx = 42;
      const items = rows.map(([label, progs]) => {
        const c = progs.length;
        const t = maxC > minC ? (c - minC) / (maxC - minC) : 1;
        const px = Math.round(minPx + t * (maxPx - minPx));
        const hash = _wordHash(label);
        const hue = hash % 360;
        const sat = Math.round(40 + t * 40);
        const lit = Math.round(40 + t * 28);
        return { label, count: c, px, color: \`hsl(\${hue},\${sat}%,\${lit}%)\`, t };
      });
      const CHAR_W = 0.6, LINE_H = 1.3, PAD = 6;
      const placed = [];
      const out = [];
      const cx = W / 2;
      function fits(x, y, w, h) {
        if (x < 2 || x + w > W - 2) return false;
        for (const p of placed) {
          if (x < p.x + p.w + PAD && x + w + PAD > p.x &&
              y < p.y + p.h + PAD && y + h + PAD > p.y) return false;
        }
        return true;
      }
      for (const item of items) {
        const iw = item.label.length * item.px * CHAR_W;
        const ih = item.px * LINE_H;
        let ox = 0, oy = 0, ok = false;
        for (let step = 0; step < 1200 && !ok; step++) {
          const angle = step * 0.28;
          const r = step * 1.4;
          const tx = cx + Math.cos(angle) * r * 1.7 - iw / 2;
          const ty = Math.sin(angle) * r - ih / 2;
          if (fits(tx, ty, iw, ih)) { ox = tx; oy = ty; ok = true; }
        }
        if (ok) {
          placed.push({ x: ox, y: oy, w: iw, h: ih });
          out.push({ item, x: ox, y: oy });
        }
      }
      if (!out.length) return;
      const minY = Math.min(...out.map(o => o.y));
      const shift = 12 - minY;
      const maxY = Math.max(...out.map(o => o.y + o.item.px * LINE_H)) + shift + 12;
      cloud.style.height = Math.max(120, Math.ceil(maxY)) + 'px';
      cloud.innerHTML = out.map(({ item, x, y }) => \`<button class="word-cloud-word"
        style="left:\${x.toFixed(1)}px;top:\${(y+shift).toFixed(1)}px;font-size:\${item.px}px;color:\${item.color};font-weight:\${Math.round(400+item.t*300)}"
        data-word="\${escAttr(item.label)}"
        title="\${escAttr(item.label)}: \${item.count} script\${item.count !== 1 ? 's' : ''}"
      >\${escHtml(item.label)}</button>\`).join('');
      cloud.querySelectorAll('.word-cloud-word').forEach(btn => {
        btn.addEventListener('click', () => {
          searchEl.value = btn.dataset.word;
          searchEl.dispatchEvent(new Event('input'));
        });
      });
      entry.rendered = true;
    }

    function renderWordCloud(rows, cloudId, searchId) {
      const cloud = document.getElementById(cloudId);
      if (!rows.length) { cloud.closest('.cloud-section').style.display = 'none'; return; }
      _cloudRegistry[cloudId] = { rows, searchId, rendered: false };
    }

    function wireTab(field, tbodyId, searchId, cloudId) {
      const rows = buildIndex(field);
      const tbodyEl = document.getElementById(tbodyId);
      renderIndexRows(tbodyEl, rows);
      renderWordCloud(rows, cloudId, searchId);
      document.getElementById(searchId).addEventListener("input", function() {
        const q = this.value.toLowerCase().trim();
        renderIndexRows(tbodyEl, q ? rows.filter(([k]) => k.toLowerCase().includes(q)) : rows);
      });
    }

    function buildArrayIndex(field) {
      const idx = {};
      for (const p of PROGRAMS) {
        for (const key of (p[field] || [])) {
          if (!idx[key]) idx[key] = [];
          idx[key].push(p);
        }
      }
      return Object.entries(idx).sort((a, b) => b[1].length - a[1].length);
    }

    function wireArrayTab(field, tbodyId, searchId, cloudId) {
      const rows = buildArrayIndex(field);
      const tbodyEl = document.getElementById(tbodyId);
      renderIndexRows(tbodyEl, rows);
      renderWordCloud(rows, cloudId, searchId);
      document.getElementById(searchId).addEventListener("input", function() {
        const q = this.value.toLowerCase().trim();
        renderIndexRows(tbodyEl, q ? rows.filter(([k]) => k.toLowerCase().includes(q)) : rows);
      });
    }

    wireTab("jets",           "jets-tbody",     "jet-search",      "jets-cloud");
    wireTab("builtins",       "builtins-tbody", "builtins-search", "builtins-cloud");
    wireTab("types",          "types-tbody",    "types-search",    "types-cloud");
    wireTab("macros",         "macros-tbody",   "macros-search",   "macros-cloud");
    wireTab("reserved_words", "reserved-tbody", "reserved-search", "reserved-cloud");
    wireArrayTab("_all_tags", "tags-tbody",     "tags-search",     "tags-cloud");

    // --- External link modal ---
    window.openExtModal = function(event, anchor) {
      event.preventDefault();
      const url = anchor.dataset.url;
      const modal = document.getElementById("ext-modal");
      document.getElementById("ext-modal-url").textContent = url;
      const btn = document.getElementById("ext-btn-proceed");
      btn.onclick = function() {
        closeExtModal();
        window.open(url, "_blank", "noopener,noreferrer");
      };
      modal.classList.add("open");
    };

    window.closeExtModal = function() {
      document.getElementById("ext-modal").classList.remove("open");
    };

    document.getElementById("ext-modal").addEventListener("click", function(e) {
      if (e.target === this) closeExtModal();
    });

    document.addEventListener("keydown", e => {
      if (e.key === "Escape") { closeExtModal(); }
    });

    // Tools tab — canonicalize via WASM
    window.runCanonicalizer = function() {
      const source = document.getElementById("tools-source").value.trim();
      if (!source) return;
      const statusEl = document.getElementById("tools-status");
      const outputEl = document.getElementById("tools-output");
      const errorEl  = document.getElementById("tools-error");
      const fieldsEl = document.getElementById("tools-fields");

      if (!window._simc_canonicalize) {
        statusEl.textContent = "WASM not loaded yet — try again in a moment.";
        statusEl.style.color = "#f85149";
        return;
      }

      statusEl.textContent = "Compiling…";
      statusEl.style.color = "#8b949e";
      outputEl.style.display = "flex";
      errorEl.style.display = "none";
      fieldsEl.style.display = "none";
      document.getElementById("tools-matches").style.display = "none";
      document.getElementById("tools-jets-field").style.display = "none";
      document.getElementById("tools-readable-field").style.display = "none";

      // Run asynchronously so the browser can repaint the "Compiling…" state.
      setTimeout(() => {
        let result;
        try {
          result = JSON.parse(window._simc_canonicalize(source));
        } catch (e) {
          result = { ok: false, error: String(e) };
        }

        if (result.ok) {
          document.getElementById("tools-program").textContent = result.program;
          document.getElementById("tools-cmr").textContent = result.cmr;
          document.getElementById("tools-prefix-hex").textContent = result.canonical_prefix;
          document.getElementById("tools-prefix-b64").textContent = result.canonical_prefix_b64;

          const jets = result.jets || [];
          const jetsField = document.getElementById("tools-jets-field");
          if (jets.length > 0) {
            document.getElementById("tools-jets").innerHTML = jets
              .map(j => \`<span class="tag">\${escHtml(j)}</span>\`).join("");
            jetsField.style.display = "block";
          } else {
            jetsField.style.display = "none";
          }

          const readableField = document.getElementById("tools-readable-field");
          if (result.readable) {
            document.getElementById("tools-readable").textContent = result.readable;
            readableField.style.display = "block";
          } else {
            readableField.style.display = "none";
          }

          // Look up matching scripts by canonical CMR (exact) then canonical prefix (fallback).
          const cmrMatches = PROGRAMS.filter(p => p.canonical_cmr && p.canonical_cmr === result.cmr);
          const debugCmrMatches = cmrMatches.length === 0
            ? PROGRAMS.filter(p => p.debug_canonical_cmr && p.debug_canonical_cmr === result.cmr)
            : [];
          const prefixMatches = cmrMatches.length === 0 && debugCmrMatches.length === 0
            ? PROGRAMS.filter(p => p.canonical_prefix && p.canonical_prefix === result.canonical_prefix)
            : [];
          const debugPrefixMatches = cmrMatches.length === 0 && debugCmrMatches.length === 0 && prefixMatches.length === 0
            ? PROGRAMS.filter(p => p.debug_canonical_prefix && p.debug_canonical_prefix === result.canonical_prefix)
            : [];
          const matches = cmrMatches.length > 0
            ? cmrMatches.map(p => ({ p, how: "canonical CMR" }))
            : debugCmrMatches.length > 0
              ? debugCmrMatches.map(p => ({ p, how: "debug canonical CMR" }))
              : prefixMatches.length > 0
                ? prefixMatches.map(p => ({ p, how: "canonical prefix" }))
                : debugPrefixMatches.map(p => ({ p, how: "debug canonical prefix" }));

          const matchesEl = document.getElementById("tools-matches");
          const listEl = document.getElementById("tools-matches-list");
          if (matches.length > 0) {
            listEl.innerHTML = matches.map(({ p, how }) => {
              const name = escHtml(p.name || p._slug);
              const label = escHtml(how);
              const href = escAttr(p.url || "#");
              return \`<div style="display:flex;align-items:center;gap:0.5rem;padding:0.45rem 0.7rem;background:#161b22;border:1px solid #21262d;border-radius:6px;font-size:0.8rem">
                <span style="color:#3fb950;font-size:0.7rem">✓ \${label}</span>
                <a href="\${href}" target="_blank" rel="noopener" style="color:#58a6ff;text-decoration:none;font-family:monospace">\${name}</a>
                <span style="color:#8b949e;font-size:0.7rem">\${escHtml(p._slug)}</span>
              </div>\`;
            }).join("");
            matchesEl.style.display = "block";
          } else {
            listEl.innerHTML = \`<div style="padding:0.45rem 0.7rem;background:#161b22;border:1px solid #21262d;border-radius:6px;font-size:0.8rem;color:#8b949e">No matching scripts found in the catalog.</div>\`;
            matchesEl.style.display = "block";
          }

          fieldsEl.style.display = "flex";
          statusEl.textContent = "Done.";
          statusEl.style.color = "#3fb950";
        } else {
          errorEl.textContent = result.error || "Unknown error";
          errorEl.style.display = "block";
          statusEl.textContent = "Compile error.";
          statusEl.style.color = "#f85149";
        }
      }, 0);
    };

    window.toolsCopy = function(id, btn) {
      const text = document.getElementById(id).textContent;
      navigator.clipboard.writeText(text).then(() => {
        const orig = btn.textContent;
        btn.textContent = "copied!";
        setTimeout(() => { btn.textContent = orig; }, 1500);
      });
    };
  </script>

  <script type="module">
    // Load the WASM canonicalizer built from simplicityhl/wasm/.
    // Build: cd simplicityhl/wasm && wasm-pack build --target web --out-dir ../../every-simplicity/site/wasm
    try {
      const m = await import('./wasm/simplicityhl_wasm.js');
      await m.default('./wasm/simplicityhl_wasm_bg.wasm');
      window._simc_canonicalize = m.canonicalize;
      document.getElementById("tools-wasm-unavailable").style.display = "none";
    } catch (_) {
      document.getElementById("tools-wasm-unavailable").style.display = "block";
    }
  </script>
</body>
</html>`;
}

mkdirSync(OUT_DIR, { recursive: true });
const programs = loadPrograms().sort((a, b) =>
  (a.name || a._slug).localeCompare(b.name || b._slug)
);

// Copy .simb files to dist/, mirroring the data/programs/ subfolder structure
let simbCount = 0;
for (const p of programs) {
  if (p._simb && p._simbSrc) {
    const dest = join(OUT_DIR, p._simb);
    mkdirSync(dirname(dest), { recursive: true });
    copyFileSync(p._simbSrc, dest);
    simbCount++;
  }
  for (const w of (p._witnessSimbs || [])) {
    const dest = join(OUT_DIR, w.path);
    mkdirSync(dirname(dest), { recursive: true });
    copyFileSync(w.src, dest);
    simbCount++;
  }
  delete p._simbSrc;
  delete p._witnessSimbs;
}

// Copy wasm output files if present (built by: cd wasm && wasm-pack build --target web --out-dir ../../every-simplicity/site/wasm)
const WASM_SRC = join(__dirname, "wasm");
const WASM_DEST = join(OUT_DIR, "wasm");
let wasmCopied = false;
if (existsSync(WASM_SRC)) {
  mkdirSync(WASM_DEST, { recursive: true });
  for (const f of readdirSync(WASM_SRC)) {
    if (f.endsWith(".js") || f.endsWith(".wasm")) {
      copyFileSync(join(WASM_SRC, f), join(WASM_DEST, f));
    }
  }
  wasmCopied = true;
}

const html = buildHtml(programs);
writeFileSync(join(OUT_DIR, "index.html"), html, "utf8");
console.log(`Built dist/index.html with ${programs.length} program(s), ${simbCount} .simb file(s)${wasmCopied ? ", wasm module" : " (no wasm — run wasm-pack first)"}.`);
