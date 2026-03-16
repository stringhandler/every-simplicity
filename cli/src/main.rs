use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Utc;
use serde_json::Value as JsonValue;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use toml_edit::{Array, DocumentMut, Item, Table, Value};

const DOCKERFILE: &str = include_str!("../Dockerfile");
const ENTRYPOINT_SH: &str = include_str!("../entrypoint.sh");
const PARSE_PY: &str = include_str!("../parse.py");

/// FNV-1a hash of all Docker context files baked in at compile time.
/// Changes whenever Dockerfile, entrypoint.sh, or parse.py changes,
/// so the image is automatically rebuilt when the context is updated.
fn image_tag() -> String {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;
    let mut hash = FNV_OFFSET;
    for b in DOCKERFILE
        .bytes()
        .chain(ENTRYPOINT_SH.bytes())
        .chain(PARSE_PY.bytes())
    {
        hash ^= b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    format!("simplicity-catalog-simc:{:016x}", hash)
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(name = "simplicity-catalog", about = "Catalog .simplicityhl programs")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Add a .simplicityhl file to the catalog.
    ///
    /// Clones the repo in Docker, runs regex parsing for jets and reserved
    /// words, and writes a TOML entry. Does not run simc.
    Add {
        /// GitHub URL of the .simplicityhl file
        /// e.g. https://github.com/owner/repo/blob/master/path/to/file.simf
        url: String,

        /// Tag to associate with this entry (repeatable)
        #[arg(short, long = "tag", value_name = "TAG")]
        tags: Vec<String>,

        /// Example witness values in name=value format (repeatable)
        #[arg(long = "witness", short = 'w', value_name = "NAME=VALUE")]
        example_witnesses: Vec<String>,

        /// Example args values in name=value format (repeatable)
        #[arg(long = "args", short = 'a', value_name = "NAME=VALUE")]
        example_args: Vec<String>,

        /// Inline arg values as NAME=actual_value::Type (repeatable).
        /// Stored in [example_arg_values] and built into a JSON args file at compile time.
        #[arg(long = "arg-value", value_name = "NAME=VALUE::TYPE")]
        example_arg_values: Vec<String>,

        /// Overwrite the entry if it already exists
        #[arg(long, short)]
        force: bool,
    },

    /// Print the raw container output for a single file without writing anything.
    /// Useful for inspecting the real simc JSON schema.
    Debug {
        /// GitHub URL of the .simplicityhl file
        url: String,
    },

    /// Compile catalogued files with simc and update their metadata.
    ///
    /// Files from the same repo are cloned once and compiled in a single
    /// Docker container run.
    Compile {
        /// Recompile all entries
        #[arg(long, conflicts_with_all = ["tag", "slugs"])]
        all: bool,

        /// Recompile entries that have this tag
        #[arg(long, value_name = "TAG", conflicts_with_all = ["all", "slugs"])]
        tag: Option<String>,

        /// Recompile specific entries by slug
        #[arg(conflicts_with_all = ["all", "tag"])]
        slugs: Vec<String>,

    },
}

// ---------------------------------------------------------------------------
// GitHub URL parsing
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct GithubFile {
    owner: String,
    repo: String,
    branch: String,
    file_path: String,
    clone_url: String,
    original_url: String,
}

/// Fetch the latest commit SHA for a specific file via the GitHub API.
/// Does not clone the repo.
fn fetch_file_commit(gh: &GithubFile) -> Result<String> {
    let api_url = format!(
        "https://api.github.com/repos/{}/{}/commits?path={}&sha={}&per_page=1",
        gh.owner, gh.repo, gh.file_path, gh.branch
    );

    let response: JsonValue = ureq::get(&api_url)
        .set("User-Agent", "simplicity-catalog")
        .set("Accept", "application/vnd.github+json")
        .call()
        .context("GitHub API request failed")?
        .into_json()
        .context("failed to parse GitHub API response")?;

    response
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|commit| commit["sha"].as_str())
        .map(|s| s.to_string())
        .context("no commits found for this file — check the URL and branch")
}

