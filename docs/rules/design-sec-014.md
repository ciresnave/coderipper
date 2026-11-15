# SEC-014 design note: authentication and authorization on Rust entry points

> Design only. No code, no rule record, no check. Written by the coderipper-c lane at `origin/main` 842261a, 2026-11-13, for the PM to rule on.
> **Id (ruled by the PM, see "PM rulings"):** this rule is **SEC-014**; it was drafted as SEC-011. The row in [`REMAINING.md`](REMAINING.md) ("Authentication and authorization on Rust entry points",
> the Rust half of SEC-003 and SEC-010; scheduled in `RULES-REVIEW-2026-11-07.md` item 3). The 2026-10-24 security-crypto plan
> (`docs/superpowers/plans/2026-10-24-security-crypto-rules.md`) uses the same id for "secret field not zeroized"; one of the two needs another id before either is built.

## What the rule would say

An HTTP route of a Rust web service is reached by anyone who can reach the port. SEC-003 asks that every such handler enforce access
control, preferably in one deny-by-default layer. The catalog's SEC-003 excludes Rust (`languages = ["typescript","python"]`), so a Rust
service gets no reading at all; SEC-014 would be that reading, first as an **inventory** (which routes exist, which of them are behind an
authentication layer or extractor) and only later as a judgement.

## Denominator: how much Rust here serves HTTP

Measured at each repository's `HEAD` over every tracked `Cargo.toml` (members included), by a dependency key named `axum`, `actix-web`,
`rocket`, `warp`, `poem` or `salvo`; checkouts under `C:\Projects` on 2026-11-13, worktrees (`*-wt-*`) and the two `auth-framework-ghsa-*`
copies excluded as duplicates, 175 non-worktree directories (not all Rust):

| Framework | Repositories that depend on it |
| :--- | :--- |
| axum | 7 (HumboldtUnifiedKidTracker, auth-framework, commy-sdk-rust, httpant, lightbulb, role-system, synapse) |
| actix-web | 3 (auth-framework, httpant, role-system) |
| warp | 3 (auth-framework, httpant, role-system) |
| rocket | 1 (role-system) |
| poem, salvo | 1 each (httpant) |

Seven distinct repositories in all. `role-system`, `auth-framework` and `httpant` are libraries or comparison harnesses that name several
frameworks (integrations, benchmarks), not services; the services are the minority. A manifest key does not say the dependency is a
default one or that a route exists, so these are upper bounds on where the rule would read anything. I did not open the seven to count which are services (an open item).
**The corpus for a false-positive read is therefore at most seven repositories and probably fewer, and the rule should ship opt-in until a measured read says otherwise.**

## The route inventory, per framework

A route is a `(method, path, handler)` triple. What `syn` can see without types:

- **axum:** `Router::new().route("/p", get(h).post(h2))`, `.nest("/p", router)`, `.merge(router)`, `.layer(L)`, `.route_layer(L)`; a handler is
  a path or a closure. Authentication usually enters as a `.layer(..)` / `.route_layer(..)` (tower middleware such as
  `from_fn(auth)`, `RequireAuthorizationLayer`) or as an extractor argument of the handler (`user: AuthUser`, `Claims`, `TypedHeader<Authorization<..>>`).
- **actix-web:** `App::new().route(..)`, `.service(web::resource("/p").route(web::get().to(h)))`, `.wrap(mw)`, `web::scope("/p").wrap(mw)`, and attribute
  macros `#[get("/p")]`, `#[post("/p")]` on handlers, which `syn` sees as attributes on a function.
- **rocket:** attribute macros `#[get("/p")]` etc. on handlers and `routes![a, b]` in `mount`; guards are handler arguments (`user: User`, `_auth: ApiKey`)
  and fairings attach globally.
- **warp:** filter chains: `warp::path("p").and(warp::get()).and(with_auth()).and_then(h)`, combined with `.or(..)`; authentication is a filter in the chain.

## What can honestly be judged

1. **Inventory (Info).** One finding per package: how many routes, per framework, and how many have a recognised auth layer or extractor
   (names from a list: `auth`, `Auth`, `Claims`, `Bearer`, `ApiKey`, `Session`, `User`, `RequireAuthorization`, `jwt`, `login_required`). Says "found", never "safe".
2. **A route with no recognised authentication, in a service that has some elsewhere (Low, Low confidence).** Where at least one route of the package is
   recognised as authenticated and another, not under the same layer or scope, is not, the second is reported. A service with *no* recognised
   authentication anywhere is reported once as an inventory finding, because an intentionally public API and a forgotten layer look the same.
3. **Not judged:** whether the policy is correct, object-level or tenant checks (SEC-003's second half), whether a check is in a called function
   (`if !user.can(..) { return 403 }` inside the body), a layer added by a wrapper crate, a service behind a gateway that authenticates, a
   route registered through a macro, a router assembled across files and returned from helper functions (cut 1 follows only calls inside one function body).

## Expected noise and why it ships opt-in

- Health, metrics, login, registration, OAuth callback and documentation routes are public by design; a path-name allowlist (`/health`, `/healthz`, `/ping`, `/metrics`,
  `/login`, `/signup`, `/auth/*`, `/.well-known/*`, `/docs`, `/openapi.json`) removes the obvious ones and not the rest.
- A repository that is itself an authentication library (`auth-framework`, `role-system`) has examples and tests that mount unauthenticated routes on purpose; those
  files are already out of the Rust walks (`tests/`, `examples/`), the library `src/` is not.
- Auth done by a name the list does not know reads as missing. The list will need a `[security] auth_names` key (the ROADMAP's R3 row already plans `[security] entry_points` for SEC-012/013; the same table is the natural home).

## Staging

1. **Cut 1:** axum and actix-web, inventory only (finding 1), default on per the PM ruling below; Info severity; a conformance pair, a patent sweep, and a read of every portfolio service by hand.
2. **Cut 2:** rocket and warp inventory; the "route with no recognised authentication" finding (2), opt-in, Low/Low.
3. **Cut 3 (if the read supports it):** SEC-003 and SEC-008 gain Rust applicability in the catalog, `[security] auth_names`, default-on only if the false-positive rate on real services is low with its denominator stated.

## PM rulings

- **Id: SEC-014** (free). The zeroize rule keeps SEC-011. The slug migration is coderipper#281.
- **Inventory Info finding: wanted, default ON.** One finding per package, never a judgement.
- **"Route with no recognised authentication": Low severity, Low confidence, opt-in** until a read of real services supplies a denominator.
- **Denominator:** 7 repositories depend on a Rust web framework (table above), an upper bound on services.

— lane: coderipper-c
