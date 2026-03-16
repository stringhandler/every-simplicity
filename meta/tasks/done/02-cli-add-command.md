# Task: CLI — `simplicity-catalog add <url>`

Build the local CLI tool that generates a TOML entry for a given `.simplicityhl` URL.

## Steps

1. Fetch the file at the given URL
2. Run `simc` on it to extract:
   - CMR
   - Jets used (with counts)
   - Reserved words used (with counts)
   - Parameters (if any)
   - Witness values (if any)
3. If the program has no parameters, also capture the compiled output from `simc`
4. Write a new `.toml` file under `data/programs/` named after a slug derived from the URL

## Output TOML shape (draft)

```toml
url = "https://..."
name = "example"
cmr = "abc123..."

[jets]
sha256 = 3
eq = 1

[reserved_words]
match = 4
fn = 2

[[parameters]]
name = "foo"
type = "u32"

[[witnesses]]
name = "bar"
type = "bool"

compiled = "..." # only present if no parameters
```
