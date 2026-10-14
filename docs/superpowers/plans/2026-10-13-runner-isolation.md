# Runner isolation for the hosted service: requirements, threat model, stack comparison (design)

> **Status: design only; nothing is built. Two of the three spike arms have been measured (section 9); the emulation arm has not.** Written 2026-10-13 after CireSnave's rulings of the same
> day, relayed by the PM. It **replaces the container-first v1 stance** of the earlier notes (ToS Part C; `2026-10-12-hosted-service.md`
> section 10.5 now points here). Where a fact was read from a primary page it says so; where it comes from a search summary it says
> that; where it is not known it is listed in section 10.

## 1. What was decided

CireSnave, 2026-10-13 (verbatim, as relayed by the PM):

> *"I'm thinking full emulation with no access to the local machine the runner is on except what is absolutely necessary. Perhaps we
> even limit what domains a build can interact with by default with the ability for the site's owner or the runners' owners to
> approve other sites for specific repositories."*

> *"A microVM is fine with me."*

Read together: the guest is **a complete virtual machine with its own kernel and no reach into the host beyond what the job strictly
needs**, and a **hardware-virtualised microVM is accepted as that**. Literal CPU emulation (no hardware virtualisation) stays in as a
**measured fallback**, because it is the strongest boundary and the slowest. Plain containers are not admitted.

The measurement spike (section 9) was approved by CireSnave, relayed by the PM, and CireSnave confirmed in session that the PM's word
is his. Two arms were run on 2026-10-13; the **emulation arm is on hold** (section 9).

## 2. Requirements

Numbered so the launch gate (section 8) and the fixtures can cite them.

| # | Requirement |
|---|---|
| R1 | **One guest per job, destroyed after, never reused**, with its own kernel, started from a read-only base image; its disk is ephemeral. |
| R2 | **Hardware-virtualised isolation** (a microVM or full VM) is required. Emulation without hardware virtualisation is allowed only where hardware virtualisation is absent **and** the section 9 numbers show the run-time limit holds. **Plain containers, and any isolation that shares the host kernel, are not admitted.** |
| R3 | **The guest sees nothing of the host** except: the job's input (the repository at one commit, delivered read-only) and a result channel back. No shared folders, no host mounts, no clipboard, no USB or other pass-through devices, no GPU, no host identity, no metadata service, no console to the host's desktop. |
| R4 | **No credentials of any kind in the guest** (no tokens, keys, agents, cloud identity). |
| R5 | **Resource caps are enforced from outside the guest:** CPU time, memory, disk, process count and wall-clock time. The orchestrator's timer kills the guest; nothing inside has to cooperate. Run-time default 30 minutes (configurable, ceiling combined by minimum: `2026-10-13-configuration-layers.md`). |
| R6 | **Network is default-deny and enforced outside the guest** (section 5). The guest has one route: to a proxy on the host side. |
| R7 | **The host-side runner agent is the attack surface and is built as one** (section 6): minimal, unprivileged, no inbound listener, signed image. |
| R8 | **A result is untrusted data until validated**: the agent accepts only defined fields from the guest, bounded in size, and treats everything else as hostile (section 7). |
| R9 | **Every approval to leave the default is recorded and shown in the report** (section 5.3). |
| R10 | **A runner enrols with an attestation** of the controls it claims; the design says plainly that the site cannot verify it (section 6). |

## 3. Threat model

