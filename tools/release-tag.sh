#!/usr/bin/env bash
# SPDX-License-Identifier: MIT OR Apache-2.0
# Tag the commit a published coderipper version came from. See RELEASING.md.
#
#   tools/release-tag.sh tag <version> [--push]   create the annotated tag v<version> on the commit named by the published
#                                                 crate's .cargo_vcs_info.json; with --push, push that one tag to origin
#   tools/release-tag.sh check [<version>]        fail unless origin has the tag v<version> (default: the newest version on crates.io)
#
# The commit comes from the crate crates.io serves, never from a date or a guess. A version whose crate has no vcs info, or
# whose commit is not in this repository, is reported and NOT tagged (exit 2). An existing tag is never moved or replaced.
set -euo pipefail

crate=coderipper
ua="coderipper-release-tag (https://github.com/ciresnave/coderipper)"

newest() {
  curl -fsS -A "$ua" "https://crates.io/api/v1/crates/$crate" | sed -n 's/.*"max_stable_version": *"\([^"]*\)".*/\1/p' | head -n 1
}

remote_has_tag() {
  [ -n "$(git ls-remote --tags origin "refs/tags/v$1")" ]
}

case "${1:-}" in
  check)
    version="${2:-$(newest)}"
    [ -n "$version" ] || { echo "release-tag: could not read the newest version from crates.io" >&2; exit 1; }
    if remote_has_tag "$version"; then
      echo "release-tag: origin has v$version"
    else
      echo "release-tag: crates.io lists $crate $version and origin has no tag v$version; run tools/release-tag.sh tag $version --push" >&2
      exit 1
    fi
    ;;
  tag)
    version="${2:?usage: release-tag.sh tag <version> [--push]}"
    push="${3:-}"
    if git rev-parse -q --verify "refs/tags/v$version" >/dev/null || remote_has_tag "$version"; then
      echo "release-tag: v$version already exists; a tag is never moved" >&2
      exit 1
    fi
    work="$(mktemp -d)"
    trap 'rm -rf "$work"' EXIT
    curl -fsS -A "$ua" -o "$work/$crate.crate" "https://static.crates.io/crates/$crate/$crate-$version.crate"
    sha="$(tar -xzOf "$work/$crate.crate" "$crate-$version/.cargo_vcs_info.json" 2>/dev/null | sed -n 's/.*"sha1": *"\([0-9a-f]\{40\}\)".*/\1/p' | head -n 1 || true)"
    if [ -z "$sha" ]; then
      echo "release-tag: $crate $version has no .cargo_vcs_info.json sha1; not tagging" >&2
      exit 2
    fi
    git fetch -q origin "$sha" 2>/dev/null || true
    if ! git cat-file -e "$sha^{commit}" 2>/dev/null; then
      echo "release-tag: $crate $version names commit $sha, which this repository does not have; not tagging" >&2
      exit 2
    fi
    git tag -a "v$version" -m "$crate $version" "$sha"
    echo "release-tag: tagged v$version at $sha"
    if [ "$push" = "--push" ]; then
      git push origin "refs/tags/v$version"
    fi
    ;;
  *)
    echo "usage: tools/release-tag.sh tag <version> [--push] | check [<version>]" >&2
    exit 64
    ;;
esac