fn parse_github_url(url: &str) -> Result<GithubFile> {
    let rest = url
        .strip_prefix("https://github.com/")
        .context("only https://github.com/ URLs are supported")?;

    let mut parts = rest.splitn(5, '/');
    let owner = parts.next().context("missing owner in URL")?;
    let repo = parts.next().context("missing repo in URL")?;
    let kind = parts.next().context("URL must contain 'blob'")?;
    let branch = parts.next().context("missing branch in URL")?;
    let file_path = parts.next().context("missing file path in URL")?;

    if kind != "blob" {
        bail!("URL must point to a file — use the 'blob' URL, not 'tree'");
    }

    Ok(GithubFile {
        clone_url: format!("https://github.com/{owner}/{repo}.git"),
        owner: owner.to_string(),
        repo: repo.to_string(),
        branch: branch.to_string(),
        file_path: file_path.to_string(),
        original_url: url.to_string(),
    })
}

fn script_name(gh: &GithubFile) -> String {
    gh.file_path
        .split('/')
        .last()
        .unwrap_or(&gh.file_path)
        .trim_end_matches(".simplicityhl")
        .trim_end_matches(".simf")
        .to_string()
}

/// Returns (org, repo, script_name)
fn entry_path_parts(gh: &GithubFile) -> (String, String, String) {
    (gh.owner.clone(), gh.repo.clone(), script_name(gh))
}

/// Output from compile mode — merged simc + hal-simplicity + parse.py output.
#[derive(Debug, Deserialize)]
struct SimcOutput {
    _file_path: String,
    /// "" for base, "wit" for witness, "args" for args compile
    #[serde(default)]
    _kind: String,
    /// Witness or args name; empty for base compile
    #[serde(default)]
    _item_name: String,
    // --- simc ---
    /// Base64-encoded compiled program
    #[serde(default)]
    program: Option<String>,
    /// Witness value (may be null)
    #[serde(default)]
    witness: Option<serde_json::Value>,
    // --- hal-simplicity ---
    #[serde(default)]
    cmr: Option<String>,
    /// Type signature e.g. "1 → 1"
    #[serde(default)]
    type_arrow: Option<String>,
    #[serde(default)]
    liquid_address_unconf: Option<String>,
    #[serde(default)]
    liquid_testnet_address_unconf: Option<String>,
    /// Jet set name from simc/hal e.g. "core" (renamed from 'jets' to avoid clash)
    #[serde(default)]
    jet_set: Option<String>,
    // --- parse.py ---
    #[serde(default)]
    jets: BTreeMap<String, u64>,
    #[serde(default)]
    builtins: BTreeMap<String, u64>,
    #[serde(default)]
    reserved_words: BTreeMap<String, u64>,
    #[serde(default)]
    types: BTreeMap<String, u64>,
    #[serde(default)]
    macros: BTreeMap<String, u64>,
    #[serde(default)]
    witnesses: Vec<String>,
    #[serde(default)]
    params: Vec<String>,
    #[serde(default)]
    comments: Vec<String>,
    /// Compile error from simc, if compilation failed.
    #[serde(default)]
    _error: Option<String>,
    /// Mermaid graph source from `hal-simplicity simplicity graph --format mermaid`
    #[serde(default)]
    _mermaid: String,
}

/// Fields we need to read back from an existing TOML to run Docker.
#[derive(Debug, Deserialize)]
struct EntryMeta {
    clone_url: Option<String>,
    repo: Option<String>,
    branch: String,
    file_path: String,
    /// Witness map: name → value, stored as `[example_witnesses]` table in TOML.
    #[serde(default)]
    example_witnesses: BTreeMap<String, String>,
    /// Args map: name → value, stored as `[example_args]` table in TOML.
    #[serde(default)]
    example_args: BTreeMap<String, String>,
    /// Inline arg values: param_name → "actual_value::Type", stored as `[example_arg_values]`.
    #[serde(default)]
    example_arg_values: BTreeMap<String, String>,
    #[serde(default)]
    tags: Vec<String>,
}

// ---------------------------------------------------------------------------
// Docker
// ---------------------------------------------------------------------------

fn ensure_docker_image(tag: &str) -> Result<()> {
    let check = Command::new("docker")
        .args(["image", "inspect", tag])
        .output()
        .context("failed to run docker — is it installed and running?")?;

    if check.status.success() {
        return Ok(());
    }

    println!("Building Docker image '{tag}' …");

    let tmp = std::env::temp_dir().join("simplicity-catalog-docker-build");
    fs::create_dir_all(&tmp)?;
    fs::write(tmp.join("Dockerfile"), DOCKERFILE)?;
    fs::write(tmp.join("entrypoint.sh"), ENTRYPOINT_SH)?;
    fs::write(tmp.join("parse.py"), PARSE_PY)?;

    let status = Command::new("docker")
        .args(["build", "-t", tag, "."])
        .current_dir(&tmp)
        .status()
        .context("failed to run docker build")?;

    if !status.success() {
        bail!("docker build failed");
    }

    Ok(())
}

