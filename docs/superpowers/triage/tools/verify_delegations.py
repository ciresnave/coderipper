#!/usr/bin/env python3
"""Check each `delegated` triage claim against the tool's own primary source. Read-only; installs nothing.

A claim is "tool T evidences rule R through its check C" (C is a lint name, a rule code, a subcommand or a flag). The agents who
proposed them worked from memory. This script fetches the tool's own documentation or source (a file in its GitHub repository,
a file in its npm package on a CDN, a documentation page, or the output of the locally installed rustc/clippy/rustfmt) and
looks for C in it. `confirmed` means C appears in that source; it does NOT mean the tool covers the rule well (that is the
cell's `confidence`, a judgement). `NOT-FOUND` and `fetch-failed` are results too: they are never hidden.

Usage: python verify_delegations.py OUT_TSV
Standard library only. GitHub is read through `gh api` (read-only GET, the logged-in account); everything else by plain HTTP GET.
"""
import base64
import csv
import json
import subprocess
import sys
import urllib.request

UA = "coderipper-triage/0.1 (read-only; https://github.com/ciresnave/coderipper)"
_cache = {}


def run(cmd):
    p = subprocess.run(cmd, capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=120)
    return (p.stdout or "") + (p.stderr or "")


def http(url):
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read().decode("utf-8", errors="replace")


def gh_file(repo, path):
    api = f"repos/{repo}/readme" if path == "README" else f"repos/{repo}/contents/{path}"
    p = subprocess.run(["gh", "api", api], capture_output=True, text=True, encoding="utf-8", errors="replace", timeout=120)
    if p.returncode != 0:
        raise RuntimeError((p.stderr or "gh failed").strip().splitlines()[-1][:100])
    doc = json.loads(p.stdout)
    if isinstance(doc, list):  # a directory: its file names are the content
        return "\n".join(x["name"] for x in doc)
    return base64.b64decode(doc["content"]).decode("utf-8", errors="replace")


def fetch(src):
    if src in _cache:
        return _cache[src]
    kind, _, rest = src.partition(":")
    if src.startswith("http"):
        text = http(src)
    elif kind == "local":
        text = {
            "rustc-lints": lambda: run(["rustc", "-W", "help"]),
            "clippy-lints": lambda: run(["cargo", "clippy", "--", "-W", "help"]),
            "cargo-fmt-help": lambda: run(["cargo", "fmt", "--help"]),
            "rustc-version": lambda: run(["rustc", "--version"]) + run(["cargo", "clippy", "--version"]),
        }[rest]()
    elif kind == "gh":
        repo, _, path = rest.partition("::")
        text = gh_file(repo, path)
    else:
        raise ValueError(f"unknown source {src!r}")
    _cache[src] = text
    return text