**Assets, most to least important:** the runner host's owner's machine, files, credentials and network (for a home PC this is a
person's computer and LAN); other jobs and other users' results; the service's credentials and queue; the integrity of a result; the
operator's reputation and legal position; compute capacity.

**Adversary:** the author of a repository the user submits (or anyone who tricks a user into submitting one). They control **every
byte of source, every build script and every procedural macro**, which are native programs run during the build, and every
dependency version the lockfile allows. They want to: escape the guest to the host; reach the host's LAN or its services; steal
credentials or other users' data; exfiltrate; use the machine (mining, proxying, DoS); forge or poison a result; exhaust capacity.

**Surfaces, in the order a job meets them:**

1. **Fetch and unpack** (the repository, then dependency archives): archive parsing, symlinks, huge or zip-bomb inputs, path
   traversal. Done by the agent on the **host side**: this is the first attack on the host, so it is bounded and done in its own
   unprivileged process or in a separate throwaway guest.
2. **Build and macro execution:** arbitrary code in the guest. The boundary is the VMM and the hypervisor (KVM or Hyper-V).
3. **Guest to host channels:** the virtio or paravirtual devices the VMM exposes, the result channel, and the network proxy.
4. **Network:** the proxy, DNS, and whatever the allowlist permits.
5. **Result extraction:** parsing what the guest returns.
6. **The agent's own control plane:** enrolment, job pickup, upload, update.

**Out of scope for v1 and said so:** a hypervisor or CPU vulnerability that defeats hardware isolation (the residual risk of every
VM boundary: Firecracker's own analysis credits KVM to the Linux community, not to itself; side-channel attacks between co-resident
guests are mitigated by one guest per host at a time and by host patching); a malicious **runner owner** attacking the service or
its users (section 7); a user with a legitimate account abusing within the limits.

## 4. The stack comparison

Evidence column: **[read]** = a primary page read on 2026-10-13; **[search]** = a search summary, not re-read; **[known]** = general
knowledge, not checked here (to confirm in the spike).

| Stack | Boundary | Host | What the evidence says | Verdict |
|---|---|---|---|---|
| **Plain container** (Docker/Podman) | shared host kernel; namespaces, cgroups, seccomp | any Linux | A kernel bug or misconfiguration is a host escape. **[known]** | **Not admitted** (R2). |
| **Rootless container + gVisor** | user-space kernel in front of the host kernel | Linux | Stronger than a plain container, still a process on the host kernel with a large syscall-emulation surface. **[known]** | **Not admitted** as the isolation stack. May be a *defence-in-depth layer inside* a guest. |
| **Firecracker** (microVM) | KVM + a minimal VMM; seccomp on the VMM process, cgroups/namespaces, a jailer that drops privileges; minimal virtio device set | Linux with `/dev/kvm` | Design doc: KVM boundary, then seccomp, cgroups/namespaces and a jailed process; it recommends the jailer. Needs a Linux host with KVM. **[search]** | **Admitted (preferred) on Linux VPS and Linux home PCs.** |
| **Cloud Hypervisor** | Rust VMM on KVM or the Microsoft hypervisor (MSHV); minimal emulation | Linux (KVM); Windows via MSHV is the project's stated target **[search]** | Same class as Firecracker; fuller device model, longer-lived VMs. | **Admitted (alternative).** |
| **QEMU/KVM** (e.g. `microvm` machine type) | full VMM on KVM | Linux | Larger VMM than Firecracker; mature; widely audited. Not researched further here. | **Admitted** where Firecracker is not usable; harden (no default devices). |
| **Hyper-V VM** (Windows Pro/Enterprise) | the Microsoft hypervisor; a separate kernel | Windows 11 Pro+ **[read: Pro/Enterprise/Education have the hypervisor-based sandbox; Home is not supported for Windows Sandbox]** | A full VM with a virtual switch the host controls; per-VM resource limits. How a default-deny network policy is enforced on the host (virtual-switch rules vs a host-side proxy) is **not verified here**. **[known]** | **Candidate for Windows home PCs; to be proved by the fixtures.** |
| **Windows Sandbox** | hypervisor-based; separate kernel; disposable | Windows 11 Pro, Enterprise, Education only; **not Home** **[read]** | "Uses hardware-based virtualization for kernel isolation." **Networking is on by default** and is an on/off switch, so an allowlist needs another mechanism; **multiple instances cannot run at once**; mapped folders and clipboard redirection exist and must be off; the guest is Windows. **[read]** | **Not admitted:** one job at a time, an on/off network switch only, and a Windows guest for a Rust build whose target is usually Linux. |
| **WSL2** | a lightweight utility VM on the Microsoft hypervisor | Windows 10/11 | **All WSL2 distributions run in one VM** (the `.wslconfig` settings configure "the virtual machine powering WSL 2"); **by default** fixed Windows drives are **mounted under `/mnt`** (`automount.enabled=true`), Windows processes can be launched from Linux (`interop.enabled=true`) and the Windows `PATH` is appended, GPU paravirtualisation is on, and the network is NAT. Per-distro cgroup isolation is a **cgroup** feature (`isolateDistroCgroup`). **[read]** | **Not admitted as an isolation stack:** its defaults give a guest the host's drives, and one VM is shared across distros. All of those can be switched off in `wsl.conf`, but the files that control it live in the user's profile and WSL's design goal is integration, not containment. **WSL2 may be a *host* for a nested microVM** (nested virtualisation defaults on, Windows 11 **[read]**): a candidate to **test**, not to assume. |
| **Full emulation** (QEMU TCG, no hardware virtualisation) | a software CPU; nothing the guest runs executes on the host CPU | any | Sources found put TCG at **about 8x to 12x slower than hardware acceleration for general workloads** (a Linaro measurement of an Android boot, and a QEMU developer's 2018 estimate for integer code, "about 12x on average, improving") **[search]**. **No compile-workload measurement was found.** The PM's "10 to 50x" is therefore **not supported by what I found, nor refuted**: the number for a Rust build is unknown until section 9. | **Fallback only, and only if measured to fit the limit** (R2). |

**What WSL2 shares with the host, plainly:** with its defaults, the host's fixed drives (as `/mnt/c` and so on, through DrvFs, a
filesystem plugin built for interop), the ability to start Windows programs, the host's GPU, the host's timezone, and a NAT'd
network; the **VM and kernel are shared by every distribution**, and the settings that govern all of that are read from files in the
signed-in user's profile. Turning each off closes those routes but does not change that it is one shared VM; a job that runs there
shares fate with the user's other WSL work. That is why it is not admitted.

**Where the choice stands:** **Linux with KVM + Firecracker (or Cloud Hypervisor)** is the v1 stack. **Windows home PCs** are in v1
only through a hardware-virtualised VM that passes the fixtures (candidate: a Hyper-V VM; candidate: a KVM microVM inside WSL2 where
nested virtualisation works); until one passes, **Windows home PCs are not runner hosts** and the VPS is. That is a consequence of the
launch gate, not a change to CireSnave's decision to have home PCs.

## 5. Network: default-deny, outside the guest, with approvals

### 5.1 The route

The guest has **one** network path: to a proxy listening on the host side of a private link (a virtual switch or a point-to-point
interface) that has **no other route**. Everything else is dropped **on the host**, not in the guest.

- **DNS only through the proxy.** The guest has no resolver of its own and no direct UDP/53 or DoH route. A request names a host to
  the proxy; **the proxy resolves it**.
- **IP literals are refused.** A request to a bare address is not a hostname on the allowlist and is dropped.
- **Private, loopback, link-local and host-gateway answers are refused** after resolution (a name that resolves to RFC 1918,
  `127.0.0.0/8`, `169.254.0.0/16`, `::1`, fc00::/7, fe80::/10 or the host's own addresses is dropped), which closes DNS-rebinding
  and makes a home LAN unreachable even if a name is allowlisted. **A home PC's LAN is unreachable by construction, not by policy.**
- **The proxy does not terminate TLS.** It allows or drops a `CONNECT host:443` by the **requested host name**, and enforces
  per-job caps: bytes in and out, connections, and rate.

### 5.2 Phases

| Phase | Network | Why |
|---|---|---|
| **Fetch** (the repository, if the guest fetches it; usually delivered, not fetched) | none: delivered by the agent | nothing for the guest to reach |
| **Resolve dependencies** (`cargo fetch --locked`) | allowlist: **the crate registry's index and its download host** (for crates.io: `index.crates.io` and `static.crates.io`; to be confirmed against what `cargo` actually requests, in the spike). No git dependencies by default. | the only way the build gets code that is not the repository's |
| **Build and checks** | **none**, unless a domain is **approved for that repository** (5.3) | a build script or macro cannot fetch a second stage or exfiltrate |
| **Delegated tools that need a network** (`osv-scanner`, `lychee`) | their own allowlist, per the earlier design | already in the hosted-service design |

A build that needs a domain it is not allowed fails, and the report says **"could not run: build needed `host`, not approved"**,
which is a gap, never a pass.

### 5.3 Approvals

- **Scope:** an approval names **a domain (or a domain and path class) and one repository**, optionally pinned to a **commit or ref**.
  There are no global approvals in v1.
- **Who may approve:** (a) the **site's owner (the operator)**, for any runner; (b) a **runner's owner**, **only for jobs on that
  owner's own runners** (it changes what their machine will let a job reach, not what anyone else's does).
- **Recording and visibility:** each approval is stored with who, when, scope and reason, and **the report lists the approved
  domains the job was allowed**, so a reader of a result can see what the build could reach.
- **Revocation** takes effect for future jobs. Nothing is approved implicitly by a repository's own configuration: **a repository's
  file can only *request* a domain** (an allowlisted key; the request is shown to the approver), never grant it.

### 5.4 What a domain allowlist does not stop (and what covers each)

| Not stopped | Covered by |
|---|---|
| **Domain fronting / shared hosts:** an allowed CDN host also serves other tenants, so a request that names the allowed host can reach another site's content on the same edge | the proxy checks the **requested name** and cannot see the HTTP `Host`; mitigation is a **narrow allowlist** (only hosts that serve only the registry) and byte caps; residual risk accepted and stated |
| **Abuse within an allowed host** (hammering the registry, using it as a drop box for small amounts of data) | per-job byte, request and connection caps; no credentials in the guest means nothing can be *published*; the job's wall-clock limit |
| **DNS tunnelling** | no direct DNS from the guest; names go only to the proxy, which accepts only allowlisted names and logs the rest as a violation |
| **Covert channels through timing or sizes** to an allowed host | caps; not preventable at this layer; low value to an attacker with no credentials |
| **Mining** | the CPU-time and wall-clock caps (R5); there is no pool address on the allowlist |
| **A compromised registry or dependency** | the lockfile (`--locked`), the supply-chain rules CodeRipper itself reports on, and the guest boundary: a malicious dependency runs in the guest like any build script |

## 6. The host-side runner agent

Once the guest is a VM, **the agent that fetches, boots, feeds and collects is the weakest part** and is designed as the attack
surface:

- **Minimal and unprivileged:** it runs as a dedicated, non-admin user (or a dedicated VM host account), with only the rights to
  start guests and hold their disks; **no personal files, no SSH agent, no cloud tokens, no password-manager session** on that
  account.
- **No inbound listener.** It **polls** the service over TLS; nothing connects to a home PC from outside.
- **Signed image and signed updates** from CodeRipper's release process; it refuses an unsigned update.
- **Bounded parsing:** the repository archive and every returned result are read with size, depth and count caps, in a process
  that can be killed.
- **Self-registration with an attestation:** the agent states that hardware virtualisation is present, that the proxy and
  LAN-egress rules are active, and that it runs unprivileged. **The site cannot verify any of this remotely.** The honest position:
  an attestation is a *statement by the host's owner*, and **the risk of a false or stale one falls on that owner (their machine, their
  LAN)**, not on the operator, who relies on the owner's word and on the fixtures having been run against **that host class**. The
  agent re-runs a short **self-test** (a probe that must fail to reach the LAN, a probe that must fail to reach a non-allowlisted
  host) at start and after any update, and **refuses to take jobs** if one succeeds; that is evidence, not proof.

