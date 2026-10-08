#!/usr/bin/env python3
"""Look up facts about the third-party tools named in the triage, from public registries.

Read-only. Standard library only. For every distinct `coordinate` in the given triage TSV files it asks the registry the
coordinate names (crates.io, npm, PyPI, GitHub) and records, with the date of the lookup:

  licence (SPDX where the registry gives one), latest version, date of the latest release, the source repository,
  whether that repository is archived, its last push date, and a `maintained` flag.

`maintained` is a rule, not a judgement: true when the latest release OR the last push is within 365 days of the lookup date,
and the repository is not archived. It says nothing about quality.

Output (never contains a token or any request header):
  tools_raw.json  - one record per coordinate: the URLs asked, the status, and the fields read from the answers
  tools.tsv       - the derived table, one row per coordinate

Rate limits: one request per second to crates.io (their crawler policy); GitHub is asked through the `gh` CLI, which uses
whatever account is already logged in, read-only (`gh api` GET). Nothing is written to any registry or repository.

Usage:  python lookup_tools.py OUT_DIR  TRIAGE_TSV [TRIAGE_TSV ...]
"""
import csv
import datetime as dt
import json
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request

UA = "coderipper-triage/0.1 (read-only lookup; https://github.com/ciresnave/coderipper)"
TODAY = dt.datetime.now(dt.timezone.utc)


def http_json(url, delay=0.0):
    """GET a URL, return (status, parsed JSON or None). The User-Agent is the only header sent."""
    if delay:
        time.sleep(delay)
    req = urllib.request.Request(url, headers={"User-Agent": UA, "Accept": "application/json"})
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            return resp.status, json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        return e.code, None
    except Exception as e:  # network trouble is recorded, never hidden
        return f"error: {type(e).__name__}: {e}", None


def gh_json(path):
    """GET a GitHub REST path through the gh CLI. Returns (status, parsed JSON or None)."""
    try:
        out = subprocess.run(["gh", "api", path], capture_output=True, text=True, encoding="utf-8", errors="replace",
                             timeout=60)
    except Exception as e:
        return f"error: {type(e).__name__}: {e}", None
    if out.returncode != 0:
        # gh prints "HTTP 404" style messages on stderr; keep only the status-like part, never anything else
        msg = out.stderr.strip().splitlines()[-1] if out.stderr.strip() else "gh failed"
        return f"gh: {msg[:120]}", None
    try:
        return 200, json.loads(out.stdout or "null")
    except ValueError:
        return "gh: unparseable answer", None


def repo_slug(url):
    """owner/repo from a git/https URL on github.com, else None."""
    if not url:
        return None
    u = url.strip()
    for prefix in ("git+", "git:"):
        if u.startswith(prefix):
            u = u[len(prefix):]
    u = u.replace("ssh://git@", "https://").replace("git@github.com:", "https://github.com/")
    if "github.com/" not in u:
        return None
    rest = u.split("github.com/", 1)[1].split("#")[0].split("?")[0]
    parts = [p for p in rest.split("/") if p]
    if len(parts) < 2:
        return None
    return parts[0] + "/" + parts[1].removesuffix(".git")


def parse_date(s):
    if not s:
        return None
    try:
        return dt.datetime.fromisoformat(s.replace("Z", "+00:00")).astimezone(dt.timezone.utc)
    except ValueError:
        return None


def look_crates(name):
    rec = {"urls": [f"https://crates.io/api/v1/crates/{name}"]}
    status, doc = http_json(rec["urls"][0], delay=1.1)
    rec["status"] = status
    if doc:
        c = doc.get("crate", {})
        vers = doc.get("versions", [])
        latest = next((v for v in vers if not v.get("yanked")), vers[0] if vers else {})
        rec.update(
            licence=latest.get("license"),
            latest_version=c.get("max_stable_version") or c.get("max_version"),
            latest_release=latest.get("created_at"),
            repo_url=c.get("repository"),
        )
    return rec


def look_npm(name):
    url = "https://registry.npmjs.org/" + urllib.parse.quote(name, safe="@")
    rec = {"urls": [url]}
    status, doc = http_json(url)
    rec["status"] = status
    if doc:
        latest = doc.get("dist-tags", {}).get("latest")
        lic = doc.get("license")
        if isinstance(lic, dict):
            lic = lic.get("type")
        repo = doc.get("repository")
        rec.update(
            licence=lic,
            latest_version=latest,
            latest_release=doc.get("time", {}).get(latest),
            repo_url=repo.get("url") if isinstance(repo, dict) else repo,
        )
    return rec


def look_pypi(name):
    url = f"https://pypi.org/pypi/{urllib.parse.quote(name)}/json"
    rec = {"urls": [url]}
    status, doc = http_json(url)
    rec["status"] = status
    if doc:
        info = doc.get("info", {})
        version = info.get("version")
        files = doc.get("releases", {}).get(version, []) or doc.get("urls", [])
        upload = min((f.get("upload_time_iso_8601") for f in files if f.get("upload_time_iso_8601")), default=None)
        lic = info.get("license_expression") or info.get("license")
        if lic and len(lic) > 80:  # a pasted licence text, not an identifier
            lic = None
        if not lic:
            classes = [c for c in info.get("classifiers", []) if c.startswith("License ::")]
            lic = "; ".join(c.split("::")[-1].strip() for c in classes) or None
        urls = info.get("project_urls") or {}
        repo_url = next((v for k, v in urls.items() if "github.com" in (v or "") and k.lower() in
                         ("source", "source code", "repository", "homepage", "code", "github")), None)
        repo_url = repo_url or next((v for v in urls.values() if "github.com" in (v or "")), None)
        rec.update(licence=lic, latest_version=version, latest_release=upload, repo_url=repo_url)
    return rec