fn run_docker<T>(
    mode: &str,
    clone_url: &str,
    branch: &str,
    file_paths: &[&str],
) -> Result<Vec<T>>
where
    T: serde::de::DeserializeOwned,
{
    let tag = image_tag();
    ensure_docker_image(&tag)?;

    let output = Command::new("docker")
        .arg("run")
        .arg("--rm")
        .arg(&tag)
        .arg(mode)
        .arg(clone_url)
        .arg(branch)
        .args(file_paths)
        .output()
        .context("failed to run docker container")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("container exited with error:\n{stderr}");
    }

    let stdout = String::from_utf8(output.stdout).context("container output is not valid UTF-8")?;

    stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            serde_json::from_str(line)
                .with_context(|| format!("failed to parse JSON from container output:\n{line}"))
        })
        .collect()
}

// ---------------------------------------------------------------------------
// TOML helpers
// ---------------------------------------------------------------------------

fn data_dir() -> PathBuf {
    let mut dir = std::env::current_dir().unwrap_or_default();
    loop {
        let candidate = dir.join("data/programs");
        if candidate.is_dir() {
            return candidate;
        }
        if !dir.pop() {
            break;
        }
    }
    PathBuf::from("data/programs")
}

/// Build a JSON args file from inline `name → "value::Type"` pairs.
///
/// Each entry becomes `{ "NAME": { "value": "...", "type": "..." } }`.
/// Values are split on the **last** `::` so hex values (which contain `::`)
/// are handled correctly.
fn build_arg_values_json(arg_values: &BTreeMap<String, String>) -> Result<String> {
    let mut map = serde_json::Map::new();
    for (name, raw) in arg_values {
        let (value, type_name) = raw.rsplit_once("::").with_context(|| {
            format!("--arg-value '{name}={raw}' must be in format 'value::Type'")
        })?;
        let mut obj = serde_json::Map::new();
        obj.insert("value".to_string(), JsonValue::String(value.to_string()));
        obj.insert("type".to_string(), JsonValue::String(type_name.to_string()));
        map.insert(name.clone(), JsonValue::Object(obj));
    }
    Ok(serde_json::to_string(&JsonValue::Object(map))?)
}

fn write_skeleton_toml(
    path: &Path,
    gh: &GithubFile,
    tags: &[String],
    commit: &str,
    example_witnesses: &[(String, String)],
    example_args: &[(String, String)],
    example_arg_values: &[(String, String)],
) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut doc = DocumentMut::new();
    let sv = |s: &str| Item::Value(Value::from(s));

    doc["url"] = sv(&gh.original_url);
    doc["name"] = sv(gh
        .file_path
        .split('/')
        .last()
        .unwrap_or(&gh.file_path)
        .trim_end_matches(".simplicityhl")
        .trim_end_matches(".simf"));
    doc["repo"] = sv(&gh.clone_url);
    doc["branch"] = sv(&gh.branch);
    doc["file_path"] = sv(&gh.file_path);
    doc["commit"] = sv(commit);
    doc["fetched_at"] = sv(&Utc::now().to_rfc3339());

    if !example_witnesses.is_empty() {
        let mut t = Table::new();
        for (name, value) in example_witnesses {
            t[name.as_str()] = Item::Value(Value::from(value.as_str()));
        }
        doc["example_witnesses"] = Item::Table(t);
    }

    if !example_args.is_empty() {
        let mut t = Table::new();
        for (name, value) in example_args {
            t[name.as_str()] = Item::Value(Value::from(value.as_str()));
        }
        doc["example_args"] = Item::Table(t);
    }

    if !example_arg_values.is_empty() {
        let mut t = Table::new();
        for (name, value) in example_arg_values {
            t[name.as_str()] = Item::Value(Value::from(value.as_str()));
        }
        doc["example_arg_values"] = Item::Table(t);
    }

    if !tags.is_empty() {
        let mut arr = Array::new();
        for t in tags {
            arr.push(t.as_str());
        }
        doc["tags"] = Item::Value(Value::Array(arr));
    }

    fs::write(path, doc.to_string()).with_context(|| format!("failed to write {}", path.display()))
}