## 7. Result integrity

Repositories scanned are **public**, so the user-facing risk of a third-party runner is **a forged, altered or withheld result**, not
a leak. A runner's owner could also attack the service or its users through the result channel.

- **Every result names the runner** and its attestation level; the dashboard marks a result from **a runner the operator does not
  run** as such: *"Run on a runner not operated by `{OPERATOR_NAME}`; not independently checked."*
- **Signed per-job:** the agent signs the result and metadata with its enrolment key (so a result can be tied to a runner and
  revoked); a signature **does not** prove honest execution.
- **Spot-checks:** a fraction of jobs from non-operator runners (a configurable rate, plus every job for a newly enrolled runner)
  are **re-run on an operator-run runner** and the finding sets compared; a mismatch takes the runner out of the pool and **marks its
  earlier results suspect**. Cost is capacity, which is the scarce resource here, so the rate is a number the operator sets.
- **Allow-listed result fields** (R8), bounded in size; anything else is dropped, and the drop is counted.
- **A runner that fails twice** is already removed from the pool by the design's lease rule; a result a runner never returns is
  re-queued, so withholding costs the runner owner reputation, not the user a result.

## 8. The launch gate (replaces the earlier one)

**Face B does not open for a host until the fixtures below have run against that host's class and passed, for each isolation
stack the operator intends to use on it.** A "class" is an OS, a hypervisor and an isolation stack (for example: *Linux + KVM +
Firecracker*; *Windows 11 Pro + Hyper-V VM*); a change of any of the three is a new class. A host that cannot pass is not used, and
"isolated runner" is not claimed for it.

