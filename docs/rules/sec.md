# Keeping attackers out of the code: the SEC rules

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

## Why it matters

Most vulnerabilities come from a few causes: data from an untrusted party passing a trust boundary with
no inspection, authorization that is missing or applied inconsistently, mishandled secrets and
cryptography, and memory unsafety. Vulnerability databases and breach analyses give this a strong
evidence base. Careful, defensive coding helps but is nowhere near enough by itself. The posture
CodeRipper takes: use the best existing scanners for what they do well, and use language models only for
what scanners cannot reach, such as flawed logic and authorization holes, and never let them be the only barrier.

## How to apply it

**Data crossing boundaries**
- Declare trust boundaries and keep a threat model. Adding an endpoint, an integration or a data store
  should trigger a review.
- Validate and parse at the boundary. Encode output, use parameterised queries, canonicalise paths, avoid
  invoking a shell, defend against server-side request forgery, and never rebuild untrusted payloads as
  polymorphic types.
- Treat model output, and any content a model reads, as untrusted wherever a model is embedded in a system.

**Who may do what**
- Centralise authorization. Deny by default, check it at every entry point, and include object-level
  checks and tenant isolation.
- Apply least privilege both in application code and in the infrastructure around it: cloud roles,
  containers, service accounts.

**Secrets and cryptography**
- Keep secrets out of source and logs, rotate them, and scan history as well as the current tree.
- Use vetted libraries with high-level interfaces, never invent primitives, draw security-relevant
  randomness from a cryptographic generator, hash passwords with a modern algorithm and leave certificate
  validation on.

**Memory safety**
- Prefer memory-safe languages, and confine and justify any native or unsafe code.

## Background and lineage

Saltzer and Schroeder (1975) set out eight design principles: grant the least privilege needed; make
refusal the default; check every access; keep mechanisms small; do not rely on secret designs; require
more than one condition for sensitive actions; share as little machinery as possible between users; and
make secure behaviour easy to follow. Later tools and frameworks build on them: STRIDE and threat
modelling, the OWASP Top 10 together with the verification standard ASVS and the maturity model SAMM, CWE as a
shared vocabulary, and Microsoft's SDL and NIST's SSDF as process guides. Automated analysis includes
taint analysis and query-based static scanners such as CodeQL, fuzzing, and the family of secret
scanners. The memory-safety push is led by CISA and NSA guidance and secure-by-design thinking.

## Measures and numbers

The source sets no numeric thresholds here. The practical measure is volume discipline: gate on
exploitable, high-confidence finding classes and track the remainder, rather than counting raw findings.

## Cautions

- Over-application is security theater: a flood of unsorted findings trains teams to disregard the scanner.
  Block only on exploitable classes reported with high confidence, and keep a record of the rest.
- SEC-006 is the same check as SUP-002 in the dependencies domain.
- SEC-010 is advisory; every model finding needs evidence from the code and a human decision.
- Some scanner rule sets carry licences that restrict redistribution or use in a hosted service, so they
  cannot be bundled by default.

## Where it is checked

No rule in this domain has a CodeRipper check today; every row is a catalog record. The triage is a
plan: where it names a tool or a native module, that is what is intended, not what exists. Only five
checks are implemented in CodeRipper at present, and none of them covers security.

| Rule | Title | Class | Checked by |
| :--- | :--- | :--- | :--- |
| SEC-001 | No unsanitised taint flow to dangerous sinks | P | no check yet; planned: Semgrep taint mode for Rust, TypeScript and Python (taint support is limited) |
| SEC-002 | No committed secrets | N | delegated to gitleaks (`--profile extended`; scans the whole git history; needs the tool installed, else a reported gap) |
| SEC-003 | Authorization on every entry point | P | no check yet; planned: route inventory joined to auth middleware for TypeScript and Python; Rust needs framework mapping and a model |
| SEC-004 | No cryptography misuse | P | no check yet; planned: a native source scan for Rust, Semgrep for TypeScript, Bandit for Python |
| SEC-005 | Unsafe and native code inventory | P | no check yet; planned: cargo-geiger for Rust, native inventories for TypeScript and Python |
| SEC-006 | No known exploitable dependency vulnerabilities | N | no check yet; planned: cargo-audit for Rust, osv-scanner for other languages (same check as SUP-002; needs network) |
| SEC-007 | A current threat model | M | no check yet (presence is checkable, currency needs a human decision) |
| SEC-008 | No sensitive data in logs or telemetry | P | no check yet; planned: native heuristics for TypeScript and Python; Rust needs taint analysis |
| SEC-009 | Least privilege in infrastructure definitions | N | no check yet; planned: Checkov through a language-neutral module |
| SEC-010 | Model-assisted review of risky changes | L | no check yet (advisory; needs a human decision) |

## Further reading

- "The Protection of Information in Computer Systems" by Jerome Saltzer and Michael Schroeder
- OWASP Top 10, ASVS and SAMM
- Common Weakness Enumeration (CWE)
- NIST SP 800-218, Secure Software Development Framework
- Microsoft Security Development Lifecycle
- CISA and NSA guidance on memory-safe languages and secure by design