/// Write a compiled program to `simb_path` and, if mermaid content is present,
/// write `<simb_path>.mermaid` alongside it.
fn write_simb(simb_path: &Path, result: &SimcOutput) -> Result<()> {
    if let Some(program) = &result.program {
        fs::write(simb_path, program)?;
        if !result._mermaid.is_empty() {
            let mut mermaid_path = simb_path.as_os_str().to_owned();
            mermaid_path.push(".mermaid");
            fs::write(PathBuf::from(mermaid_path), &result._mermaid)?;
        }
    }
    Ok(())
}

fn apply_counts(doc: &mut DocumentMut, key: &str, counts: &BTreeMap<String, u64>) {
    // Always clear any stale value first; re-write only if non-empty.
    doc.remove(key);
    if counts.is_empty() {
        return;
    }
    let mut t = Table::new();
    for (k, v) in counts {
        t[k] = Item::Value(Value::from(*v as i64));
    }
    doc[key] = Item::Table(t);
}

fn apply_simc_to_toml(doc: &mut DocumentMut, s: &SimcOutput) {
    let sv = |v: &str| Item::Value(Value::from(v));

    // program is written to a separate .simb file, not the TOML
    if let Some(cmr) = &s.cmr {
        doc["cmr"] = sv(cmr);
    }
    if let Some(ta) = &s.type_arrow {
        doc["type_arrow"] = sv(ta);
    }
    if let Some(js) = &s.jet_set {
        doc["jet_set"] = sv(js);
    }

    if let Some(la) = &s.liquid_address_unconf {
        doc["liquid_address"] = sv(la);
    }
    if let Some(lta) = &s.liquid_testnet_address_unconf {
        doc["liquid_testnet_address"] = sv(lta);
    }
    if let Some(witness) = &s.witness {
        if !witness.is_null() {
            doc["witness"] = sv(&witness.to_string());
        }
    }

    // Write or clear compile error
    doc.remove("compile_error");
    if let Some(err) = &s._error {
        if !err.is_empty() {
            doc["compile_error"] = sv(err);
        }
    }

    doc.remove("witnesses");
    if !s.witnesses.is_empty() {
        let mut arr = Array::new();
        for w in &s.witnesses {
            arr.push(w.as_str());
        }
        doc["witnesses"] = Item::Value(Value::Array(arr));
    }

    doc.remove("params");
    if !s.params.is_empty() {
        let mut arr = Array::new();
        for p in &s.params {
            arr.push(p.as_str());
        }
        doc["params"] = Item::Value(Value::Array(arr));
    }

    doc.remove("comments");
    if !s.comments.is_empty() {
        let mut arr = Array::new();
        for c in &s.comments {
            arr.push(c.as_str());
        }
        doc["comments"] = Item::Value(Value::Array(arr));
    }

    apply_counts(doc, "jets", &s.jets);
    apply_counts(doc, "builtins", &s.builtins);
    apply_counts(doc, "macros", &s.macros);
    apply_counts(doc, "reserved_words", &s.reserved_words);
    apply_counts(doc, "types", &s.types);
}

// ---------------------------------------------------------------------------
// Shared: group TOMLs by repo and run Docker for each group
// ---------------------------------------------------------------------------

/// Recursively collect all .toml files under `dir`.
fn collect_tomls(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            out.extend(collect_tomls(&path));
        } else if path.extension().and_then(|e| e.to_str()) == Some("toml") {
            out.push(path);
        }
    }
    out
}

/// Returns a `org/repo/name` identifier for a TOML path rooted at `data_dir`.
fn relative_id(path: &Path, data_dir: &Path) -> String {
    path.strip_prefix(data_dir)
        .unwrap_or(path)
        .with_extension("")
        .to_string_lossy()
        .replace('\\', "/")
}

fn select_tomls(
    dir: &Path,
    all: bool,
    tag: Option<&str>,
    slugs: &[String],
) -> Result<Vec<PathBuf>> {
    let paths = collect_tomls(dir);

    let selected: Vec<PathBuf> = paths
        .into_iter()
        .filter(|path| {
            if all {
                return true;
            }
            // Match by org/repo/name or just name
            if !slugs.is_empty() {
                let id = relative_id(path, dir);
                let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                return slugs.iter().any(|s| s == &id || s == name);
            }
            if let Some(filter_tag) = tag {
                if let Ok(raw) = fs::read_to_string(path) {
                    if let Ok(meta) = toml_edit::de::from_str::<EntryMeta>(&raw) {
                        return meta.tags.iter().any(|t| t == filter_tag);
                    }
                }
            }
            false
        })
        .collect();

    if selected.is_empty() {
        bail!("no entries matched the selector");
    }
    Ok(selected)
}

