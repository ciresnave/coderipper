# Package C: ARC-004 infrastructure types in policy signatures, ARC-012 construction only in composition roots

Read [README](README.md) first. Both build on `[architecture]` (`components`, `volatile`, `composition`) and the public-surface reader in `api_budget.rs`.

**ARC-004**: a public item's signature (parameter, return, field type) in a *policy* component names a type from a `volatile` component or a
declared infrastructure pattern (ORM entity, request object, transaction handle, framework type; allowlist in `[arc004] patterns`).
Resolve names through `use` paths (`modgraph::resolve_abs`), not text. Explicit exceptions (annotation-only derives on value types) by waiver.
ARC-003 flags *imports*; this flags the *signature*, so check that one site is not reported twice.

**ARC-012**: concrete construction (`Type::new(..)`, struct literal) of a type that implements a declared port or trait, outside the
`composition` modules. Tests and `main` are exempt. Shapes only; start from trait impls (`Item::Impl.trait_`, the syn 3 tuple shape) as `layers.rs` does.
Opt-in if the corpus shows more than a few false positives.

Patent read first: dependency-injection and composition-root detection tools; claim charts only for hits with live claims on detecting construction outside a root.

Done when: both registered, fixtures, corpus numbers, patent rows, fragments, READY. Do ARC-004 first.
