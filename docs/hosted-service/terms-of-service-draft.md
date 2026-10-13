# CodeRipper hosted service: Terms of Service template and takedown policy — DRAFT v2

> **Status: a draft for CireSnave's approval. It is not legal advice and it has not been reviewed by a lawyer, and none is planned
> (CireSnave, 2026-10-13: no funds, none willing to do it free).** It was written by an AI agent (the CodeRipper lane) from CireSnave's
> rulings of 2026-10-12 and 2026-10-13 and the design in `docs/superpowers/plans/2026-10-12-hosted-service.md`. Nothing here is in
> force and no service exists yet. v2 replaces v1 after two outside reviews of v1 (an AI rewrite and an AI critique of both); what was
> adopted and what was not is in Part C.
>
> **What this document is.** CodeRipper is software that anyone can host. This is a **template**: a site that hosts a CodeRipper service
> (Thinker's Journal is one such site, not the only one) is the *operator*, renders the template with its own values, and is the party
> its users deal with. The CodeRipper authors are not a party to the operator's terms and receive no complaints about an operator's
> service. Each operator remains responsible for checking that the text suits its own situation and law.
>
> **Layout.** Part A (template values and operator duties) and Part C (review notes) are for people and are **not rendered**. Part B is
> the user-facing text; its `{VALUES}` are filled in when it is rendered. `[ASSUMPTION n]` marks a legal assumption nobody qualified has
> checked; they are listed in Part A.4 and are removed from the rendered text.

## Part A. For the operator (not rendered)

### A.1 Values the renderer fills in

CodeRipper's hosted-service package takes these from a configuration file or from its installer, and refuses to render with any of the
first five missing. Defaults are CodeRipper's; an operator may change them.

| Value | Meaning | Default |
|---|---|---|
| `{OPERATOR_NAME}` | who "we" is: the legal person or site that runs this service | **required** |
| `{CONTACT}` | where takedown, abuse, privacy and vulnerability reports go | **required** |
| `{GOVERNING_LAW}` | governing law and venue | **required** |
| `{MIN_AGE}` | minimum age and who may open an account | **required** |
| `{LIABILITY_CAP}` | the cap in Part B section 9 | none: render fails |
| `{MAX_RUNTIME_MINUTES}` | default limit on one job's run time | 30 |
| `{MAX_RUNTIME_CEILING_MINUTES}` | the most a user may set it to | operator's choice; propose 120 |
| `{RESULT_RETENTION_DAYS}` / `{RESULT_RETENTION_MAX_DAYS}` | default and ceiling for how long a result is kept | 30 / 365 |
| `{JOB_METADATA_RETENTION_DAYS}` | how long job metadata is kept after the result expires | 90 (never less than 30: the queue ordering reads 30 days) |
| `{BACKUP_EXPIRY_DAYS}` | how long a deleted result may survive in backups | operator's choice; if the operator keeps none, the sentence is dropped |
| `{TAKEDOWN_RESPONSE_HOURS}` | time to hide a reported result | operator's choice |
| `{VULN_ACK_DAYS}` | time to acknowledge a vulnerability report | operator's choice |
| `{COMMUNITY_NAME}` | the community the service can prepare posts for, if any | none (the section is dropped) |

CireSnave's own values come from `C:\Projects\CIRESNAVE-CONTACT.md` when the Thinker's Journal instance is configured; they are not
written into this file or into the repository.

### A.2 What an operator must do for this text to be true

1. **Collect the user's authorisation at intake** (Part B section 3) with the boilerplate in Part A.3 and keep the record. CodeRipper's
   intake interface carries the record with the job and refuses a job without one; it cannot check that the statement is true.
2. **Publish a privacy notice of its own.** What CodeRipper produces is listed in Part B section 8. What the operator's *site* logs
   (addresses, sessions, cookies) is the operator's own matter and is not described here, because CodeRipper's code does not handle
   requests: it takes a job and a repository and returns a result and metadata.
3. **Be the contact.** Takedown, copyright and abuse reports about a service go to whoever runs it. If you use a site other than ours,
   write to that site, not to the CodeRipper authors.
4. **Decide the copyright question** (A.4 item 5): whether to register a designated agent and run a counter-notice flow. This draft
   gives a notice-and-takedown process that is a start, not a statement that the operator has safe-harbour status anywhere.
5. **Set the runner pool honestly.** Part B section 4 tells users that a job may run on a machine run by someone other than the
   operator. If the operator admits only its own machines, delete that sentence; if it admits others, the result shows which runner ran
   the job.

### A.3 Boilerplate: the authorisation the site asks for at intake

The operator's site shows this to the user before it hands a repository to CodeRipper. `{repository}` and `{username}` are filled in by
the site, which is the only party that knows them.

> **Before we scan `{repository}`:** `{username}`, please confirm that you have the right to ask for this. By continuing you state that
> (a) you are entitled to submit `{repository}` for scanning, as its owner, a maintainer with authority to do so, or with the owner's
> permission; (b) you authorise `{OPERATOR_NAME}` and the runners it uses to fetch, compile, execute and analyse that repository at the
> commit you chose, only to produce the report you asked for; (c) you understand that compiling a Rust repository runs its build
> scripts and procedural macros on a machine that is not yours; and (d) submitting it breaks no law, no agreement binding you and no
> right of anyone else. If you are not sure, do not continue.

The site records the user, the repository, the commit, the time and the version of this text.

### A.4 Legal assumptions nobody qualified has checked

| # | Assumption | What changes if it is wrong |
|---|---|---|
| 1 | A report's findings (short quoted file paths and lines, rule ids, identifiers) are not a copy of the repository's source and need no licence. A finding can still quote an identifier, a secret or personal data. | Results would be limited to counts and locations, and fields would be allow-listed. |
| 2 | A user's statement at intake (A.3), plus the push check the site makes through its forge, is enough to authorise a scan of a public repository. A fork or an organisation may have rules of its own. | The site would need an owner check or an organisation opt-in. |
| 3 | Running a repository's build scripts in an isolated runner, at the request of someone who has stated that they may ask, is not unauthorised access. | The service could not run build-time code (Face A only). |
| 4 | Calling the service a processor of user-requested jobs does not by itself settle who is liable for what a repository's code does in a sandbox. **It is not relied on**: the sandbox, the limits and the abuse policy are what the operator relies on. | Liability and insurance would have to be answered first. |
| 5 | A notice-and-takedown process like section 10 is adequate for a service that stores findings and not the repository's content. Whether the US DMCA, the EU DSA or another regime applies to a given operator is unknown. | A designated agent and a counter-notice flow would be needed. |
| 6 | The warranty disclaimer and liability limit in section 9 are enforceable where users live. | Local consumer law may override them. |
| 7 | The retention in section 8 is compatible with privacy law where users live. | A fuller privacy notice, a lawful-basis statement and a data-subject-request process would be needed. |
| 8 | Running untrusted jobs on a runner host does not breach its provider's terms (a VPS provider's or a home internet provider's). | Runner locations would have to change. |
| 9 | A user's intake statement is a meaningful record that the user, not the operator, vouched for the right to scan. | The operator carries the whole risk of an unauthorised scan. |

## Part B. The terms, as rendered

### 1. What this service is

CodeRipper reads a software repository and reports what automatic linters and compilers do not: unused public functions, ignored return
values, unused parameters, inconsistent versions, missing CI protection, and a set of file-level rules. `{OPERATOR_NAME}` runs it for
you in one of two ways:

- **From your own CI (Face A).** Your workflow runs CodeRipper on your CI runner and uploads the result. We never run your code.
- **By repository (Face B).** You choose a repository on our site; we fetch it and run CodeRipper in an isolated runner. This runs the
  repository's build scripts and procedural macros, because checking Rust code means compiling it.

**What it is not.** It is not a security audit, a licence review, a statement about quality or legal compliance, or a guarantee.
Findings can be wrong, and rules can be missing, skipped or inconclusive. A report with nothing in it means "every rule that could run
did run and found nothing", and the report says which rules could not run and why. You decide what to do with a report; we are not
responsible for a decision to merge, ship or rely on one.

### 2. Your account

- You need an account and must be logged in. You must meet these requirements to open one: `{MIN_AGE}`.
- You are responsible for what your account submits.
- You may not use more than one account, an identity other than your own, or automated submissions to get around the queue ordering
  in section 7, a suspension, or any other limit of the service. Accounts for an organisation, and automation that follows these
  terms, are fine.

### 3. What you may submit (Face B)

You may submit a repository only if **all** of these are true:

1. it is **public**;
2. you can **push** to it (we check through the repository's host when you submit);
3. you have made the statement we ask for when you submit it: that you may ask for the scan, and that you authorise the processing in
   section 4 (this is your statement, not ours, and you are responsible for it being true);
4. you are not submitting it to harm someone, to probe our infrastructure, or to use our machines for anything but the scan.

A fork or an organisation's repository may carry rules of its own: if you are not sure you may submit it, do not. The owner of a
repository may ask us not to scan it again (section 10). Private repositories are not accepted in this version; use Face A, which runs on
your own CI.

**Licence you give us.** You keep your rights in what you submit. You give `{OPERATOR_NAME}`, and the runners it uses, a limited
permission to fetch, copy, compile, execute and analyse the submitted repository at the commit you chose, only as needed to produce your
report, to run and protect the service, and to deal with abuse. We do not use a repository for anything else, such as training a model or
advertising.

### 4. What we do with a submission

- We fetch the repository at one commit and run CodeRipper in a fresh, isolated runner. The runner is destroyed after the job; a job
  that does not end is killed (see "Limits" below) and the runner destroyed then.
- **Your job may run on a machine run by someone other than `{OPERATOR_NAME}`**, a runner provider we have admitted to our pool. That
  machine sees the repository (public) and runs its build scripts. The result says which runner ran the job.
- **Dependencies are fetched from the public crate registry before the build; the build itself runs with no network access.** A build
  script that needs the network fails, and the report says so: it is a gap, not a pass.
- **Limits.** Each job has a maximum run time of `{MAX_RUNTIME_MINUTES}` minutes by default (you can set it higher on your account, up to
  `{MAX_RUNTIME_CEILING_MINUTES}`), and caps on memory, disk and processes. A job that reaches a cap is stopped, and the report says
  which cap and that the check did not complete. These limits protect the machine the job runs on; they are not a limit on how many
  jobs you may run.
- We do not keep the repository's source after the job. We keep the report and the job metadata (section 8).
- Output from build scripts and tools is **not shown verbatim**: known secret shapes are removed and a failed build script's words are
  replaced by a fixed sentence. This is a safeguard, not a promise that no secret can ever appear: do not put secrets in build output.

### 5. Acceptable use

Do not:

- submit a repository you may not ask us to scan, or that you do not have the rights to submit;
- try to escape, probe or overload the sandbox, a runner or the scheduler;
- reach other users' reports, accounts, credentials or job data, or try to take data from a host or another job;
- use the runners to mine, to attack, to host or to send traffic;
- submit repositories whose only purpose is to attack the runner;
- scan at a rate meant to crowd out other users, or get around a limit, a rate limit, an identity check or an isolation boundary;
- submit personal data or confidential material you have no right to submit;
- upload through Face A a result you know is fabricated, or source files, secrets or personal data (a Face A upload is a result, not a
  repository);
- use the service for anything unlawful, or to infringe another person's rights.

**Security research.** We want to hear about weaknesses; see section 10. Testing the sandbox's isolation from inside a job is welcome
only by arrangement: write to `{CONTACT}` first and we will tell you what is in scope. If you find something by accident, stop, do not
use it, do not look at other users' data and tell us. Without an arrangement, an attempt to escape the sandbox is a breach of these
terms. `[ASSUMPTION 3]`

### 6. Results and what you may do with them

You may use, share and publish your report. Before you publish it, read it: it may name files, identifiers or other material of the
repository, and you are responsible for what you publish. A report's being visible does not give anyone a right to anything it quotes.
If a report exposes a secret or personal data, tell us at `{CONTACT}` (section 10).

### 7. Fair use and the queue

We do not set a quota on how much you use in total. When the runners are busy, jobs are ordered **first by the number of jobs the
account has started in the last 30 days (fewest first), second by the resource its jobs used in that time (least first)**, then by when
the job was submitted. Jobs that you cancelled before they started, or that we re-ran because a runner vanished, are not counted
against you twice. Each account may have **at most two jobs running at once**: a safety limit for the machines, not a cap on total use.

The ordering is a best effort, not a guarantee of when a job runs. When the queue is full we refuse a new job and ask you to try later.
**A job we have accepted** (it is in the queue) is retried on another runner if its runner fails, a bounded number of times; if we
cannot finish it, it is marked **failed** and you are told, never silently dropped. We may apply temporary safeguards against abuse,
capacity problems or an emergency, including delaying or refusing submissions. We may change the numbers; the ordering rule will always
be published.

### 8. Results, retention and what we store

Three different things are promised separately.

- **Who can see a result.** You choose, per result. Nothing is public unless you make it public. A link to a result is not meant to be
  guessable, and results are not for search engines.
- **How long it is kept.** `{RESULT_RETENTION_DAYS}` days by default; you can set a different default on your account, up to
  `{RESULT_RETENTION_MAX_DAYS}` days. After that the result is deleted from our active systems and its link says "expired".
- **What deletion means.** A result you delete, or that expires, is removed from our active systems promptly; copies in backups are
  gone within `{BACKUP_EXPIRY_DAYS}` days. Making a result private does not take back a copy someone already has, and a post you
  published elsewhere is not removed when the result expires (you may delete it yourself).

**What we store about a job.** Your account; the repository and commit; the report; and **job metadata that CodeRipper produces with the
report**: when it ran, which rules ran, which runner, run time, CPU, memory and disk use, and the outcome. Metadata is kept
`{JOB_METADATA_RETENTION_DAYS}` days after the result expires so that the queue ordering and abuse handling can work. We do not keep the
repository's source after the job. What our site records about your visits (addresses, sessions) is described in our privacy notice,
not here. `[ASSUMPTION 7]`

**Community posts.** If `{COMMUNITY_NAME}` offers it, the service prepares a summary for you to post; you publish it, and it stays
under your control.

### 9. No warranty, and limits on our liability

The service and its reports are provided **as they are**. We do not promise that a report is complete or correct, that a repository is
safe, that a job will complete, or that the service will be available. We may change or end features, rules, supported languages and
infrastructure.

To the extent the law allows, we exclude liability for indirect or consequential loss, for loss of data, profit or business, and for loss
arising from your reliance on a report. **Our total liability to you for everything arising from the service is limited to
`{LIABILITY_CAP}`.** Nothing in these terms excludes or limits liability that the law does not allow to be excluded or limited, and
nothing takes away rights you have by law that cannot be waived. `[ASSUMPTION 4]` `[ASSUMPTION 6]`

### 10. Takedown, abuse and vulnerability reports

Write to **`{CONTACT}`** and say which of these it is. These are separate because they need different things from us.

| You are reporting | Send us | We do |
|---|---|---|
| **A copyright complaint**: a result contains material you own that you did not allow | the result link; the work and where it is; your name and contact; a statement that you believe the use is unauthorised and that your notice is accurate | hide the result within `{TAKEDOWN_RESPONSE_HOURS}` hours; tell the submitting account and give it the notice; restore it if the account shows a right to use it |
| **A request to remove a result about your repository, or naming you** (not a copyright claim) | the result link and how you are connected to the repository | restrict it while we look; hide it within `{TAKEDOWN_RESPONSE_HOURS}` hours if you are connected to the repository and it should not be public; tell the account. We keep the right not to remove a finding that is true and relevant. If you own a repository, you may ask us not to scan it again |
| **Personal data or a confidential matter in a report** | the result link and what it exposes | restrict it at once, then delete it from active systems |
| **A secret in a report** | the result link | restrict it at once, then delete it; **rotate the secret yourself: a secret shown in a report is exposed** |
| **Someone is using our runners to attack or mine** | the time and the address or account you saw | suspend the account, kill its jobs, keep the job metadata, tell you what we can |
| **A weakness in the sandbox, a runner or the service** | what you found and how to reproduce it | acknowledge within `{VULN_ACK_DAYS}` days, fix it as soon as we can, and credit you if you want. An acknowledgement is not a fix date |

A result reported in these ways may be hidden or restricted without warning. Where an account is repeatedly the subject of valid
complaints we warn, suspend or end the account. A person whose result was hidden may ask why at `{CONTACT}`. We keep a record of each
request and what we did. If you use a service run by someone else, write to them, not to the CodeRipper authors. `[ASSUMPTION 5]`

### 11. Suspension and ending

We may stop a job, remove a result, or suspend or end an account if you break these terms, if the law or a legal order requires it, if
you threaten the safety of our machines or other users, or if continuing is unsafe. In an urgent case we may do this first and tell you
afterwards; queued jobs of a suspended account are cancelled. You may ask us at `{CONTACT}` to review a decision. You may delete your
account at any time; its results are deleted as in section 8, and posts you published elsewhere remain under that service's rules.

### 12. Changes, law and contact

We may change these terms. A change that affects you is announced on the service before it applies, applies to jobs submitted after
it, and if you do not accept it you may stop using the service and delete your account. These terms are governed by
`{GOVERNING_LAW}`. If a part is found unenforceable, the rest stands. Contact: `{CONTACT}`.

## Part C. What the two reviews of v1 changed (not rendered)

v1 was rewritten by an AI model (Qwen) and the result reviewed by another (ChatGPT) on 2026-10-13. Each point was checked against v1
and the design before use.

**Adopted:** named values for placeholders; bullets in the acceptable-use list; a user representation and a fork/organisation clause
instead of treating push access as proof of authority; a processing licence (neither v1 nor the rewrite had one); "accepted job" defined,
with failure shown not hidden; visibility, retention and deletion as three promises, with backups; a wider suspension clause and a review
route; copyright, privacy, secret, abuse and vulnerability complaints as separate rows; a coordinated-disclosure paragraph; the
one-account rule aimed at the behaviour; assumptions moved out of the rendered text; false positives and negatives stated.

**Not adopted from the rewrite:** "at the request of a user with push access" in assumption 3 (changed the claim from the owner's
request); "no *strict* quota" (hedged a ruling); "we disclaim all liability" followed by a cap (contradicts itself); "request the
Community team to hide the post" (v1 said a post *of ours*); dropping "you know is" before "fabricated". It listed IP addresses as
stored in a note and not in the text; no CodeRipper code handles an address, so that is left to the operator's privacy notice.

**Not adopted from the critique:** replacing "every rule that could run did run" with a vaguer sentence (it is CodeRipper's coverage
contract: a rule that cannot run is listed); putting sandbox requirements in the terms (they are in the design); treating "no network"
as unclear (v2 states the resolve step, which the design already has).

**For the design, not the terms (not done here):** the design's section 10.5 says hardware virtualisation only, never a shared-kernel
container, on a machine that also holds personal files. CireSnave's 2026-10-13 ruling is containers for v1, on a VPS or a home PC, to be
decided. These terms say "isolated runner" and name no technology. The design needs a reconciliation before Face B opens: a hardened
container runtime (a user-space kernel such as gVisor, or rootless with a seccomp profile, no capabilities and resource limits) on a
host with no route to a home LAN is closer to the design's intent than a plain container, and a home PC is the worse host. Also for the
design: the self-registering runner image (runners admitted to a pool, each result naming its runner), the intake interface that
carries the authorisation record, the result-and-metadata interface the site serves, and the installer/config that renders this
template.