| Fixture (a hostile repository, a test harness or a probe) | Must show |
|---|---|
| **Guest-escape probes**: attempts to reach `/dev/kvm` or the VMM's control socket from inside, to write outside its disk, to read host memory through every device the VMM exposes, to use the clipboard/shared-folder/USB paths | none works; the device list in the guest equals the allowlist of R3 |
| **Host-gateway and LAN reach**: connect to the host's gateway, a LAN address, `127.0.0.1` on the host, link-local, a metadata address | every attempt fails; the proxy logs each as a violation |
| **DNS**: query a non-allowlisted name directly (UDP/53, DoH, a hard-coded resolver), resolve a name that points at a private address (rebinding), use an IP literal | all fail |
| **DNS tunnelling**: encode data in queries to the proxy | rejected as non-allowlisted; volume capped |
| **Allowed-host abuse**: exceed byte, request and connection caps against an allowed host; try a different virtual host on the same CDN edge | cap hit, job stopped, gap reported; the fronting residual is **recorded as such** |
| **Resource exhaustion**: a fork bomb, a memory balloon, a disk-fill, an infinite build, a build that spawns detached children | each stopped at the cap **from outside**, host unaffected, runner destroyed and verified clean |
| **Cleanup**: after success, failure, timeout and forced kill | no guest disk, process, mount or network namespace remains; the next job sees none of it |
| **Result channel abuse**: an oversized, malformed or crafted result; a result with unexpected fields | bounded, rejected or sanitised, counted |
| **Agent self-test**: tamper with the proxy rules or the virtualisation check | the agent refuses jobs |
| **Archive attacks** (traversal, symlink, bomb) against fetch and unpack | contained and bounded |

