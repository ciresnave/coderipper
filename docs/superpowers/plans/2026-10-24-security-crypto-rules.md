# Security and cryptography rules (family 1): design and measurement

> **Status: DESIGN with a measurement, for PM review. Nothing here is built.** The PM's task of 2026-10-23 (family 1 of the ROADMAP's "Next rule
> families"): *"unwrap/expect/panic on untrusted input, hard-coded keys, weak algorithms, non-constant-time compare, AEAD without AAD, secrets not
> zeroized, injection, path traversal/zip-slip, unbounded allocation from untrusted lengths, TLS verification off, sensitive data in logs"*. It extends
> the catalogue records SEC-001, SEC-004, SEC-005 and SEC-008 (`rules/sec.toml`, `docs/rules/sec.md`; SEC-002 and SEC-006 are delegated and built).
>
> **Headline.** Of the seven incidents the portfolio's own history records (section 1), a throwaway detector tells the commit *before* from the commit *after*
> on **five** (hard-coded key material, twice; Debug that prints a secret; unzeroized key material, twice), and cannot on **two** (an AAD that omits
> metadata; a username in a log line), which are not statically checkable and the document says so. On today's code, **one rule clears the gate**
> (`secret-field-not-zeroized`, 25 of 30 where a reviewer would act, 83%), **one is a clean inventory that no one should act on** (weak algorithms: every
> use is mandated by a protocol), and the name-based rules that looked most natural (a secret-named value in a log line, a hard-coded secret constant)
> **fail badly until their matcher is tightened, and still do not clear**. The measurement also found five places in auth-framework worth a reviewer's
> time (section 7).

## 1. The incidents, and what each tells us

The positive controls are real history, not seeded files. Each row is a fix commit; the detector is run on the files the commit changed, at the commit's
parent and at the commit itself (`incidents.sh`, below).

| incident (fix) | what was wrong | rule that should see it | before the fix | after the fix |
|---|---|---|---|---|
| auth-framework #218 `2825bbea` (JARM) | `EncodingKey::from_secret(b"test_key_for_development_only_...")` as a fallback signing key | hard-coded key material | **4** findings | **0** |
| auth-framework #222 `2a949b59` (JWT introspection) | `let key_bytes = b"introspection_jwt_secret_key_change_in_production"; EncodingKey::from_secret(key_bytes)` | hard-coded key material, through a local | **2** | **0** |
| auth-framework #265 `69bd8aa0` | `derive(Debug)` on types with a password, a token, a user code | Debug prints a secret field | 1 | **0** |
| auth-framework #61 `ba97efde` | `Rsa { private: Vec<u8>, .. }` held an RSA private key unzeroized | secret field not zeroized (enum variants too) | 3 | **2** |
| auth-framework #66 to #72 (`8c3b1b14`, `a281f6ef`, `16d269da`, `8aa88268`) | secret-string fields of config, protocol and GUI types not zeroized | secret field not zeroized | 10 (and 8 Debug) | **5** (and 3) |
| auth-framework #133 `825e1b42` | the AEAD's associated data did not bind the envelope's algorithm and key id (format v2 does) | **none: not statically checkable** | the same | the same |
| synapse #186 `6a2962e` | a username in a bcrypt-failure log line | **none: a username is not a secret by name** | none | none |

Two things these rows establish. First, the detector's counts fall on the fixes that removed a concrete, syntactic defect, which is the best evidence
available that the rule means what it says. Second, **#133 is a class of defect no source scan sees**: whether the associated data should include a given
field is a design decision. The only static neighbours are "an AEAD call with no AAD" and "an explicit empty AAD" (section 2), and both fire on the
fixed code too (the legacy v0 decrypt path keeps an empty AAD on purpose). A rule claiming to catch #133 would be lying.

The reviewer-facing citation for the zeroize policy is the code itself: #61's doc comment records *"per CireSnave's standing rule, a credential that can
be re-provided by the caller should only be resident as long as needed and then zeroed so it cannot linger in freed memory"*.

## 2. The rules that can be written from source text