def look_github(slug):
    rec = {"urls": [f"gh api repos/{slug}"], "status": None}
    status, doc = gh_json(f"repos/{slug}")
    rec["status"] = status
    if doc:
        lic = (doc.get("license") or {}).get("spdx_id")
        rec.update(licence=lic if lic not in (None, "NOASSERTION") else None, repo_url=doc.get("html_url"))
        rel_status, rel = gh_json(f"repos/{slug}/releases/latest")
        rec["urls"].append(f"gh api repos/{slug}/releases/latest")
        rec["release_status"] = rel_status
        if rel:
            rec.update(latest_version=rel.get("tag_name"), latest_release=rel.get("published_at"))
    return rec


def repo_facts(slug):
    """Archived flag and last push of a GitHub repository (the maintenance signal)."""
    status, doc = gh_json(f"repos/{slug}")
    if not doc:
        return {"repo_status": status}
    return {
        "repo_status": 200,
        "repo_archived": bool(doc.get("archived")),
        "repo_pushed_at": doc.get("pushed_at"),
        "repo_licence": ((doc.get("license") or {}).get("spdx_id")),
    }


COPYLEFT = ("GPL", "AGPL", "LGPL", "EUPL", "SSPL", "BUSL", "CPAL", "OSL")
PERMISSIVE = ("MIT", "APACHE", "BSD", "ISC", "UNLICENSE", "0BSD", "ZLIB", "CC0", "PYTHON", "MPL", "BLUEOAK", "BSL-1.0")


def licence_class(lic):
    if not lic:
        return "unknown"
    u = lic.upper()
    if any(k in u for k in COPYLEFT):
        return "copyleft-or-restricted (check)"
    if any(k in u for k in PERMISSIVE):
        return "permissive-or-weak"
    return "other (check)"


def lookup(coord):
    kind, _, name = coord.partition(":")
    if kind == "builtin" or coord == "-":
        return {"coordinate": coord, "status": "not looked up (ships with the language toolchain)"}
    look = {"crates": look_crates, "npm": look_npm, "pypi": look_pypi, "github": look_github}.get(kind)
    if look is None:
        return {"coordinate": coord, "status": f"unknown coordinate kind {kind!r}"}
    rec = look(name)
    rec["coordinate"] = coord
    slug = name if kind == "github" else repo_slug(rec.get("repo_url"))
    if slug:
        rec["repo_slug"] = slug
        rec.update(repo_facts(slug))
    return rec


def derive(rec):
    last_release = parse_date(rec.get("latest_release"))
    pushed = parse_date(rec.get("repo_pushed_at"))
    recent = [d for d in (last_release, pushed) if d]
    fresh = any((TODAY - d).days <= 365 for d in recent)
    archived = rec.get("repo_archived")
    lic = rec.get("licence") or rec.get("repo_licence")
    ok = isinstance(rec.get("status"), int) and rec["status"] == 200
    return {
        "coordinate": rec["coordinate"],
        "found": "yes" if ok else ("n/a" if str(rec.get("status", "")).startswith("not looked up") else "NO: " + str(rec.get("status"))),
        "licence": lic or "",
        "licence_class": licence_class(lic) if ok else "",
        "latest_version": rec.get("latest_version") or "",
        "latest_release": (rec.get("latest_release") or "")[:10],
        "repo": rec.get("repo_slug", ""),
        "repo_archived": "" if archived is None else str(archived).lower(),
        "repo_last_push": (rec.get("repo_pushed_at") or "")[:10],
        "maintained": ("yes" if (fresh and not archived) else "no") if ok else "",
    }


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 2
    out_dir, files = argv[1], argv[2:]
    coords = {}
    for f in files:
        with open(f, encoding="utf-8", newline="") as fh:
            for row in csv.DictReader(fh, delimiter="\t"):
                c = (row.get("coordinate") or "").strip()
                if c and c != "-":
                    coords.setdefault(c, []).append(row.get("tool", ""))
    raw, table = [], []
    for c in sorted(coords):
        rec = lookup(c)
        rec["tool_names_seen"] = sorted(set(coords[c]))
        raw.append(rec)
        table.append(derive(rec))
        print(f"{c}: {table[-1]['found']} {table[-1]['licence']} {table[-1]['latest_version']} maintained={table[-1]['maintained']}",
              file=sys.stderr)
    stamp = TODAY.strftime("%Y-%m-%dT%H:%MZ")
    with open(f"{out_dir}/tools_raw.json", "w", encoding="utf-8", newline="\n") as fh:
        json.dump({"retrieved": stamp, "records": raw}, fh, indent=1, sort_keys=True)
        fh.write("\n")
    cols = ["coordinate", "found", "licence", "licence_class", "latest_version", "latest_release", "repo", "repo_archived",
            "repo_last_push", "maintained"]
    with open(f"{out_dir}/tools.tsv", "w", encoding="utf-8", newline="") as fh:
        w = csv.DictWriter(fh, fieldnames=cols, delimiter="\t", lineterminator="\n")
        w.writeheader()
        w.writerows(table)
    print(f"retrieved {stamp}; {len(table)} coordinates", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