/// `(toml_path, file_path, witnesses, args, arg_values)` — name→value maps
type RepoGroup = BTreeMap<
    (String, String),
    Vec<(
        PathBuf,
        String,
        BTreeMap<String, String>,
        BTreeMap<String, String>,
        BTreeMap<String, String>,
    )>,
>;

fn group_by_repo(paths: &[PathBuf]) -> Result<RepoGroup> {
    let mut groups: RepoGroup = BTreeMap::new();
    for path in paths {
        let raw =
            fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
        let meta: EntryMeta = toml_edit::de::from_str(&raw)
            .with_context(|| format!("cannot parse {}", path.display()))?;
        let clone_url = meta.clone_url.or(meta.repo).unwrap_or_default();
        groups.entry((clone_url, meta.branch)).or_default().push((
            path.to_owned(),
            meta.file_path,
            meta.example_witnesses,
            meta.example_args,
            meta.example_arg_values,
        ));
    }
    Ok(groups)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn cmd_add(
    url: &str,
    tags: &[String],
    example_witnesses: &[(String, String)],
    example_args: &[(String, String)],
    example_arg_values: &[(String, String)],
    force: bool,
) -> Result<()> {
    let gh = parse_github_url(url)?;
    let (org, repo, name) = entry_path_parts(&gh);
    let out_path = data_dir()
        .join(&org)
        .join(&repo)
        .join(format!("{name}.toml"));

    if out_path.exists() && !force {
        bail!(
            "Entry already exists: {}\nUse --force to overwrite.",
            out_path.display()
        );
    }

    print!("Fetching commit for {} … ", gh.file_path);
    let commit = fetch_file_commit(&gh)?;
    println!("{}", &commit[..12]);

    write_skeleton_toml(
        &out_path,
        &gh,
        tags,
        &commit,
        example_witnesses,
        example_args,
        example_arg_values,
    )?;

    println!("Added:  data/programs/{org}/{repo}/{name}.toml");
    if !tags.is_empty() {
        println!("Tags:   {}", tags.join(", "));
    }
    println!("Run `simplicity-catalog compile {org}/{repo}/{name}` to compile.");
    Ok(())
}

fn cmd_compile(all: bool, tag: Option<&str>, slugs: &[String]) -> Result<()> {
    if !all && tag.is_none() && slugs.is_empty() {
        bail!("specify --all, --tag <tag>, or one or more slugs");
    }

    let dir = data_dir();
    let selected = select_tomls(&dir, all, tag, slugs)?;
    println!("Compiling {} entry/entries …", selected.len());

    let groups = group_by_repo(&selected)?;
    let mut compiled = 0usize;
    let mut failed = 0usize;

    for ((clone_url, branch), entries) in &groups {
        // Encoding: "file"  |  "file:::wit:::name:::value"  |  "file:::args:::name:::value"
        let mut encoded: Vec<String> = Vec::new();
        for (_, fp, witnesses, args, arg_values) in entries.iter() {
            // Only do a base (no-args) compile if the program has no example_args/arg_values.
            // Programs that require args cannot be compiled without them.
            if args.is_empty() && arg_values.is_empty() {
                encoded.push(fp.clone());
            }
            for (name, value) in witnesses {
                encoded.push(format!("{fp}:::wit:::{name}:::{value}"));
            }
            for (name, value) in args {
                encoded.push(format!("{fp}:::args:::{name}:::{value}"));
            }
            if !arg_values.is_empty() {
                let json = build_arg_values_json(arg_values)?;
                encoded.push(format!("{fp}:::args:::default:::{json}"));
            }
        }
        let file_args: Vec<&str> = encoded.iter().map(|s| s.as_str()).collect();

        let results: Vec<SimcOutput> =
            match run_docker("compile", clone_url, branch, &file_args) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("error compiling {clone_url}: {e:#}");
                    failed += entries.len();
                    continue;
                }
            };

        for (toml_path, file_path, witnesses, args, arg_values) in entries {
            let stem = toml_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("?")
                .to_string();

            // Base compile result: _kind == ""
            // For args-only programs, fall back to the first args result for TOML metadata.
            let base_result = results
                .iter()
                .find(|r| &r._file_path == file_path && r._kind.is_empty());

            let meta_result = base_result.or_else(|| {
                if !args.is_empty() || !arg_values.is_empty() {
                    results
                        .iter()
                        .find(|r| &r._file_path == file_path && r._kind == "args")
                } else {
                    None
                }
            });

            match meta_result {
                None => {
                    eprintln!("warning: no simc result for {file_path}");
                    failed += 1;
                }
                Some(simc_out) => {
                    let raw = fs::read_to_string(toml_path)?;
                    let mut doc: DocumentMut = raw.parse()?;
                    apply_simc_to_toml(&mut doc, simc_out);
                    fs::write(toml_path, doc.to_string())?;

                    // Only write a base .simb for programs that don't require args
                    if simc_out._kind.is_empty() {
                        write_simb(&toml_path.with_extension("simb"), simc_out)?;
                    }
                    println!("  compiled: {stem}");
                    compiled += 1;
                }
            }

            // Per-witness: <name>.<witness_name>.simb
            for (item_name, _) in witnesses {
                if let Some(r) = results.iter().find(|r| {
                    &r._file_path == file_path && r._kind == "wit" && &r._item_name == item_name
                }) {
                    write_simb(
                        &toml_path.with_file_name(format!("{stem}.{item_name}.simb")),
                        r,
                    )?;
                    println!("  compiled witness: {stem}.{item_name}");
                } else {
                    eprintln!("warning: no witness result for {file_path} (wit: {item_name})");
                }
            }

            // Per-args (URL-based): <name>.args.<args_name>.simb
            for (item_name, _) in args {
                if let Some(r) = results.iter().find(|r| {
                    &r._file_path == file_path && r._kind == "args" && &r._item_name == item_name
                }) {
                    write_simb(
                        &toml_path.with_file_name(format!("{stem}.args.{item_name}.simb")),
                        r,
                    )?;
                    println!("  compiled args: {stem}.args.{item_name}");
                } else {
                    eprintln!("warning: no args result for {file_path} (args: {item_name})");
                }
            }

            // Per-arg-values (inline): always keyed as "default" → <name>.args.default.simb
            if !arg_values.is_empty() {
                if let Some(r) = results.iter().find(|r| {
                    &r._file_path == file_path && r._kind == "args" && r._item_name == "default"
                }) {
                    write_simb(
                        &toml_path.with_file_name(format!("{stem}.args.default.simb")),
                        r,
                    )?;
                    println!("  compiled arg-values: {stem}.args.default");
                } else {
                    eprintln!("warning: no arg-values result for {file_path}");
                }
            }
        }
    }

    println!("\nDone: {compiled} compiled, {failed} failed.");
    Ok(())
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn cmd_debug(url: &str) -> Result<()> {
    let gh = parse_github_url(url)?;
    let tag = image_tag();
    ensure_docker_image(&tag)?;
    let status = Command::new("docker")
        .arg("run")
        .arg("--rm")
        .arg(&tag)
        .arg("compile")
        .arg(&gh.clone_url)
        .arg(&gh.branch)
        .arg(&gh.file_path)
        .status()
        .context("failed to run docker container")?;

    if !status.success() {
        bail!("container exited with non-zero status");
    }
    Ok(())
}

