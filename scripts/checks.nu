#!/usr/bin/env nix
#!nix develop .#ci --command nu

# Run all linters and formatters.
def main []: nothing -> nothing {
    let markdown: list<string> = files "*.md"
    let scripts: list<string> = files "*.nu"
    let nix: list<string> = files "*.nix"

    # Git
    committed origin/main..HEAD

    # GitHub
    zizmor --pedantic .github

    # Spellchecking
    typos

    # Markdown
    lychee --verbose .

    let alerts = vale --no-exit --output=JSON ...$markdown | from json
    if ($alerts | is-not-empty) {
        vale ...$markdown
        exit 1
    }

    # TOML
    tombi lint --error-on-warnings

    # Nushell
    nufmt --dry-run ...$scripts
    nu-lint --config .nu-lint.toml ...$scripts

    # Nix
    nixfmt --check --width=120 ...$nix
    deadnix --fail .

    # Rust
    cargo fmt --all --check
    cargo shear --locked
    cargo deny check --deny warnings
    cargo clippy --locked --all-targets
    cargo build --locked --all-targets
}

def files [pattern: string]: nothing -> list<string> {
    git ls-files --cached --others --exclude-standard $pattern | lines
}
