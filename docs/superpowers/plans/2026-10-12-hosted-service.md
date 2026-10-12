# Hosted CodeRipper: sandbox, abuse policy and community-post scope — Design

> **Status: design only; nothing is built, no cargo was run for it.** Requested by the PM on 2026-10-12 after CireSnave ruled the
> hosted plan's six product questions (board 180). It continues `2026-10-04-three-faces.md` §3 (the decision paragraph) and depends on
> `2026-10-12-redaction-pass.md` (the precondition for showing third-party output). **Not verified:** what the runner provider can do
> (none is chosen), and the Community lane's post API (answered by the Community lane on 2026-10-12 from its code; §6).

## 1. What CireSnave decided (2026-10-12, verbatim)

> *"On decision 180: 1) Both...but initially run on their CI with a dashboard on our site. 2) Submitter's choice. 3) On their CI, no limit.
> On ours, no limit but deprioritize by total used by a user account. 4) Login required. 5) Yes...but only heavily sandboxed.
> 6/B) Results post as a post on community. Default results retention is short (maybe a month) but that default should be settable by the user."*

PM rulings on this design's first questions (2026-10-12): **B v1 scans only public repositories the logged-in user can push to**; a
**per-account concurrent-job cap (2) exists as a safety measure, not a quota, and the docs say so**; **retention defaults to 30 days,
is user-settable, with a ceiling of 365 days; the Community post persists as a summary and its link says "expired"**. Runner provider
and a cost ceiling were answered by CireSnave (board 187a): the runners are his VPS and spare home PCs and his cost does not change
with the number of jobs, so **the limits are capacity, not money** (§10). The terms of service and takedown contact are
CireSnave's (board 187).

## 2. Shape: two faces of one service

| | **A: results from the user's CI** | **B: scan by URL on our compute** |
|---|---|---|
| Who runs the code | the user's own CI runner | our sandbox |
| Untrusted code on our infrastructure | **none** | yes, heavily sandboxed |
| Untrusted *data* on our infrastructure | yes: uploaded JSON | yes: tool output and findings |
| Order | **first** | after a sandbox spike on the chosen provider |
| Limit | none (his #3) | no quota; fair-share ordering and a safety cap (§5) |

Both write the same **result document**, which is what the dashboard and the community post render. Building the result document, its
schema and the upload endpoint is the first build step; B reuses all of it.

### 2.1 Face A: the upload

`cargo coderipper --message-format json` runs in the user's workflow and the JSON is uploaded, authenticated by the repository's GitHub
OIDC token. The token proves **which repository** uploaded, not that CodeRipper ran or that the JSON is honest, and a pull request from a
fork gets no `id-token`. So the ingest treats the upload as attacker-controlled text:

- validate against the versioned schema, reject unknown `schema_version`; size caps on the body and on every string and list;
- apply the redactor (`redact::text`, the redaction-pass design) **on receipt**, and strip anything that looks like a local path prefix;
- store as data, never as markup; the dashboard escapes on render and the community summary is built from escaped fields only;
- a result is labelled with what the token proves ("uploaded by workflow X of repository Y at commit Z"), never "verified by CodeRipper";
- a login is required (his #4): the OIDC repository must belong to, or be writable by, the account that owns the dashboard entry
  (checked through the GitHub API at upload time).

## 3. Sandbox design for face B

The sandbox is the only thing between a stranger's `build.rs` / procedural macro and our infrastructure; the design assumes the
scanned code is hostile.

1. **One fresh isolated runner per job, destroyed after.** A microVM (Firecracker class) or a gVisor container; never reused between
   jobs, never shared between accounts. No privileged mode, no nested virtualisation, no host mounts.
2. **Fetch happens outside the runner's trust.** The host downloads a **pinned-SHA tarball** (never `git clone`: no hooks, no
   submodules, no LFS), with a size cap, rejecting symlinks, absolute paths and `..` entries on extraction. The repository is mounted
   read-only; the build writes to a scratch overlay.
3. **Three phases, with the network removed where code runs:**
   - *Resolve*: `cargo fetch --locked` with egress limited to crates.io (and its download host) through a proxy that allows nothing else.
     No repository code runs here (a `build.rs` runs at build time, not at fetch).
   - *Build / check*: **no network at all.** Build scripts and procedural macros cannot exfiltrate or fetch a second stage. A build that
     needs the network fails, and the run says "could not run" with the redacted cause: **a gap, never a pass** (the standing rule that
     no input is a gap, applied to a build).
   - *Report*: the host reads one size-capped result file; there is no other channel out.
4. **No secrets in the runner.** No environment tokens, no cloud metadata endpoint reachable, no credentials of ours; the GitHub API
   calls for the account's permission check happen on the host, not in the runner.
5. **Resource caps (safety, not quota):** CPU, memory, disk, process count and **wall clock (propose 15 minutes)**, enforced by the
   hypervisor, not by the job. A job over a cap is killed and reported as a gap with the cap named.
6. **Caches are per account and repository only**, keyed by repository id and lockfile hash. A shared build cache is a poisoning
   channel between accounts; the cost of not sharing it is build minutes, which is what the fair-share ordering prices.
7. **Tools come from the image**, installed from `src/tools/tools.lock` with their hashes verified, not downloaded by the job.
8. **Output policy:** `RedactionPolicy::On`, with a build script's output **withheld** (the fixed sentence of the redaction-pass
   design), the scratch-directory prefix stripped, and the result size-capped. The redaction pass is a precondition of B; it is not a
   precondition of A's first dashboard, but A applies the same redactor on receipt.
