kind: Fix
---
**SEC-012 reads the field's type as well as its name.** A numeric field or collection (a `Vec`/set of non-byte numbers, a map with numeric values), the companions `names`/`num`/`number`, and a lexer `token` (a vocabulary-prefixed name such as `bos_token`, or a `token` in a struct or file whose name says vocab, tokeniz or lexer) are no longer reported as secrets. `Vec<u8>`, `Vec<String>` and string maps still are, and a `password` beside a vocabulary `token` is still reported. Measured with the new binary on checkouts of `main`: baracuda 2 findings to 0; auth-framework 79 findings, unchanged (those are `String` tokens a type cannot clear); the other repos are listed in the PR. A `String` that holds a secret's name (a field called `secret`) is still flagged: rename it `secret_name`.

— lane: coderipper
