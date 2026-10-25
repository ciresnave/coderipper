# The hardening gate: nothing runs a stranger's code before CireSnave's go

**Status: a checklist for CireSnave to approve. Nothing on it is built, and no job runs anywhere.** It turns three of his rulings into a gate:

> *"H4: Home PC first but we do our best to make sure everything is hardened before we even consider it."* (CireSnave, 2026-10-25)

and the standing position of `docs/superpowers/plans/2026-10-12-hosted-service.md` (§8, §10.5) and `docs/superpowers/plans/2026-10-13-runner-isolation.md` (requirements R1 to R10, §8): the hosted service builds repositories that strangers submit, a build runs the stranger's own code (`build.rs`, procedural macros), and the first host is his own PC, which also holds his files and sits on his home network. So the gate is written first, from the hostile side, and a box is ticked only with the evidence beside it.

**How it works.** Every row below has a plain-language meaning, a thing that must be shown, and a place for the evidence (a test log with the host class and the date, kept with the result). CodeRipper's lane proposes the rows and runs the fixtures; **CireSnave approves the finished list, and then separately gives the go**. Until his go, face B stays closed, no submitted repository is built on any host, and the fixtures themselves run only inside the sandbox they test, never on a developer machine or in this repository's CI. A host that fails any row is not used. A change of operating system, hypervisor or isolation stack is a new host class and starts the list again.

## A. The machine and the account

| # | Must be true | Plain meaning | Evidence |
|---|---|---|---|
| A1 | The runner runs as a **dedicated non-administrator account** with no personal files, no SSH agent, no cloud tokens and no password-manager session | if the runner is taken over, it holds nothing of yours | listing of the account's rights and files; the account cannot read your profile |
| A2 | **No inbound listener**: the runner polls the service over TLS; nothing connects to the PC from outside | there is no door to knock on | port scan from the LAN and from outside shows nothing the runner opened |
| A3 | The runner agent and its image are **signed**; an unsigned update is refused | a fake update cannot take over the runner | a tampered update is refused (test) |
| A4 | The host is patched, with hardware virtualisation on, and the runner **refuses jobs** when its start-up self-test fails (the self-test in the isolation plan, §6) | it checks its own locks before every shift | tamper with the rules; the agent takes no job |

## B. The guest (the throwaway machine the build runs in)

| # | Must be true | Plain meaning | Evidence |
|---|---|---|---|
| B1 | **One virtual machine per job**, started from a read-only image, destroyed after, never reused (R1) | every job gets a fresh machine, and nothing carries over | after success, failure, timeout and forced kill, no disk, process, mount or network namespace remains |
| B2 | **Hardware-virtualised** (a microVM or a full VM), never a plain container (R2) | a real wall, not a curtain | the isolation stack is named and is not a shared-kernel container |
| B3 | The guest sees **only** the repository (read-only) and a result channel: no shared folders, host mounts, clipboard, USB, GPU or host identity (R3) | the guest cannot touch your desktop or files | the guest's device list equals the allowlist |
| B4 | **No credentials of any kind** in the guest (R4) | nothing to steal inside | environment and filesystem listings from a probe job |
| B5 | **Resource caps enforced from outside**: CPU, memory, disk, process count and wall-clock time (30 minutes by default in the isolation plan R5; the hosted-service plan §3 proposes 15, to be settled), killed by the host's timer (R5) | a runaway build is stopped by someone who is not the build | fork bomb, memory balloon, disk fill, infinite build and detached children are each killed at the cap, host unaffected |

## C. The network

| # | Must be true | Plain meaning | Evidence |
|---|---|---|---|
| C1 | The guest's only route is a proxy on the host side of a private link with **no other route**, enforced on the host (R6) | the only door leads to a guard | direct attempts to the gateway, a LAN address, `127.0.0.1` on the host, link-local and a metadata address all fail and are logged |
| C2 | **The home network is unreachable by construction**: a separate segment or virtual switch with no route to the LAN, router or other machines | even a perfect escape from the guest lands in an empty room | from the runner's segment, nothing on the LAN answers |
| C3 | DNS goes only through the proxy; IP literals refused; private, loopback and link-local answers refused after resolution | a trick name cannot point back inside | a direct resolver, a DNS-over-HTTPS route, a name that resolves to a private address and an IP literal all fail |
| C4 | During the build and checks there is **no network at all** except an approved domain for that repository; dependency resolution may reach only the crate registry's index and download hosts | a build script cannot fetch a second stage or send data out | a `build.rs` that opens a socket fails; a build that needs an unapproved host reports "could not run", never a pass |
| C5 | DNS tunnelling and abuse of an allowed host are capped | smuggling data in lookups, or hammering a permitted host, is stopped | volume and connection caps are hit and the job stops |

## D. The inputs and outputs

| # | Must be true | Plain meaning | Evidence |
|---|---|---|---|
| D1 | Unpacking the repository and its dependencies is bounded and done by an unprivileged process (or a throwaway guest) | a booby-trapped archive cannot hurt the host | traversal, symlink and archive-bomb fixtures are contained |
| D2 | A result is **untrusted data**: only defined fields, bounded in size; anything else is dropped and counted (R8) | the guest cannot send the host anything but a report | oversized, malformed and crafted results are bounded or rejected |
| D3 | A build script's output is **withheld** and the rest is redacted before anyone sees it | a hostile program cannot print secrets or fake text into a report | the redaction pass is built and its tests pass (a precondition recorded in the hosted-service plan) |
| D4 | Every approval to leave the default network is recorded and listed in the report (R9) | anyone reading a result can see what the build could reach | an approval appears in the report |

## E. Proof, not promises

| # | Must be true | Plain meaning | Evidence |
|---|---|---|---|
| E1 | The **hostile fixture set** passes on this host class: a `build.rs` that opens a socket, reads the environment, reaches the metadata address, writes outside its area, fork-bombs, sleeps past the clock or writes gigabytes; a procedural macro that does the same; archives with `../` and symlinks; a document linking to `http://169.254.169.254/` | we attacked it ourselves, and it held | the table in the isolation plan's launch gate (§8), with each fixture confirmed to fail when its defence is removed (mutation-checked), kept with host class and date |
| E2 | No secret of yours is reachable from the guest, the runner account or the segment | a final look from the attacker's chair | an independent pass over A1, B4 and C2 by a lane that did not build them |
| E3 | The metrics exist before the first job (queue age, jobs killed at a cap by cap, runner health) | we can see it working | the metrics page, with data from the fixture runs |

## F. What stays open, said plainly

- **A flaw in the hypervisor or the processor** that defeats hardware isolation is out of scope for v1 and stays a residual risk of every virtual-machine boundary. The mitigations are one guest at a time on the host and a patched host.
- **A home PC is a worse host than a rented server**: it holds your data, is rebooted, and sits on a home address. The hosted-service plan still recommends a rented server first; this gate is what makes the home PC acceptable as the first host if you choose it. Whether the home PC's hypervisor can run the chosen stack (for example nested virtualisation under WSL2) is **not verified**.
- **The compile-time cost under emulation** and the **hardware-virtualisation overhead** are unmeasured (isolation plan §9.2, §9.3); the 30-minute default held with 39 times headroom for this repository only, and the 43-member `fuel` workspace was not run.
- **A runner that is not yours** can send a forged result; the gate for that is the spot-check rate in the isolation plan §7 (open: the rate).

## Approval

| | |
|---|---|
| Checklist approved by CireSnave | not yet |
| Fixtures run, evidence attached (host class, date) | not yet |
| **Go to run submitted repositories on this host (CireSnave)** | **not given** |