9. **Delegated tools that need the network are code-free but not free of risk.** `osv-scanner` (SUP-002/SEC-006) needs the OSV API;
   `lychee` (DOC-010) fetches arbitrary URLs written in the submitter's documents (server-side request forgery against our network).
   They run in a **separate job class that executes no repository code**, behind an allowlisting proxy that blocks private, loopback and
   link-local ranges and the metadata address. **Recommendation: DOC-010 stays off in hosted v1** (it reports a gap there, which the
   coverage table says); osv is allowlisted to the OSV host only.

Which checks run where (read from the checks' own behaviour in `three-faces.md` §3 and the delegation framework; the buf and zizmor
rows are **not re-verified** for plugin or network use and must be before B opens):

| Class | Rules | Why |
|---|---|---|
| builds repository code | `reachability`, `unused-return-values`, `unused-parameters` | `cargo` runs `build.rs` and proc-macros |
| reads files only | `version-consistency` (`cargo metadata --no-deps`), neutral rules, `gitleaks`, `zizmor`, `buf` | no repository code runs; confirm per tool |
| needs the network, runs no repository code | `osv-scanner`, `lychee`, `ci-protection-presence` | allowlisted egress or off |

## 4. Result document (the integration point)

A versioned JSON document (`schema_version`, tool version, ref scanned, per-rule outcome, findings with severity and location, notes,
errors, the redaction count and the policy that applied), built from the library's `RunResult`. Properties the design requires:

- **Deterministic and replayable:** the same document renders the dashboard page, the community summary and an API response.
- **Every string is attacker-controlled** and carries no markup. Rendering escapes; links are `rel="nofollow ugc noopener"`.
- **Never claims coverage it does not have:** a rule that could not run (no network, build failed, cap hit) is listed as such, with the
  reason, and a run's "all clear" is only ever "every rule that could run, ran, and found nothing".
- An **idempotency key** per scan (account, repository, commit, profile) so a retry cannot create a second post or a second row.

## 5. Abuse policy