Each fixture is mutation-checked (the mutation that should make it fail is applied and confirmed to), and results are kept with the
host class and the date.

## 9. The measurement spike (run 2026-10-13: two arms; the emulation arm is on hold)

**Purpose:** the choice between hardware-virtualised microVM and emulation must rest on a number: does a representative Rust build
fit inside the 30-minute default under each?

**Subject:** CodeRipper at `a1e8732` (0.6.11): `cargo build --release --locked --offline` from an empty `target`, dependencies
already fetched (84 crates compiled, the last being `coderipper`, including build scripts and procedural macros of its dependencies).
`CARGO_INCREMENTAL=0`, no `RUSTC_WRAPPER`.

### 9.1 Results

| Arm | Where it ran | Runs (wall-clock) | Median | Peak memory | `target` size |
|---|---|---|---|---|---|
| **A. Native Windows** (MSVC toolchain, `rustc 1.98.1`) | this box: Windows 11 Pro, AMD Ryzen 9 7940HX, 16 cores / 32 threads, 63 GB, hypervisor present | 155.6 s, 165.2 s, 138.1 s | **155.6 s** (138.1 to 165.2) | 874, 904, 891 MB (sampled sum of the build's process-tree working sets, 1 s) | 0.31 GB |
| **B. Hardware-virtualised Linux guest** (`rustc 1.99.0`, Linux 6.18.33.1-microsoft-standard-WSL2, 32 vCPU, 32 GB) | a throwaway container in Docker Desktop's WSL2 utility VM on the Microsoft hypervisor, same box | 46.7 s, 46.1 s, 44.3 s | **46.1 s** (44.3 to 46.7) | 1537 to 1613 MB (peak cgroup `memory.current`, which **includes page cache**: an upper bound, not a resident-set figure) | 0.23 GB |
| **C. Software emulation** (QEMU TCG, no hardware acceleration) | **not run** | - | - | - | - |

`cargo fetch --locked` took 5.7 s in the guest (the registry index was already cached, so it is **not** a representative fetch time).

**Does the 30-minute default hold?** Yes, with wide margin, for this repository: 46 s in the hardware-virtualised guest (about 39
times under the limit) and 2 min 36 s native on Windows. **That is one mid-size build; larger workspaces (the 43-member `fuel`
clone, about 4 minutes cold per member by an earlier measurement) are the case that matters, and none was run here.**

### 9.2 What the numbers do not say

- **They are not a like-for-like virtualisation-overhead measurement.** There is no bare-metal Linux on this box, so there is no
  native-Linux arm to set arm B against. Arm A (Windows MSVC, Windows filesystem, antivirus in the path) is a different operating
  system and toolchain; **its being 3.4 times slower than the Linux guest says nothing about virtualisation cost**. The overhead of
  hardware virtualisation (typically a small percentage for CPU-bound work, **not measured here**) is unmeasured.
- Arm B is a **container inside a shared VM**, a proxy for the speed of a hardware-virtualised guest, **not** a microVM and not an
  admitted isolation stack. Docker Desktop's VM also ran other lanes' containers, and other lanes' work shared the host CPU
  (host CPU before each native run: 19.5%, 16.3%, 10%; the guest runs were not sampled). Variance from that is in the ranges above.
- **Toolchains differ** (1.98.1 vs 1.99.0) and so do the `target` sizes.
- **Three runs per arm, not five.** The first native run follows a `docker run rustc -V` I issued by mistake while it was building;
  it is the middle value and the range is reported.
- Peak memory was measured two different ways and **must not be compared across arms.**

### 9.3 The emulation arm: attempted, blocked

The PM allowed a pinned image pull. **`rust:latest` for `linux/arm64`** was pulled (index digest
`sha256:81015672d6939b0265f09adf96d5de125482bb1330106076956b0518e74edc90`; about 2.2 GB added to the local image store, which now
holds both platforms under that tag at 4.58 GB). **It cannot run:** `docker run --platform linux/arm64 ... uname -m` fails with
`exec format error`, because no user-mode emulator (a QEMU `binfmt_misc` handler) is registered in Docker Desktop's VM, and no QEMU
is installed on the host. Registering one is a change to the shared Docker VM (the usual route is a privileged container that
installs the handler), which is outside the spike's rules. **Ruling (the PM, relaying CireSnave, 2026-10-13): skip arm C; emulation
stays unmeasured**, and shared-state changes such as registering a handler in the shared Docker VM need CireSnave's word, not a
spike's. **No emulation number exists.** The sources in section 4 (8x to 12x for general workloads, none for compilation) are all there is until it runs.
The 39x headroom above means that, **for this repository only**, emulation would fit the limit even at a slowdown beyond those
figures.

**Cleanup:** `docker image rm --platform linux/arm64 rust:latest` was refused ("content will be removed from all images referencing
this variant", `--force` needed), so the arm64 variant was **left in place**: 2.21 GB on disk (557 MB of content) under
`rust:latest`. `docker run rust:latest` runs `x86_64`, `rustc 1.99.0`. **A later cleanup, by someone who knows what else references
that content, can remove it.**

**A side effect to record:** the first `docker pull --platform linux/arm64 rust:latest` briefly made the local tag `rust:latest`
resolve to arm64 on this shared Docker, so a plain `docker run rust:latest` failed with `exec format error` until the amd64
platform was pulled again (digest unchanged, "up to date"). It runs amd64 again (`rustc 1.99.0`). The cause was pulling a second
platform under an existing tag; a later spike should pull by digest into a separate repository name.

## 10. Not known, and decisions that are CireSnave's

- **The compile-time slowdown under TCG** is unmeasured (section 9.3).
- **Hardware-virtualisation overhead against bare-metal Linux** is unmeasured: this box has none (section 9.2); the VPS would.
- **How a default-deny network policy is enforced on a Windows host** with a Hyper-V VM (switch rules or a host-side proxy) is not
  verified; the fixtures decide.
- **Whether nested KVM in WSL2** works on a given home PC and is acceptable is untested.
- **Whether a Firecracker-class VMM can run on the VPS** depends on the VPS exposing KVM (nested virtualisation on many VPS
  providers is off); not checked. If it does not, the VPS needs a provider that does, or it cannot be a Face B host.
- **For CireSnave:** (1) whether arm C (emulation) may use the network or an install; (2) the spot-check rate for non-operator runners; (3) whether a Windows home PC
  is a v1 runner host only after a class passes the fixtures (this document's position) or not at all in v1.