All of them are readings of the syntax tree (`syn` for Rust). **None can see types**, so each says what it guesses. Test code, `tests/`, `benches/`,
`examples/` and `build.rs` are not read. Ids continue the catalogue: SEC-004 (cryptography misuse) is split into lettered checks; SEC-011 to SEC-013 are new.

| id (proposed) | rule | what it reads | precision note |
|---|---|---|---|
| SEC-004a | **Hard-coded key material** | `from_secret`, `new_from_slice`, `from_slice`, `hmac::Key::new` given a non-empty string or byte literal, **or a local bound to one** | Exact on the call names, and the local is followed within one function. Misses a key built from `concat!`, a `const`, or a helper. It also fires on a deliberate fixture key in non-test code, which is what a waiver is for |
| SEC-004b | **TLS verification off** | `danger_accept_invalid_certs(x)` unless `x` is literal `false`, `danger_accept_invalid_hostnames`, rustls `.dangerous()` | Exact on the API names. A configuration pass-through (`danger_accept_invalid_certs(config.accept_invalid_certs)`) fires by design: the finding says "check the default" |
| SEC-004c | **Weak algorithm in use** | `md5`, `sha1`, `des`, `rc4`, ECB at a word boundary in `use` and constructor lines | Exact, but **an inventory, not a defect**: protocols mandate MD5 (RADIUS), SHA-1 (HOTP, WS-Security, the HIBP range API). Severity Info; a waiver cites the protocol |
| SEC-004d | **AEAD without associated data** | `.encrypt(nonce, x)` / `.decrypt(nonce, x)` with two arguments in a file that uses an AEAD crate; `Payload { aad: b"" }` | Fires on the project's own `decrypt(&envelope, key)` wrapper (3 of 7): needs the receiver's type. Whether AAD is *required* is a protocol fact (JWE requires it), not a syntax one |
| SEC-004e | **Non-constant-time comparison of a secret** | `==`/`!=` where both operands end in a secret-named or MAC-like identifier | Name-based: fires on a message digest compared for ack matching and on LLM token ids. The match is on the **last segment**, which removed most of the noise |
| SEC-011 | **Secret field not zeroized** | a struct or enum-variant field whose name's last segment is `password`, `passwd`, `passphrase`, `secret`, `token`, `credential`, `bearer`, or `api_key`, `private_key`, `signing_key`, ... , typed `String`, `Vec<u8>`, `Option<..>` or `[u8; N]`, in a type with no `Zeroize`/`ZeroizeOnDrop` | **The one rule that clears the gate.** FP causes: a CSRF token, a response token, a parser's DSL token |
| SEC-008a | **Debug prints a secret** | `derive(Debug)` on a type with a SEC-011-shaped field | 50% on the census: `token` is also a DSL token, a pagination cursor, an ACME challenge |
| SEC-008b | **A secret logged as the value** | a log macro whose arguments name a secret as the value (not a receiver such as `token.user_id`), including an inline `{password}` | After tightening, 3 findings in 8 repositories; see section 5 |
| SEC-012 | **Panic on untrusted input** | `unwrap`, `expect`, `panic!`, indexing, slicing, `as` truncation **inside a function declared as an entry point** (`[security] entry_points`) | **Not measured here.** Without a declared entry point the whole crate would be reported; the rule exists only with the declaration. Reuses Clippy's restriction lints (`unwrap_used`, `expect_used`, `indexing_slicing`, `cast_possible_truncation`) |
| SEC-013 | **Allocation sized by an untrusted length** | `Vec::with_capacity(n)`, `vec![x; n]`, `reserve(n)` where `n` was read in the same function by `from_le_bytes`, `read_u32`, `read_u64` or similar and not bounded by `min`, `try_from`, a comparison or `ensure!` | **Not measured here.** The in-tree **negative control** exists: fuel's `gguf.rs` (`fuel-formats/src/gguf.rs:366`) carries a comment explaining that `Vec::with_capacity(len)` is deliberately not used because `len` is untrusted |
| SEC-001 | **Injection, path traversal, zip-slip** | delegated: CodeQL or Semgrep taint (synapse already runs CodeQL) | Needs taint, which source text does not give. A native syntactic subset is possible (`Command::new("sh").arg("-c")` with a `format!` argument; an archive entry name joined to a destination without a `..` check) and is **not built or measured here** |

