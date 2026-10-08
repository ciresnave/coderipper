# Governing many repositories together

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

## Why it matters

A single repository can be spotless while the organization around it is not. Shared libraries,
unwritten agreements between repositories, differing habits and mismatched versions let drift
build up between repositories, and they enlarge the damage any one change can cause. A verifier
that claims to cover one or several codebases therefore needs a view at the level of the whole
organization. The supporting evidence is practitioner experience from large organizations that
run monorepos and large-scale changes, not controlled measurement.

## How to apply

Know what you have:

- Keep an inventory in which each repository has a named owner, an importance level and a stage
  of life. Ownership metadata such as a CODEOWNERS file makes the owner findable from the code.

Keep the structure sound:

- Merge the per-repository manifests into one dependency graph. Declare which tier may depend on
  which (platform code never on product code), forbid cycles among repositories of a tier, and
  weight a change by how many things depend on the repository.
- Avoid lockstep releases. Needing several repositories to ship together suggests a distributed
  monolith, which has the downsides of both layouts.

Make the seams safe:

- Give each cross-repository interface a contract and test it from both sides.
- For internal libraries with many dependents, build and test those dependents against a
  candidate release before it ships, in the manner of the Rust project's crater runs.

Keep things current and uniform:

- Agree a limit on version skew: consumers may trail the newest release by only so many
  versions,
  and deprecated internal libraries are removed on a published schedule.
- Let repositories inherit a baseline from one organization-level source, recording each
  exception as a waiver. Template repositories and "golden paths" keep new projects close to
  the baseline from day one.
- Invest in cross-repository change tooling: codemods and structural rewriting tools
  (OpenRewrite, comby, ast-grep), plus update bots, make rolling out a policy change affordable.

## Background and lineage

Two layouts both work: a monorepo with graph-aware builds (Bazel, Nx, Cargo workspaces), as
described in Google's account of large-scale change, or many repositories with strict contracts.
Platform engineering, inner source and ownership files grew up around the same concerns. What
matters is that the graph and the contracts are visible and enforced, whichever layout is used.

## Measures and numbers

No fixed thresholds ship with this domain. The skew allowance (how many versions behind is
acceptable) is a number each organization chooses for itself.

## Cautions

- Lockstep detection (WSP-009) rests on release and deploy history and is a hint about coupling,
  not proof of it.
- Clone detection across repositories finds textual copies far better than copies that were
  rewritten but do the same thing; the second kind needs a model-based judgment.
- The existing check `version-consistency` is about one project's own packages sharing a number,
  which is a different claim from skew between repositories.

## Where it is checked

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| WSP-001 | Every repository is registered with an owner | N | catalog record; no check yet (planned: native check of a registry file and CODEOWNERS presence) |
| WSP-002 | Cross-repository dependencies point the right way and do not loop | A | catalog record; no check yet (needs a declared architecture) |
| WSP-003 | Internal library versions stay within the skew policy | N | catalog record; no check yet (planned: native manifest comparison per ecosystem); the legacy check `version-consistency` overlaps in part |
| WSP-004 | Widely used libraries are tested against their dependents before release | X | catalog record; no check yet (needs project code to run) |
| WSP-005 | Organization policy is inherited and exceptions are waived | T | catalog record; no check yet (needs policy resolution) |
| WSP-006 | Copied code across repositories | P | catalog record; no check yet (planned: jscpd run over several checkouts; rewritten copies would need a model) |
| WSP-007 | Interfaces between repositories have contract tests | X | catalog record; no check yet (needs project code to run) |
| WSP-008 | Conventions do not drift between repositories | N | catalog record; no check yet (planned: native comparison of generic config files) |
| WSP-009 | Repositories forced to release together | H | catalog record; no check yet (needs a human judgment over release history) |
| `version-consistency` | One version across a project | legacy | legacy check: every package of a project shares one version unless it tracks another project; relates to WSP-003 only partly |
| `ci-protection-presence` | The default branch enforces required checks | legacy | legacy check: reads branch protection and flags rules that require nothing; no catalog rule covers it, nearest context is the supply-chain CI-hardening rule SUP-008 and the layered-gates rule GOV-008 |

## Further reading

- "Why Google Stores Billions of Lines of Code in a Single Repository", Rachel Potvin and Josh
  Levenberg (2016).
- The crater tool in the Rust project, for testing a change against published dependents.
- OpenRewrite, comby and ast-grep, for structural code rewriting.
- Bazel and Nx documentation, for graph-aware builds.
