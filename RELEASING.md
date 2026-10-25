# Releasing

The PM publishes: no lane runs `cargo publish`, and a version number is allocated at gate time (the version in `Cargo.toml` of a pull request is a proposal).

## Tag every publish

Each version published to crates.io has an annotated git tag `v<version>` on the commit it was published from (issue #94). After `cargo publish`:

```
tools/release-tag.sh tag <version> --push
```

The script takes the commit from the published crate's `.cargo_vcs_info.json` (never from a date), checks the commit is in this repository, creates the annotated tag (`coderipper <version>`), and pushes that one tag. It refuses to move or replace a tag, and it does not tag a version whose crate has no vcs info or whose commit is not in the repository (exit 2: list it instead).

`tools/release-tag.sh check [<version>]` fails unless origin has the tag; the scheduled `release-drift` workflow runs it for the newest published version, so a publish without a tag turns that workflow red the next morning.

## History

Tags `v0.9.20` and `v0.9.24` were created on 2026-10-25. No earlier version has one: of the 34 versions on crates.io then, 27 were published without vcs info, and 5 name a commit that is not in the repository (a branch commit that was squash-merged away). Those are not tagged, because a tag on a guessed commit would be a false statement.