# (language, rule id) -> [(source, needle), ...]. Each needle is what the claim says the tool provides.
G = "gh:"
CLAIMS = {
    # ---- Rust (local toolchain for lints; tools' repositories for the rest)
    ("rust", "MOD-001"): [("local:rustc-lints", "unreachable-pub")],
    ("rust", "MOD-002"): [(G + "regexident/cargo-modules::README", "acyclic")],
    ("rust", "EXT-001"): [("local:clippy-lints", "wildcard_enum_match_arm")],
    ("rust", "EXT-003"): [("local:clippy-lints", "unimplemented")],
    ("rust", "RDB-001"): [("local:cargo-fmt-help", "--check")],
    ("rust", "RDB-002"): [("local:rustc-lints", "non-snake-case")],
    ("rust", "RDB-003"): [("local:clippy-lints", "cognitive_complexity")],
    ("rust", "RDB-004"): [("local:clippy-lints", "too_many_arguments")],
    ("rust", "RDB-005"): [(G + "kucherenko/jscpd::README", "Rust")],
    ("rust", "RDB-006"): [("local:rustc-lints", "dead-code")],
    ("rust", "TYP-006"): [("local:clippy-lints", "unwrap_used")],
    ("rust", "TYP-007"): [("local:clippy-lints", "fn_params_excessive_bools")],
    ("rust", "ERR-001"): [("local:clippy-lints", "let_underscore_must_use")],
    ("rust", "ERR-003"): [("local:clippy-lints", "indexing_slicing")],
    ("rust", "ERR-011"): [("local:rustc-lints", "unused-must-use")],
    ("rust", "COR-001"): [("local:clippy-lints", "missing_errors_doc")],
    ("rust", "COR-006"): [(G + "rust-lang/miri::README", "undefined behavior")],
    ("rust", "COR-007"): [("local:clippy-lints", "float_cmp")],
    ("rust", "SEC-001"): [(G + "semgrep/semgrep::README", "Rust")],
    ("rust", "SEC-005"): [(G + "geiger-rs/cargo-geiger::README", "unsafe")],
    ("rust", "SEC-006"): [(G + "rustsec/rustsec::cargo-audit/README.md", "advisor")],
    ("rust", "SUP-002"): [(G + "rustsec/rustsec::cargo-audit/README.md", "advisor")],
    ("rust", "SUP-003"): [(G + "EmbarkStudios/cargo-deny::README", "licenses")],
    ("rust", "SUP-004"): [(G + "EmbarkStudios/cargo-deny::README", "sources")],
    ("rust", "SUP-005"): [(G + "EmbarkStudios/cargo-deny::README", "bans")],
    ("rust", "SUP-006"): [(G + "bnjbvr/cargo-machete::README", "unused dependencies")],
    ("rust", "API-001"): [(G + "obi1kenobi/cargo-semver-checks::README", "semver")],
    ("rust", "API-002"): [(G + "cargo-public-api/cargo-public-api::README", "public API")],
    ("rust", "API-003"): [(G + "obi1kenobi/cargo-semver-checks::README", "breaking")],
    ("rust", "API-007"): [(G + "sbdchd/squawk::README", "migration")],
    ("rust", "API-009"): [("local:clippy-lints", "exhaustive_enums")],
    ("rust", "DOC-001"): [("local:rustc-lints", "missing-docs")],
    ("rust", "WSP-006"): [(G + "kucherenko/jscpd::README", "duplicat")],
    # ---- TypeScript
    ("typescript", "MOD-001"): [("https://cdn.jsdelivr.net/npm/@microsoft/api-extractor/README.md", "API")],
    ("typescript", "MOD-002"): [(G + "sverweij/dependency-cruiser::doc/rules-reference.md", "no-circular")],
    ("typescript", "MOD-003"): [(G + "import-js/eslint-plugin-import::docs/rules/no-internal-modules.md", "no-internal-modules")],
    ("typescript", "ARC-005"): [(G + "sverweij/dependency-cruiser::doc/rules-reference.md", "moreUnstable")],
    ("typescript", "EXT-001"): [(G + "typescript-eslint/typescript-eslint::packages/eslint-plugin/docs/rules/switch-exhaustiveness-check.mdx", "exhaustive")],
    ("typescript", "RDB-001"): [(G + "prettier/prettier::README", "formatter")],
    ("typescript", "RDB-002"): [(G + "typescript-eslint/typescript-eslint::packages/eslint-plugin/docs/rules/naming-convention.mdx", "naming")],
    ("typescript", "RDB-003"): [("https://cdn.jsdelivr.net/npm/eslint-plugin-sonarjs/README.md", "cognitive-complexity")],
    ("typescript", "RDB-004"): [(G + "eslint/eslint::docs/src/rules/max-depth.md", "max-depth"), (G + "eslint/eslint::docs/src/rules/max-params.md", "max-params")],
    ("typescript", "RDB-005"): [(G + "kucherenko/jscpd::README", "TypeScript")],
    ("typescript", "RDB-006"): [(G + "webpro-nl/knip::README", "unused")],
    ("typescript", "RDB-007"): [(G + "eslint/eslint::docs/src/rules/no-magic-numbers.md", "no-magic-numbers")],
    ("typescript", "TYP-006"): [(G + "typescript-eslint/typescript-eslint::packages/eslint-plugin/docs/rules/no-non-null-assertion.mdx", "non-null")],
    ("typescript", "TYP-007"): [("https://cdn.jsdelivr.net/npm/eslint-plugin-sonarjs/README.md", "no-selector-parameter")],
    ("typescript", "ERR-001"): [(G + "eslint/eslint::docs/src/rules/no-empty.md", "no-empty"), (G + "typescript-eslint/typescript-eslint::packages/eslint-plugin/docs/rules/no-floating-promises.mdx", "floating")],
    ("typescript", "ERR-003"): [("https://raw.githubusercontent.com/microsoft/TypeScript-Website/v2/packages/tsconfig-reference/copy/en/options/noUncheckedIndexedAccess.md", "noUncheckedIndexedAccess")],
    ("typescript", "ERR-005"): [(G + "eslint/eslint::docs/src/rules/preserve-caught-error.md", "preserve-caught-error")],
    ("typescript", "COR-001"): [(G + "gajus/eslint-plugin-jsdoc::docs/rules/require-throws.md", "require-throws")],
    ("typescript", "SEC-001"): [(G + "semgrep/semgrep::README", "TypeScript")],
    ("typescript", "SEC-004"): [(G + "semgrep/semgrep::README", "SAST")],
    ("typescript", "SUP-003"): [(G + "RSeidelsohn/license-checker-rseidelsohn::README", "licen")],
    # SUP-005: npm-ls.md pulls its flag docs from the config definitions, so --json is checked there
    ("typescript", "SUP-005"): [(G + "npm/cli::docs/lib/content/commands/npm-ls.md", "--all"), (G + "npm/cli::workspaces/config/lib/definitions/definitions.js", "'json'")],
    ("typescript", "SUP-006"): [(G + "webpro-nl/knip::README", "dependencies")],
    ("typescript", "SUP-009"): [(G + "CycloneDX/cyclonedx-node-npm::README", "SBOM")],
    ("typescript", "API-001"): [("https://cdn.jsdelivr.net/npm/@microsoft/api-extractor/README.md", "API")],
    ("typescript", "API-002"): [("https://cdn.jsdelivr.net/npm/@microsoft/api-extractor/README.md", "API")],
    ("typescript", "API-007"): [(G + "sbdchd/squawk::README", "migration")],
    ("typescript", "DOC-001"): [(G + "TypeStrong/typedoc::site/options/validation.md", "notDocumented")],
    ("typescript", "WSP-006"): [(G + "kucherenko/jscpd::README", "duplicat")],
    # ---- Python
    ("python", "MOD-002"): [(G + "pylint-dev/pylint::doc/user_guide/checkers/features.rst", "cyclic-import (R0401)")],
    ("python", "MOD-003"): [(G + "seddonym/import-linter::docs/contract_types", "forbidden")],
    ("python", "EXT-001"): [(G + "microsoft/pyright::docs/configuration.md", "reportMatchNotExhaustive")],
    ("python", "EXT-005"): [(G + "pylint-dev/pylint::doc/user_guide/checkers/features.rst", "too-many-ancestors (R0901)")],
    ("python", "RDB-001"): [(G + "astral-sh/ruff::README", "format")],
    ("python", "RDB-002"): [("https://docs.astral.sh/ruff/rules/", "N801")],
    ("python", "RDB-003"): [(G + "rohaquinlop/complexipy::README", "ognitive")],
    ("python", "RDB-004"): [("https://docs.astral.sh/ruff/rules/", "PLR0913")],
    ("python", "RDB-005"): [(G + "kucherenko/jscpd::README", "Python")],
    ("python", "RDB-006"): [(G + "jendrikseipp/vulture::README", "unused")],
    ("python", "RDB-007"): [("https://docs.astral.sh/ruff/rules/", "PLR2004")],
    ("python", "TYP-006"): [(G + "microsoft/pyright::docs/configuration.md", "reportOptionalMemberAccess")],
    ("python", "TYP-007"): [("https://docs.astral.sh/ruff/rules/", "FBT001")],
    ("python", "ERR-001"): [("https://docs.astral.sh/ruff/rules/", "S110"), ("https://docs.astral.sh/ruff/rules/", "BLE001"), ("https://docs.astral.sh/ruff/rules/", "E722")],
    ("python", "ERR-002"): [("https://docs.astral.sh/ruff/rules/", "BLE001")],
    ("python", "ERR-004"): [("https://docs.astral.sh/ruff/rules/", "TRY002")],
    ("python", "ERR-005"): [("https://docs.astral.sh/ruff/rules/", "B904")],
    ("python", "ERR-007"): [("https://docs.astral.sh/ruff/rules/", "SIM115")],
    ("python", "ERR-011"): [(G + "microsoft/pyright::docs/configuration.md", "reportOptionalMemberAccess")],
    ("python", "COR-001"): [("https://docs.astral.sh/ruff/rules/", "DOC501"), ("https://docs.astral.sh/ruff/rules/", "DOC201")],
    ("python", "SEC-001"): [(G + "semgrep/semgrep::README", "Python")],
    ("python", "SEC-004"): [(G + "PyCQA/bandit::README", "security")],
    ("python", "SUP-006"): [(G + "fpgmaas/deptry::docs/rules-violations.md", "DEP002")],
    ("python", "API-001"): [(G + "mkdocstrings/griffe::README", "breaking")],
    ("python", "API-002"): [(G + "mkdocstrings/griffe::README", "API")],
    ("python", "API-007"): [(G + "3YOURMIND/django-migration-linter::README", "migration")],
    ("python", "DOC-001"): [("https://docs.astral.sh/ruff/rules/", "D100"), ("https://docs.astral.sh/ruff/rules/", "D107")],
    ("python", "WSP-006"): [(G + "kucherenko/jscpd::README", "duplicat")],
    # ---- neutral
    ("neutral", "SEC-002"): [(G + "gitleaks/gitleaks::README", "sarif")],
    ("neutral", "SEC-006"): [(G + "google/osv-scanner::README", "vulnerab")],
    ("neutral", "SEC-009"): [(G + "bridgecrewio/checkov::README", "SARIF")],
    ("neutral", "SUP-002"): [(G + "google/osv-scanner::README", "vulnerab")],
    ("neutral", "SUP-008"): [(G + "zizmorcore/zizmor::README", "GitHub Actions")],
    ("neutral", "API-006"): [(G + "bufbuild/buf::README", "breaking")],
    ("neutral", "DOC-010"): [(G + "lycheeverse/lychee::README", "links")],
}


