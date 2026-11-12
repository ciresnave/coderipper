<!-- addendum -->
## Addendum: the {{LINE}} line ({{DATE}}, {{VERSION}})

Version line covered by this addendum: **{{LINE}}** ({{VERSION}}). One rule is added, read from `src/module/neutral/test_artifact_path.rs`: `test-artifact-path` (a Rust test that finds a built artifact by walking up from `current_exe()` instead of asking Cargo). Swept {{DATE}} by the coderipper-c lane (Claude Sonnet 5.5), same method and limits as above.

**Idea {{IDEA1}} (`test-artifact-path`).** Parse Rust test code, find a path derived from a call to `current_exe` by going up (`parent`, `ancestors`, `pop`, `with_file_name`) or by joining a build-directory name (`examples`, `deps`, `target`, `debug`, `release`), and report it unless the same function reads one of Cargo's own path variables. Matching a call, a walking method and a directory literal inside one function is the whole mechanism; nothing is built or run.

| # | Idea | Query ({{DATE}}) |
|---|---|---|
| Q{{Q1}} | {{IDEA1}} | patent static analysis detect hard-coded path to build output derived from running executable location in test code instead of build system provided environment variable |
| Q{{Q2}} | {{IDEA1}} | patent.google.com automatically detect test code that locates sibling binary by traversing parent directories of current executable path brittle test build layout dependency source code analysis |

**Positive controls:** Q{{Q1}} returned US10482262 (static analysis based on abstract program representations), US10048960 (identifying source code used to build executable files), Klocwork's hard-coded search-path checker and the `noabspath` tool, so the engine reaches the neighbourhood. Q{{Q2}} returned US11048798 (detecting libraries in program binaries), US10241769 (marking sibling caller routines) and test-detection heuristics in open-source tools, so it reaches the neighbourhood too. **A patent that reports test code for deriving a build-output path from the running executable's location, from source, was not found; that absence is a statement about these two queries only** (one engine, no USPTO or Espacenet search).

**Read:** the claims of **US10048960B2** (Identifying source code used to build executable files, Semmle, now Microsoft; filed 2014-12-17; Google Patents shows it Active) through `tools/patent-claims.py`, all three independent claims (1, 16, 31). Each requires intercepting operating-system requests that start a compiler and a linker, hashing the object files, generating object and link artifacts, and matching the hash values in a static analysis system to associate an executable with the source files that built it. **Not read in full:** US11048798, US10241769, US10482262 and every other result named above, by title and snippet only.

**Result: no hit reads on `test-artifact-path`.** The rule intercepts no process start, hashes no object file and associates no executable with its sources; it reads a function body and looks for a call, a walking method and a directory literal. **Watch:** re-read US10048960 before the rule resolves which executable a path names rather than naming the walk.

**Limits:** two queries, one search engine, no USPTO, Espacenet or WIPO full-text search, no citation chasing, snippets not claims (except the one patent read), and the status shown is Google Patents', not the official record. A due-diligence read, not legal advice.