fn main() {
    // Suppress broken pipe errors (e.g. container stdout closing unexpectedly)
    std::panic::set_hook(Box::new(|info| {
        if let Some(s) = info.payload().downcast_ref::<String>() {
            if s.contains("Broken pipe") {
                std::process::exit(0);
            }
        }
        eprintln!("{info}");
    }));

    let cli = Cli::parse();
    let result = match cli.command {
        Commands::Add {
            url,
            tags,
            example_witnesses,
            example_args,
            example_arg_values,
            force,
        } => {
            let parse_pairs = |v: Vec<String>| -> Vec<(String, String)> {
                v.into_iter()
                    .map(|s| {
                        if let Some((name, value)) = s.split_once('=') {
                            (name.to_string(), value.to_string())
                        } else {
                            ("default".to_string(), s)
                        }
                    })
                    .collect()
            };
            let witnesses = parse_pairs(example_witnesses);
            let args = parse_pairs(example_args);
            let arg_values = parse_pairs(example_arg_values);
            cmd_add(&url, &tags, &witnesses, &args, &arg_values, force)
        }
        Commands::Debug { url } => cmd_debug(&url),
        Commands::Compile { all, tag, slugs } => cmd_compile(all, tag.as_deref(), &slugs),
    };
    if let Err(e) = result {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}