def main(out_path, cells_path):
    cells = {}
    with open(cells_path, encoding="utf-8", newline="") as fh:
        for r in csv.DictReader(fh, delimiter="\t"):
            cells[(r["language"], r["id"])] = r
    delegated = {k for k, r in cells.items() if r["status"] == "delegated"}
    missing = sorted(delegated - set(CLAIMS))
    extra = sorted(set(CLAIMS) - delegated)
    rows = []
    versions = fetch("local:rustc-version").strip().replace("\n", " | ")
    for key in sorted(CLAIMS):
        for src, needle in CLAIMS[key]:
            try:
                text = fetch(src)
                # rustc and clippy print lint names with dashes; the lints are written with underscores
                norm = (lambda t: t.lower().replace("_", "-")) if src.startswith("local:") else str.lower
                result = "confirmed" if norm(needle) in norm(text) else "NOT-FOUND"
                size = len(text)
            except Exception as e:  # recorded, never hidden
                result, size = f"fetch-failed: {type(e).__name__}: {str(e)[:80]}", 0
            r = cells.get(key, {})
            rows.append({"language": key[0], "id": key[1], "tool": r.get("tool", ""), "tool_rule": r.get("tool_rule", ""),
                         "source": src.replace(G, "github:"), "needle": needle, "source_bytes": size, "result": result,
                         # a specific lint/code/flag found in the tool's own docs or source is strong; a plain keyword found in a
                         # README ("Python", "licenses") only shows the tool talks about the subject
                         "strength": "weak (keyword in a README)" if src.rsplit("/", 1)[-1].split("::")[-1].upper().startswith("README") else "strong (specific name in docs or source)"})
            print(f"{key[0]:10} {key[1]:8} {r.get('tool','')[:22]:22} {needle[:34]:34} {result}", file=sys.stderr)
    with open(out_path, "w", encoding="utf-8", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=list(rows[0].keys()), delimiter="\t", lineterminator="\n")
        w.writeheader()
        w.writerows(rows)
    print(f"toolchain: {versions}", file=sys.stderr)
    print(f"delegated cells without a claim entry: {missing}; entries without a delegated cell: {extra}", file=sys.stderr)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