| Concern | Control |
|---|---|
| Anonymous abuse | **Login required** (his #4). `auth-framework` when ready; until then GitHub OAuth, which is also what answers "can this user push to this repository" |
| Scanning someone else's repo and posting it | **v1 B scans only a public repository the logged-in user can push to**, verified through the GitHub API at submit time (PM, 2026-10-12). Private repositories use face A on their own CI |
| One account starving the rest | **Fair share, not a cap** (his #3 and his scheduling ruling of 2026-10-12): fewest runs in the last month first, then least resource used; see §10. Global backpressure on queue depth is a capacity limit, not a quota |
| One account occupying every runner | **Per-account concurrent-job cap of 2.** A safety measure on parallelism, not a quota on total use; stated as such in the user-facing docs (PM, 2026-10-12). Any change is CireSnave's, because it sits beside "no limit" |
| Outbound attacks from our runners | no egress during build (§3.3); the resolve proxy allows crates.io only |
| Mining / pegged CPU | bounded by wall clock; flagged when CPU is saturated with no cargo progress; repeated timeouts deprioritise the account and can suspend it |
| Hostile tarballs | extraction rules in §3.2; size cap |
| Hostile result text | schema, caps, redaction, escaping (§2.1, §4) |
| Our own errors | kill switch per account and per repository; ban list; **job metadata (not contents) logged** with a retention of its own (to be set with the terms) |
| Terms of service, takedown contact | **CireSnave's** (board 187); B does not open before they exist |

## 6. Community post integration (scope, not a build)

CireSnave: *"Results post as a post on community."* **Finding from the Community lane (2026-10-12, read from its `origin/main` at
`7caf172`, cited by file there): today a service cannot post as a user.** Posts are created only by a browser session
(session cookie, CSRF double-submit, verified email; the author id comes from the session, never the body). There is no bearer or
API-key route, no service identity, no post type, no per-post visibility, no idempotency key, and no per-account rate limit on
`POST /posts`. Community accounts are its own (Argon2id); `auth-framework` is a design reference there, not a dependency, so there is
**no shared login** with this service yet. A service holding a user's password is rejected as the wrong blast radius.

What exists and can be relied on: `PATCH /posts/:id` replaces title, markdown, status and tags in place; the markdown limit is
100,000 characters; the renderer sanitises on read, forces `rel="nofollow ugc noopener noreferrer"` on external links, and the page CSP
allows images only from `'self'` and `cdn.thinkersjournal.com` (so a remote image in a result post will not render); a post has no
expiry (it lives until its author deletes it, the account is removed, or moderation hides it).

**What this means for the design:**

- **CodeRipper's side is unchanged and small:** the result document, an escaped markdown summary of at most about 10 KB (our own cap,
  far under Community's), the dashboard link, and an idempotency key *of ours*. The post is made **when the submitter asks**.
- **Until Community has a delegated-credential route, a result post is made by the user's own browser session**, three ways that need
  nothing from Community: (a) our dashboard shows the summary markdown with a copy button and the Community "new post" link; (b) the
  same with a prefilled draft if Community's composer ever accepts one (not today); (c) the post is made later by a delegated route.
  **Recommendation: (a) for face A's first dashboard**, which changes nothing in Community, and (c) only if CireSnave wants the post made
  by the service, because (c) is a new authentication model, an endpoint, an idempotency key, a per-author rate budget and an abuse
  answer for automated posting, all of it **Community's lane to design and CireSnave's to approve** (it said "M4-adjacent").
- **We must persist the Community post id ourselves** to update a re-scan in place with `PATCH`, and a lost create response is a
  duplicate until Community adds an idempotency key.
- **Plan for the post disappearing:** moderation may hide a post that quotes attacker-controlled repository text. The dashboard
  result is the record; the post is a copy.
- **Expiry is ours only:** results expire per the account's retention (default 30 days, ceiling 365); the post persists as a summary
  and the dashboard link reads "expired" (PM, 2026-10-12). Nothing on Community's side will remove it.
- **Volume:** each published post triggers cache purges whose quota is scarce on Community's side. Face B must not post
  automatically at volume: posts are submitter-initiated, one per scan at most.
- **Open, for CireSnave via the PM:** whether "post as a post on community" requires the service to post (route (c)) or accepts the
  user posting a prepared summary (route (a)).

### 6.1 Outcome: route A agreed with the Community lane (2026-10-12)

The Community lane did **not** want route (c) for v1 (a delegated credential, a per-author budget and an abuse path are unjustified
when the user can post from their own session), and neither does this lane; the PM's instruction was "A = user's own session +
idempotency key". Agreed contract, **proposals from the Community lane, nothing built on either side**:

- **Prefill by URL fragment, not a cross-site POST** (a cross-origin write cannot work: the session cookie is `SameSite=Lax` and the
  API accepts non-GET only from the community origin): `GET https://community.thinkersjournal.com/new-post#cr=<base64url(JSON)>`.
  The fragment never reaches a server (no logs, no Referer, no URL length limit); a small bundled script reads it once, sets the
  title, textarea and tags by `.value` (never `innerHTML`), and clears the hash. Without JavaScript the editor is empty and nothing
  breaks. The user still presses Publish. Payload: `{title, markdownSource, tags: ["coderipper"], idempotencyKey}`; the body is
  sanitised on render and the 100,000-character cap applies.
- **Idempotency key:** header `Idempotency-Key` on `POST /posts` (`[A-Za-z0-9_-]`, 16-64 characters), scoped to
  `(author_id, key)`; the same key and body returns the original post, the same key with a different body is a 422, no header is
  today's behaviour. Our key is the **result id**: one scan result, one key; a re-scan gets a new key. **Community builds this
  later: it needs a database migration applied by CireSnave before merge and a version from the PM; no date.**
- **Update in place:** the user pastes the post's URL back to our dashboard; we then show text for `PATCH /posts/:id` (full replace
  of title, markdown, status and tags). A server-side return-to-dashboard redirect is not in scope (an open-redirect guard would be
  needed); say if it is wanted.
- **Tag:** `coderipper` (freeform, slugified, no schema change; at most 5 tags per post).
- Until the idempotency key exists, a double submit by the user makes two posts; the dashboard shows the post id once pasted back.

## 7. Build order (each step its own PR; nothing before the PM's go)

1. Redaction pass: the redactor and its tests, then wiring and `--redact` (`2026-10-12-redaction-pass.md` §8 decisions still open).
2. The result document and its JSON schema (library, additive), plus the ingest validator as a library function so it is testable
   without a service.
3. Face A: GitHub Action, upload endpoint, dashboard page, retention setting. Needs the site's stack from the front-end lanes; **not
   decided here**.
4. Community post: route (a) of §6 needs nothing from Community; route (c) needs its new delegated-credential route.
5. Sandbox spike on the chosen provider (CireSnave's decision, board 187): one job class, measured, with the escape and exfiltration
   attempts in §8 as its acceptance test.
6. Face B behind a flag, public repositories the account can push to, concurrency cap 2, fair-share queue.

## 8. What would convince us the sandbox holds (written before it exists)

A hostile fixture set, each of which must fail to escape and be reported as a gap or a redacted failure, never as a pass:
a `build.rs` that opens a socket; one that reads the environment and the metadata address; one that writes outside the scratch
overlay; one that fork-bombs; one that sleeps past the wall clock; one that writes gigabytes; a proc-macro that does the same; a
tarball with a symlink to `/etc/passwd` and one with `../` entries; a document with a link to `http://169.254.169.254/` and to a
private address (DOC-010 must not follow it); a finding whose text contains markup and a fake token (the escape and redaction
tests of §4 and the redaction pass). The conformance runner's "fixtures are code that runs" warning applies: these run **only** in
the sandbox, never on a developer machine or in this repository's CI.

## 9. Limits, stated up front

- This is a design. No provider, cost, schema or API exists yet; every number marked "propose" is a starting value.
- Face A cannot prove a result is honest; face B can, because we ran it. The dashboard says which.
- "Heavily sandboxed" is a claim to be earned by §8's fixtures, not asserted here.
- Findings about a third party's repository are not a concern in v1 B by construction (§5); face A runs on the user's own workflow
  and can name any repository the user's token can read, which the GitHub permission check bounds.

## 10. Scheduling and capacity (CireSnave, 2026-10-12)

> *"we will give priority to those users who have run the least runs within the last month and, second to that, those who have used the
> least resources.  That way, only the real resource hogs get slowed down or delayed."*
>
> *"Runners are on systems like my VPS and my spare PCs at home.  As such, no matter how many runners run in a month, my cost is exactly
> the same."* (board 187a)

So there is **no per-job money cost and no dollar ceiling**; the limits are **capacity** (CPU, memory, disk, queue length, uptime of
machines that are not data-centre machines), and **the runners are CireSnave's own machines running strangers' jobs**, so isolation
matters more, not less (an escape lands on his network). This section replaces the "compute-seconds ordering" wording in §5.

### 10.1 The ordering

A queued job's sort key, smallest first:

1. **runs in the trailing 30 days** for its account (jobs that *started*; a job cancelled before it started does not count; a job
   re-queued because its runner vanished counts once);
2. **resource used in the trailing 30 days** for the account (see 10.2);
3. **submission time** (first in, first out among equals).

The account's two numbers are read **when a runner becomes free**, not when the job was submitted, so a user who has just run twenty jobs
moves behind a user who has run none, including jobs already waiting. That is the intent of "only the real resource hogs get slowed
down". The ordering is published (the terms say so).

### 10.2 What "resource" means

Measured by the host, never by the job: **CPU-seconds actually consumed** (the primary number), plus **peak memory x wall-clock
seconds** and **peak disk**, recorded per job and kept as job metadata. For ordering they are combined into one number with weights
that reflect which resource is scarce on the runner that ran it (a runner short of memory weights memory); **the weights are tunable
and are not measured yet** (open: they need a few real jobs on the real machines, the measurement of §8's spike). Two things are
deliberately **not** in it: the repository's size and the result's severity.

### 10.3 What the ordering does not do

- **It is not a quota.** Nobody is refused for total use; they wait behind lighter users. The per-account **concurrent cap of 2**
  remains and is stated as a safety limit.
- **Starvation.** Strict priority can delay a heavy user for as long as lighter users keep arriving, which is what was asked for. The
  design adds one **recommended** guard, not a ruling: a waiting job's key is improved by the time it has waited (after `{N}` hours it
  is next), so a job is never starved forever by a stream of lighter ones. Open for CireSnave: yes/no and `{N}` (propose 24).
- **Gaming.** Many accounts split one person's usage; login is required and one person one account is a term, but the real control is
  the push-access rule (a repository can be scanned only by someone who can push to it).

### 10.4 Capacity limits on his machines

| Limit | Behaviour |
|---|---|
| Per-runner CPU, memory, disk | each runner advertises its capacity; a job declares nothing (the host picks the smallest slot the repository's workspace needs, with the caps of §3.5) and is killed at the cap, reported as a gap with the cap named |
| Queue length | a **global queue ceiling**; past it a new submission is refused with "try again later" and nothing is discarded that was accepted. Capacity, not a user quota |
| A machine goes away (a home PC sleeps, a VPS reboots) | jobs run under a lease with a heartbeat; a lost job is re-queued whole and counted once; a runner that fails twice is taken out of the pool until a person returns it |
| Heterogeneous machines | a runner pool, each with a label (VPS, home); a job that needs more than a home PC offers waits for the VPS |
| Many jobs, one machine | one job per isolated runner VM; per-runner concurrency is set by cores and memory, not by the account cap |

### 10.5 The runners are untrusted-job hosts (stricter than §3)

Because the machines are his own, §3's isolation is a **minimum**, with these added:

- **Hardware-virtualised isolation only** (a microVM or a full VM); a container sharing the host kernel is not enough on a machine
  that also holds personal files. No job gets a host mount, a GPU, USB or the clipboard.
- **Network:** the runner hosts sit on a **separate network segment with no route to the home LAN, router or other machines** (a guest
  VLAN or a VPS-only network); egress is the §3 resolve proxy only, enforced **on the host**, not in the guest.
- **Nothing else of his lives in the job's reach**: a dedicated OS user or VM host for the runner service, no personal credentials,
  SSH agent, cloud tokens or password-manager session on a runner host, and no shared folders with the owner's desktop.
- **Disk:** per-job overlay destroyed after the job; the host's free space is a capacity limit (a job that fills it is killed).
- **A home PC is a worse host than a VPS** (it also holds the owner's data, it is rebooted, it is on a home connection and a home
  address). The recommendation is **VPS first for face B**, home PCs only after the segment and VM isolation above are in place and
  tested with §8's hostile fixtures.
- Open for CireSnave: whether a home PC will be used at all in v1.

### 10.6 Metrics that must exist before B opens

Queue depth and age, per-account runs and resource over 30 days, per-runner utilisation, jobs killed at a cap by cap, jobs re-queued,
and runner health. They are what makes "only the hogs wait" checkable; without them the ordering cannot be shown to work.