Dropped as a rule: **a hard-coded secret constant by name** (`const X_TOKEN: &str = "..."`). It was 0 of 11, then 0 of 5 after tightening: the names
(`NOTICE_TOKEN`, `TYP_REFRESH_TOKEN`, `V1_CREATE_API_KEYS`) are markers, claim values and SQL. A real hard-coded secret is an entropy question, and SEC-002
(gitleaks) already answers it over the whole history.

## 3. Not statically checkable, and not to become a rule

- **Whether the associated data should bind a given field** (#133), **whether a key is strong enough**, **whether a random source is a CSPRNG**
  where the type is hidden behind a trait, **whether a comparison is timing-observable** (it matters only where an attacker can measure it).
- **Whether a value is attacker-controlled.** That is taint. A name list stands in for it in SEC-004e and SEC-011, and each says so.
- **PII in logs** (synapse #186): a username is not a secret by name, and a list of PII names is a policy, not a syntax.
- **Authorization on every entry point** (SEC-003): needs a route inventory and a model of the middleware.
- **Whether a dependency's cryptography is correct.** That is what the advisory databases (SEC-006) are for.

## 4. Scoping, configuration and the default-on policy

- **Names are configuration.** `[security] secret_names = ["pin", "otp"]` adds names and `not_secret_names = ["eos_token"]` removes them, each entry with a
  reason, so a codebase where `token` means a language-model token (fuel, Unpopped) removes it once instead of waiving it 5 times.
- **Entry points are declared** for SEC-012 and SEC-013: `[security] entry_points = ["src/api/", "fn handle_*"]`. Nothing is reported without them.
- **Severity.** SEC-004a, SEC-004b: High (exact, and a stated waiver is cheap). SEC-011, SEC-008a: Medium. SEC-004c: Info. SEC-004d, SEC-004e, SEC-008b:
  Medium with confidence Low. None blocks (default-on never means blocking).
- **Waivers need a reason that names the protocol or the owner** (`reason = "RADIUS (RFC 2865) mandates MD5"`); a weak-algorithm waiver without one is
  rejected by the loader (a **proposed** check, not built).
- **Default-on under the ROADMAP's policy.** The precision gate (at least 80% where a reviewer would act, on at least 30 findings, in at least three
  repositories) is met by SEC-011 only. SEC-004a and SEC-004b are **exact on their API names** and have too few findings to sample (1 and 2), so they
  qualify only under the policy's second clause (exact syntax, nothing to sample) and by the two incident controls. Everything else ships **opt-in**.
- **Reuse before writing.** Where Clippy or the compiler has the check (`unwrap_used`, `expect_used`, `indexing_slicing`, `cast_possible_truncation`, `unsafe_code`),
  require the lint to be enabled (the `lint-suppression` rule finds an `#[allow]` of it without a reason) and add only the scoping, the count, and the
  declared entry points. SEC-005 (the `unsafe` inventory) stays `cargo-geiger`'s.

**How this avoids contradicting the other rules.** A panic is not a bug in a CLI's `main`; SEC-012 therefore fires only where an entry point is declared, and
never in `main`, argument parsing or a test. SEC-011 adds `Zeroizing` or a derive, which costs a line and no clarity. SEC-004c asks for a *reason*, not a
replacement, so it cannot conflict with a protocol's specification. A rule that would ask a project to weaken clarity for security is not in this family.

## 5. Measurement

**Detector.** A throwaway `syn` visitor (`security-prototype/secproto-main.rs.txt`, not shipped), run over every non-test `.rs` file `git ls-files`
lists (and `git archive` of `origin/main` for the lane repositories, read-only) in **eight repositories**: auth-framework `ae1328b4` (357 files), synapse
`861dc15` (196), mlmf `cedff18` (165), lightbulb `e1f9d3d` (136), lsp-bridge `8773679` (24), and fuel `5a00e3a`, baracuda `8e162f7`, Unpopped `21e6279` (the
depth-1 clones of the performance measurement).

**Controls (before the sample).** `control-lib.rs.txt` seeds one instance of each rule and the cases that must stay silent (`danger_accept_invalid_certs(false)`, `Payload { aad: b"ctx" }`, `info!("generated {} tokens", tokens)`, `MAX_TOKENS`, a `Zeroizing<String>` field, a key
from a runtime value): all ten rule kinds fire and the silent cases stay silent. The first run of the incident files exposed **four detector defects**
that the seeded control had not: a secret in an **enum variant**, a key literal **bound to a local**, a field literally named `private`, and the real AEAD
API being `Payload { .. }` rather than a two-argument call. All four were fixed and the incident table of section 1 is the result.

**Matcher versions.** *v2* matched a secret word anywhere in a name (`token_endpoint`, `credential_type`, `password_hash` all matched). *v3* matches the
**last segment** of the name, or a known two-segment key name, and excludes examples and benches by path. The v2 results are kept below because they are the
evidence that the tightening was needed.

**Judging.** One reader (this lane). Each finding in a seeded order (`random.Random(2026)` over the findings sorted by repository, file and line), read in
its context, with two marks: **right** (the code has what the rule names) and **acts** (a reviewer would change the code, not only waive it; for a secret
field, the owner's zeroize rule above). A rule with fewer than 30 findings was read whole. The stopping rule of the performance sample (30 judged, or 7
that no reviewer would act on) applies to SEC-011 only, which reached 30.

### Results

| rule | matcher | findings | judged | right | acts | reading |
|---|---|---|---|---|---|---|
| `secret-field-not-zeroized` (SEC-011) | v2 | 146 | 12 | 8 (67%) | 5 (42%) | substring names: hashes, public keys and ids matched |
| | **v3** | 113 | **30** | 28 (93%) | **25 (83%)** | **clears the gate**: 22 of the 30 are auth-framework, the rest synapse (3), mlmf (2), fuel (2), lightbulb (1) |
| `debug-prints-secret-field` (SEC-008a) | v2 | 40 | 14 | 3 (21%) | 3 (21%) | 7 false at row 11 |
| | v3 | 20 | 20 (census) | 10 (50%) | 10 (50%) | `token` as a DSL token, a cursor, a challenge |
| `secret-named-value-in-log` (SEC-008b) | v2 | 89 | 12 | 0 | 0 | the secret-named word was only the receiver of a field (`token.user_id`) |
| | v2, receiver rule | 17 | 17 (census) | 4 (24%) | 1 (6%) | 2 of the 4 are examples, now excluded by path |
| | v3 | 3 | 3 (census) | 2 | 1 | too few to judge a rate |
| `non-constant-time-compare` (SEC-004e) | v2 / v3 | 5 / 4 | all | 2 | 2 | 2 of 4 |
| `aead-without-aad` (SEC-004d) | | 7 | 7 (census) | 4 (57%) | 2 (28%) | 3 are the project's own `decrypt` wrapper |
| `weak-algorithm` (SEC-004c) | | 10 | 10 (census) | 9 (one is in a test) | **0** | RADIUS, HOTP, WS-Security and HIBP mandate them |
| `tls-verification-off` (SEC-004b) | | 2 | 2 (census) | 2 | 1 | a QUIC client accepting any certificate; a config pass-through |
| `hardcoded-key-material` (SEC-004a) | | 1 | 1 (census) | 1 | 1 | a dev key as the verification key (below) |
| `hardcoded-secret:const` | v2 / v3 | 11 / 5 | all | 0 | 0 | **dropped** |

**Not measured:** SEC-012, SEC-013 and the injection subset of SEC-001 (section 2).

## 6. What the numbers say

- **SEC-011 is the rule that matches the owner's policy and the code.** 83% of the sampled findings are credentials that a reviewer would zeroize. The
  population (113 in these eight repositories, 93 of them in auth-framework) is large, so it would be reported, not blocking, and that is the point of the
  per-project opt-out count.
- **Tightening a name matcher moved the whole family.** The same rule went from 42% to 83% by matching the last segment of a field name. The same idea
  did not rescue the log rule, whose failure is that the *value* is rarely the secret.
- **Weak algorithms are an inventory.** Every one of the ten is a protocol's own requirement. A rule that cannot be acted on belongs at Info with a reason,
  or in `cargo-deny`-style policy, not in a count that grows a backlog.
- **The incidents that matter most are the ones source text cannot see** (#133, #186). The family needs the delegated, taint-based tools for those, and the
  roadmap should say that the native subset is a floor.

## 7. What the measurement found in auth-framework (to verify, `origin/main` `ae1328b4`)

The detector was written against this repository's history, so it is no surprise that it finds the same shapes still there. These are reported, **not
claimed**: each is a reading of a few lines, and the owner may have a reason.

1. `src/server/oidc/oidc_extensions.rs:1069`: `DecodingKey::from_secret("shared_signals_development_key_not_for_production".as_ref())` as the verification key when
   `SHARED_SIGNALS_VERIFICATION_KEY` is unset, the shape #218 and #222 removed elsewhere (the code logs a security warning and continues).
2. `src/api/oauth2.rs:1229`: logs the first ten characters of a bearer token (`&req.token[..10.min(req.token.len())]`).
3. `src/protocols/gnap.rs:865` and `src/auth.rs:1356`: a continuation token and a stored access token compared with `!=`, which is not constant time.
4. `src/server/oidc/oidc_advanced_jarm.rs:801` and `:1152`: A256GCM content encryption and decryption with no associated data; RFC 7516 makes the protected header the AAD.
5. `src/security/enhanced_crypto.rs:84` and `:119`: ChaCha20-Poly1305 encrypt and decrypt with no associated data (no binding requirement is stated there).

The 22 auth-framework SEC-011 findings in the sample (section 10) are a separate matter: the zeroize policy of #61 to #72 applied to fields those commits did
not reach.

## 8. Plan

| order | rule | ships | needs first |
|---|---|---|---|
| 1 | SEC-011 secret field not zeroized | default-on | `[security] secret_names` / `not_secret_names`; the Zeroizing/derive/impl-Drop recognition; fixture pair; re-sample on a repository outside this one author's |
| 2 | SEC-004a hard-coded key material, SEC-004b TLS off | default-on (exact syntax, two incident controls) | fixture pairs, including the local-literal form |
| 3 | SEC-004c weak algorithm | default-on at **Info**, waiver with a protocol reason | the reason requirement |
| 4 | SEC-008a, SEC-004d, SEC-004e, SEC-008b | opt-in | receiver types for SEC-004d; a decision on bare `token` for SEC-008a |
| 5 | SEC-012, SEC-013 | opt-in | `[security] entry_points`; a measurement of each on the three repositories that parse untrusted files (fuel, mlmf, baracuda) |
| 6 | SEC-001 injection, traversal, zip-slip | delegated first | a CodeQL or Semgrep module; the native subset after it |

**Open points for the PM.** (1) Is a rule that is right 93% of the time and acted on 83% of the time, but whose population is 93 findings in one repository, to be
default-on before it has been sampled on a repository of another owner? (2) Weak algorithms at Info: a count, or silent unless asked? (3) Should the
auth-framework lane be given section 7 as a task, or does the PM route it?

## 9. Reproduce

The detector, the control, the incident script, the sampler and the table generator are in `security-prototype/` as `.txt`. The tables below are
`gen_sec_table.py`'s output; the figures in section 5 are its summary.

## 10. The verdicts, one row each

`right` = the code has what the rule names; `acts` = a reviewer would change the code, not only waive it. Rank = position in the seeded order of that
rule's findings. Sites are `repository path:line` at the refs of section 5.

#### `secret-field-not-zeroized` (113 findings; 30 judged; detector right 28, reviewer acts 25)

| # | site | right | acts | why |
|---|---|---|---|---|
| 1 | auth-framework `src/deployment/monitoring.rs:264` | yes | yes | an unzeroized `password` in a basic-auth config |
| 2 | auth-framework `src/authentication/credentials.rs:107` | yes | yes | an unzeroized `client_secret` in a credential variant |
| 3 | auth-framework `src/admin/web.rs:403` | yes | no | a per-request CSRF token in a template: not zeroize-worthy |
| 4 | lightbulb `src/bin/lightbulb-cli.rs:18` | yes | no | a CLI `--api-key` argument (already in the process arguments) |
| 5 | auth-framework `src/tokens/mod.rs:158` | yes | yes | an unzeroized `access_token` |
| 6 | auth-framework `src/protocols/opa.rs:42` | yes | yes | an unzeroized bearer `auth_token` |
| 7 | auth-framework `src/protocols/ws_trust.rs:241` | yes | yes | an unzeroized security token value |
| 8 | auth-framework `src/auth_modular/mfa/sms_kit.rs:155` | yes | yes | an unzeroized SMS-provider `auth_token` |
| 9 | auth-framework `src/tokens/mod.rs:63` | yes | yes | an unzeroized `access_token` |
| 10 | auth-framework `src/threat_intelligence.rs:87` | yes | yes | an unzeroized `api_key` |
| 11 | auth-framework `src/server/oidc/oidc_response_modes.rs:93` | yes | no | a signed JARM response token handed to the client, not a stored credential |
| 12 | mlmf `crates/mlmf-source-hub/src/lib.rs:124` | yes | yes | an unzeroized registry `token` |
| 13 | synapse `crates/synapsed-client/src/lib.rs:134` | yes | yes | an unzeroized cached session `token` |
| 14 | auth-framework `src/cli/mod.rs:147` | yes | yes | a CLI `password` kept in a field |
| 15 | auth-framework `src/providers.rs:483` | yes | yes | an unzeroized OAuth `access_token` |
| 16 | auth-framework `src/authentication/credentials.rs:31` | yes | yes | an unzeroized `password` |
| 17 | fuel `fuel-dispatch/src/fkc/error.rs:176` | no | no | a DSL token in a parser error, not a credential |
| 18 | auth-framework `src/server/token_exchange/core.rs:85` | yes | yes | an unzeroized `subject_token` |
| 19 | synapse `src/synapse/auth/utils.rs:34` | yes | yes | an unzeroized `private_key` as `Vec<u8>` |
| 20 | auth-framework `src/methods/saml/mod.rs:50` | yes | yes | an unzeroized SAML `private_key` |
| 21 | auth-framework `src/auth.rs:243` | yes | yes | an unzeroized `new_refresh_token` |
| 22 | auth-framework `src/server/token_exchange/advanced_token_exchange.rs:258` | yes | yes | an unzeroized `subject_token` |
| 23 | auth-framework `src/auth_modular/mfa/sms_kit.rs:91` | yes | yes | an unzeroized SMS-provider `auth_token` |
| 24 | auth-framework `src/server/oauth/oauth2_server.rs:378` | yes | yes | an unzeroized `access_token` in a server response |
| 25 | auth-framework `src/auth_modular/mfa/sms_kit.rs:97` | yes | yes | an unzeroized SMS-provider `auth_token` |
| 26 | auth-framework `src/auth_modular/mfa/sms_kit.rs:175` | yes | yes | an unzeroized `secret_access_key` (a field kept only for configuration compatibility) |
| 27 | fuel `fuel-dispatch/src/fkc/error.rs:136` | no | no | a DSL token in a parser error, not a credential |
| 28 | auth-framework `src/protocols/indieauth.rs:80` | yes | yes | an unzeroized `access_token` |
| 29 | mlmf `src/distributed_loader.rs:449` | yes | yes | an unzeroized SMTP `password` |
| 30 | synapse `src/transport/nat_traversal.rs:32` | yes | yes | an unzeroized TURN `password` |

#### `debug-prints-secret-field` (20 findings; 20 judged; detector right 10, reviewer acts 10)

| # | site | right | acts | why |
|---|---|---|---|---|
| 1 | fuel `fuel-dispatch/src/fkc/error.rs:176` | no | no | a DSL token in a parser error |
| 2 | auth-framework `src/server/oauth/rich_authorization_requests.rs:532` | no | no | a pagination continuation token, an opaque cursor |
| 3 | auth-framework `src/protocols/acme.rs:219` | no | no | an ACME challenge token, public by protocol |
| 4 | auth-framework `src/protocols/oauth1.rs:24` | no | no | an OAuth 1.0 token identifier; the secret beside it is `Zeroizing` |
| 5 | mlmf `crates/mlmf-hf-tokenizer/src/lib.rs:63` | no | no | a vocabulary token |
| 6 | fuel `fuel-dispatch/src/fkc/error.rs:136` | no | no | a DSL token in a parser error |
| 7 | lightbulb `src/bin/lightbulb-cli.rs:18` | yes | yes | `derive(Debug)` on CLI arguments that hold an `api_key` |
| 8 | fuel `fuel-dispatch/src/fkc/error.rs:52` | no | no | a DSL token in a parser error |
| 9 | synapse `src/transport/mod.rs:190` | yes | yes | `derive(Debug)` on a TURN server with a `credential` |
| 10 | fuel `fuel-dispatch/src/fkc/error.rs:498` | no | no | a DSL token in a parser error |
| 11 | lightbulb `src/api/admin.rs:427` | yes | yes | `derive(Debug)` on a response that holds the raw `api_key` |
| 12 | lightbulb `src/engine/production_providers.rs:174` | yes | yes | `derive(Debug)` on a config with an `api_key` |
| 13 | lightbulb `src/api/admin.rs:873` | yes | yes | `derive(Debug)` on a response that holds the new raw `api_key` |
| 14 | mlmf `src/distributed_loader.rs:449` | yes | yes | `derive(Debug)` on an SMTP config with a `password` |
| 15 | mlmf `src/distributed.rs:370` | yes | yes | `derive(Debug)` on a config with an `api_key` |
| 16 | synapse `src/transport/nat_traversal.rs:32` | yes | yes | `derive(Debug)` on a TURN server with a `password` |
| 17 | synapse `src/synapse/auth/utils.rs:34` | yes | yes | `derive(Debug)` on a key pair with a `private_key` |
| 18 | synapse `src/certificate.rs:479` | no | no | a 32-byte public signing key in a validated-chain summary |
| 19 | lightbulb `src/api/mod.rs:92` | yes | yes | `derive(Debug)` on a config with a `jwt_secret` |
| 20 | baracuda `crates/baracuda-cuda-vocab/src/manifest.rs:78` | no | no | a CUDA architecture token |

#### `secret-named-value-in-log` (3 findings; 3 judged; detector right 2, reviewer acts 1)

| # | site | right | acts | why |
|---|---|---|---|---|
| 1 | synapse `src/bin/client.rs:353` | yes | no | a key generator that prints the private key on purpose ('keep this secret') |
| 2 | lightbulb `src/test_notice.rs:175` | no | no | a marker constant named `NOTICE_TOKEN` |
| 3 | auth-framework `src/api/oauth2.rs:1229` | yes | yes | logs the first ten characters of a bearer token |

#### `non-constant-time-compare` (4 findings; 4 judged; detector right 2, reviewer acts 2)

| # | site | right | acts | why |
|---|---|---|---|---|
| 1 | fuel `fuel-transformers/src/dev_tools/qwen3_yardstick.rs:333` | no | no | LLM token ids |
| 2 | synapse `src/transport/manager.rs:644` | no | no | a message digest compared for ack matching, public data |
| 3 | auth-framework `src/protocols/gnap.rs:865` | yes | yes | a continuation token compared with `!=`: a timing-comparable secret |
| 4 | auth-framework `src/auth.rs:1356` | yes | yes | a stored access token compared with `!=` |

#### `aead-without-aad` (7 findings; 7 judged; detector right 4, reviewer acts 2)

| # | site | right | acts | why |
|---|---|---|---|---|
| 1 | auth-framework `src/storage/encryption.rs:1310` | no | no | the project's own `decrypt(&envelope, key)` wrapper, which binds its AAD inside |
| 2 | auth-framework `src/security/enhanced_crypto.rs:84` | yes | no | ChaCha20-Poly1305 with no associated data; no binding requirement stated |
| 3 | auth-framework `src/server/oidc/oidc_advanced_jarm.rs:801` | yes | yes | JWE content encryption with no AAD: RFC 7516 makes the protected header the AAD |
| 4 | auth-framework `src/storage/encryption.rs:1430` | no | no | the project's own `decrypt` wrapper |
| 5 | auth-framework `src/storage/encryption.rs:1025` | no | no | the project's own `decrypt` wrapper |
| 6 | auth-framework `src/server/oidc/oidc_advanced_jarm.rs:1152` | yes | yes | JWE content decryption with no AAD: RFC 7516 makes the protected header the AAD |
| 7 | auth-framework `src/security/enhanced_crypto.rs:119` | yes | no | ChaCha20-Poly1305 with no associated data; no binding requirement stated |

#### `aead-empty-aad` (1 findings; 1 judged; detector right 1, reviewer acts 0)

| # | site | right | acts | why |
|---|---|---|---|---|
| 1 | auth-framework `src/storage/encryption.rs:706` | yes | no | the legacy v0 envelope decrypt, empty AAD by format, kept for migration (documented in the code) |

#### `weak-algorithm` (10 findings; 10 judged; detector right 9, reviewer acts 0)

| # | site | right | acts | why |
|---|---|---|---|---|
| 1 | auth-framework `src/protocols/hotp.rs:17` | yes | no | HOTP is HMAC-SHA-1 by RFC 4226 |
| 2 | auth-framework `src/protocols/ws_security.rs:428` | yes | no | the WS-Security UsernameToken digest is SHA-1 by the specification |
| 3 | auth-framework `src/protocols/radius.rs:595` | yes | no | RADIUS authenticators are MD5 by RFC 2865 |
| 4 | auth-framework `src/protocols/radius.rs:641` | yes | no | RADIUS response authenticator, MD5 by RFC 2865 |
| 5 | auth-framework `src/utils/breach_check.rs:59` | no | no | inside a test |
| 6 | auth-framework `src/protocols/radius.rs:653` | yes | no | RADIUS accounting authenticator, MD5 by RFC 2866 |
| 7 | auth-framework `src/utils/breach_check.rs:19` | yes | no | the Have I Been Pwned range API takes a SHA-1 prefix |
| 8 | auth-framework `src/utils/breach_check.rs:8` | yes | no | the same SHA-1 import |
| 9 | auth-framework `src/protocols/ws_security.rs:421` | yes | no | the same WS-Security SHA-1 import |
| 10 | auth-framework `src/protocols/radius.rs:22` | yes | no | the same RADIUS MD5 import |

#### `tls-verification-off` (2 findings; 2 judged; detector right 2, reviewer acts 1)

| # | site | right | acts | why |
|---|---|---|---|---|
| 1 | synapse `src/transport/quic_tls.rs:76` | yes | yes | a QUIC client that accepts any server certificate (`DangerAcceptAnyServerCert`) |
| 2 | auth-framework `src/server/core/common_http.rs:30` | yes | no | a configuration switch passed through (`accept_invalid_certs`); the default is the thing to check |

#### `hardcoded-key-material` (1 findings; 1 judged; detector right 1, reviewer acts 1)

| # | site | right | acts | why |
|---|---|---|---|---|
| 1 | auth-framework `src/server/oidc/oidc_extensions.rs:1069` | yes | yes | a development key used as the verification key when the environment variable is unset: the shape #218 and #222 removed elsewhere |

#### `hardcoded-secret:const` (5 findings; 5 judged; detector right 0, reviewer acts 0)

| # | site | right | acts | why |
|---|---|---|---|---|
| 1 | lightbulb `src/test_notice.rs:73` | no | no | a log-marker constant named `NOTICE_TOKEN` |
| 2 | synapse `src/keystore.rs:245` | no | no | the file name `signing.key.pem` |
| 3 | synapse `src/connectivity.rs:351` | no | no | a documented placeholder, 'not a credential' |
| 4 | mlmf `crates/mlmf-core/src/lib.rs:44` | no | no | a gate-marker constant named `NOTICE_TOKEN` |
| 5 | auth-framework `src/security/secure_jwt.rs:46` | no | no | a `typ` claim value |
