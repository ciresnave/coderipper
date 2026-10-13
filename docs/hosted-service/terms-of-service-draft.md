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
> **Layout.** Part A (template values and operator duties), Part C (review notes) and Part D (the Arizona and federal check) are for people and are **not rendered**. Part B is
> the user-facing text; its `{VALUES}` are filled in when it is rendered. `[ASSUMPTION n]` marks a legal assumption nobody qualified has
> checked; they are listed in Part A.4 and are removed from the rendered text.

## Part A. For the operator (not rendered)

### A.1 Values the renderer fills in

CodeRipper's hosted-service package takes these from a configuration file or from its installer, and refuses to render with any value marked **required**
or "none: render fails" missing. Defaults are CodeRipper's; an operator may change them.

| Value | Meaning | Default |
|---|---|---|
| `{OPERATOR_NAME}` | who "we" is: the legal person or site that runs this service | **required** |
| `{CONTACT}` | where takedown, abuse, privacy and vulnerability reports go | **required** |
| `{GOVERNING_LAW}` | the law that governs the terms (for example, "the State of Arizona") | **required** |
| `{VENUE}` | where a dispute is heard (for example, "the state or federal courts located in Maricopa County, Arizona") | **required** |
| `{PRIVACY_POLICY_URL}` | the operator's own privacy notice, linked from sections 2 and 8 | **required** |
| `{MIN_AGE}` | minimum age and who may open an account | **required** |
| `{LIABILITY_CAP}` | the cap in Part B section 9. Must not be so low that it removes any reason to take care (Part D): a floor of the greater of what the user paid in the last 12 months and a stated sum is a judgement, not a rule of law | none: render fails |
| `{MAX_RUNTIME_MINUTES}` | default limit on one job's run time | 30 |
| `{QUEUE_STEP_MINUTES}` | how often waiting jobs are lowered in weight (section 7). Each step lowers every waiting weight by `QUEUE_STEP_MINUTES x 60`, so a weight falls one second per second waited; a weight never goes below 0 | 10 |
| `{DMCA_AGENT}` | the designated agent registered with the Copyright Office (name, address, email). **Optional**: if set, section 10's copyright notice and reply procedure renders; if unset it does not, and the operator has no statutory safe harbour (A.2 item 5) | none |
| `{MAX_RUNTIME_CEILING_MINUTES}` | the most a user may set it to. Optional: if unset, section 4's parenthesis drops "up to ..." and says only that the user may change it where the service allows | operator's choice |
| `{RESULT_RETENTION_DAYS}` / `{RESULT_RETENTION_MAX_DAYS}` | how long the **operator's site** keeps a result, and the most a user may set. CodeRipper hands the result over and enforces none of this | operator's choice (no CodeRipper default) |
| `{JOB_METADATA_RETENTION_DAYS}` | how long the operator's site keeps job metadata after the result expires | operator's choice (a site that orders its queue by the last 30 days needs at least that) |
| `{BACKUP_EXPIRY_DAYS}` | how long a deleted result may survive in backups. **Set it only if the backups can keep the promise**: immutable or provider-default backups may not be deletable on a day count, and the sentence should then say "at the next backup rotation" | operator's choice; if the operator keeps none, the sentence is dropped |
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
3. **Check the promises against the infrastructure** before rendering: that backups can be deleted within
   `{BACKUP_EXPIRY_DAYS}` (immutable backups often cannot), that the queue adjustment is published, and that a privacy notice
   exists at `{PRIVACY_POLICY_URL}`. A term the operator cannot keep is a breach of contract by the operator.
4. **Be the contact.** Takedown, copyright and abuse reports about a service go to whoever runs it. If you use a site other than ours,
   write to that site, not to the CodeRipper authors.
5. **Own the copyright question** (A.4 item 5). The site stores and serves the report, so a copyright complaint is the operator's, not
   CodeRipper's (CireSnave, 2026-10-13). **An operator that wants the statutory safe harbour for
   material stored at a user's direction must register a designated agent, publish it, run the notice and reply procedure of
   section 10 and reasonably implement a repeat-infringer policy; one that does none of that has no safe harbour, and section 10's
   table is then only a courtesy.** Nothing here says an operator has safe-harbour status anywhere.
6. **Set the runner pool honestly.** Part B section 4 tells users that a job may run on a machine run by someone other than the
   operator. If the operator admits only its own machines, delete that sentence; if it admits others, the result shows which runner ran
   the job. **An operator that admits other people's runners decides on what terms** (enrolment terms, attestation of the controls in
   the design's section 10.5, a way to remove a runner) and must not tell users more than it has done: the rendered text promises no
   contract with a runner provider.
8. **Check consumer-protection law where your users live** before rendering section 9, and consider limiting the service to places
   you have checked (in `{MIN_AGE}`'s eligibility text). This draft was checked against Arizona and federal law only (Part D).
7. **Publish the vulnerability-reporting route outside the terms** so a researcher can find the scope and the contact without
   accepting a contract, and **verify a repository owner's request not to be scanned** the way a push is verified, applying it to
   forks and renames the operator can identify, to queued jobs and to future jobs (existing results are handled by section 10).

### A.3 Boilerplate: the authorisation the site asks for at intake

The operator's site shows this to the user before it hands a repository to CodeRipper. `{repository}` and `{username}` are filled in by
the site, which is the only party that knows them.

> **Before we scan `{repository}`:** `{username}`, please confirm that you have the right to ask for this. By continuing you state that
> (a) you are entitled to submit `{repository}` for scanning, as its owner, a maintainer with authority to do so, or with the owner's
> permission; (b) you authorise `{OPERATOR_NAME}` and the runners it uses to fetch, compile, execute and analyse that repository at the
> commit you chose, only to produce the report you asked for; (c) you understand that compiling a Rust repository runs its build
> scripts and procedural macros on a machine that is not yours; and (d) submitting it breaks no law, no agreement binding you and no
> right of anyone else. If you are not sure, do not continue.

The user must take a **positive step** (a button or box labelled as agreeing, with a visible link to the full terms, not a notice
below the fold or a bare "continue"). The site records the user, the repository and its owner or organisation, the immutable commit, the time, and the version (or a
hash) of the rendered terms shown, and no more personal data than that.

### A.4 Legal assumptions nobody qualified has checked

| # | Assumption | What changes if it is wrong |
|---|---|---|
| 1 | A report's findings (short quoted file paths and lines, rule ids, identifiers) are not a copy of the repository's source and need no licence. A finding can still quote an identifier, a secret or personal data. | Results would be limited to counts and locations, and fields would be allow-listed. |
| 2 | A user's statement at intake (A.3), plus the push check the site makes through its forge, is enough to authorise a scan of a public repository. A fork or an organisation may have rules of its own. | The site would need an owner check or an organisation opt-in. |
| 3 | Running a repository's build scripts in an isolated runner, at the request of someone who has stated that they may ask, is not unauthorised access. | The service could not run build-time code (Face A only). |
| 4 | Calling the service a processor of user-requested jobs does not by itself settle who is liable for what a repository's code does in a sandbox. **It is not relied on**: the sandbox, the limits and the abuse policy are what the operator relies on. | Liability and insurance would have to be answered first. |
| 5 | A notice-and-takedown process like section 10 is adequate for a service that stores findings and not the repository's content. Whether the US DMCA, the EU DSA or another regime applies to a given operator is unknown. | A designated agent and a counter-notice flow would be needed. |
| 6 | The warranty disclaimer and the liability cap in section 9 are enforceable under Arizona and federal law and where users live. Checked in Part D; not settled. | A cap below what a court accepts, or a clause it refuses, falls away; the carve-outs in section 9 are what keeps the rest. |
| 7 | The retention in section 8 is compatible with privacy law where users live. | A fuller privacy notice, a lawful-basis statement and a data-subject-request process would be needed. |
| 8 | Running untrusted jobs on a runner host does not breach its provider's terms (a VPS provider's or a home internet provider's). | Runner locations would have to change. |
| 11 | The cap and the compensation clause (section 9) hold for users outside Arizona. **Not checked.** The research behind them (Part D) is about Arizona and the federal courts only; consumer-protection and unfair-terms rules elsewhere (for example in the EU and the UK, which were not researched here) may void a cap or a compensation clause for a consumer. | The clauses fall away for those users, and the operator carries that risk; each stands or falls alone (section 12). |
| 9 | A user's intake statement is a meaningful record that the user, not the operator, vouched for the right to scan. | The operator carries the whole risk of an unauthorised scan. |
| 10 | The user's promise to compensate the operator (section 9) is enforceable. It is limited to **third-party claims caused by the user's own breach or lack of right**, and excludes the operator's own fault. No Arizona authority was found on a consumer's indemnity; a broader clause would risk being held unconscionable (Part D). | The operator carries more of its own risk. |

### A.5 What the renderer and the service must check (design; nothing is built)

A rendered template is only as true as the service behind it. A service must **not start accepting jobs** while any of these fails:

1. Every required value is present, and the relationships hold: the run-time ceiling is at least the default; the retention
   ceiling is at least the default; metadata and backup periods are consistent with the retention shown; the queue step is positive.
2. **Sections follow the configuration:** no community-posting text if `{COMMUNITY_NAME}` is unset; no external-runner sentence if
   the pool is the operator's own; no backup sentence if there are no backups; no copyright procedure if `{DMCA_AGENT}` is unset.
3. No `{PLACEHOLDER}` remains in the rendered text; the exact rendered text is stored and versioned, and the version a user accepted
   is recorded (A.3).
4. Each promise has an owner and a way to see it holds: retention and expiry (the cleanup worker is running), backup removal,
   source cleanup after success, failure, timeout and forced kill, takedown response times, the queue's published rule.
5. Golden tests render a minimal operator, one with external runners, one without backups, and invalid or contradictory
   configurations (which must fail).

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

- You need an account and must be logged in. You must meet these requirements to open one: `{MIN_AGE}`. How we handle your
  account details is in our privacy notice (`{PRIVACY_POLICY_URL}`).
- You are responsible for what your account submits.
- You may not use more than one account, an identity other than your own, or automated submissions to get around the queue ordering
  in section 7, a suspension, or any other limit of the service. Accounts for an organisation, and automation that follows these
  terms, are fine.

### 3. What you may submit, and what you give us

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

**Face A uploads.** A Face A upload is a result, not a repository. You give `{OPERATOR_NAME}` a limited permission to receive,
check, store, display to the people you choose, and delete the result and its metadata, and to handle security and abuse; nothing
more. You must have the right to upload it: a result can name files and identifiers from a repository you may run CodeRipper on but
may not be free to republish.

### 4. What we do with a submission

- We fetch the repository at one commit and run CodeRipper in a fresh, isolated runner. The runner (the disposable environment the job ran in) is destroyed after the job, while the report and job metadata are kept
  (section 8); a job
  that does not end is killed (see "Limits" below) and the runner destroyed then.
- **Your job may run on a machine run by someone other than `{OPERATOR_NAME}`**, a runner provider we have admitted to our pool. That
  machine sees the repository and runs its build scripts. **Only public repositories are accepted**, so the runner sees the repository as it is published (a public repository can still hold
  things its owner did not mean to share); the result says which runner ran the job.
- **Dependencies are fetched from the public crate registry before the build; the build itself runs with no network access.** A build
  script that needs the network fails, and the report says so: it is a gap, not a pass.
- **Limits.** Each job has a maximum run time of `{MAX_RUNTIME_MINUTES}` minutes by default (you can change it on your account where the service
  allows it, up to `{MAX_RUNTIME_CEILING_MINUTES}` where one is set), and caps on memory, disk and processes. A job that reaches a cap is stopped, and the report says
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
use it, do not look at other users' data and tell us. **If you do those things and report it promptly to `{CONTACT}`, we will
take no action against your account for the discovery**; that is a promise about what we do, not about anyone else or about the
law. Deliberate testing still needs an arrangement first. Without an arrangement, an intentional attempt to escape the sandbox is a breach of these
terms. `[ASSUMPTION 3]`

### 6. Results and what you may do with them

You may use, share and publish your report. Before you publish it, read it: it may name files, identifiers or other material of the
repository, and you are responsible for what you publish. A report's being visible does not give anyone a right to anything it quotes.
If a report exposes a secret or personal data, tell us at `{CONTACT}` (section 10).

### 7. Fair use and the queue

We do not set a quota on how much you use in total. When the runners are busy, each job is given a **weight** when you submit it, and
the lightest weight runs first. The weight is made of three things: how much run time your account has used recently (including jobs
you have waiting), how long the job is expected to take (so that a 20-minute job waits behind 30-second ones), and an adjustment that
`{OPERATOR_NAME}` may apply, for example to give paying customers priority; the ordering rule, **including that such an
adjustment exists**, is published. **While a job waits, its weight is lowered
every `{QUEUE_STEP_MINUTES}` minutes**, so a heavy job is delayed but not delayed forever. Each account may have **at most two jobs
running at once**: a safety limit for the machines, not a cap on total use.

The ordering is a best effort, not a guarantee of when a job runs. When the queue is full we refuse a new job and ask you to try later.
**A job we have accepted** (it is in the queue) is retried on another runner if its runner fails, a bounded number of times; if we
cannot finish it, it is marked **failed** and you are told, never silently dropped. We may apply temporary safeguards against abuse,
capacity problems or an emergency, including delaying or refusing submissions. We may change the numbers; the ordering rule will always
be published.

### 8. Results, retention and what we store

Three different things are promised separately.

- **Who can see a result.** You choose, per result. Nothing is public unless you make it public. A result you make public can be opened by
  anyone who has its link; a link is not a password. We ask search engines not to index results, but they may not honour that.
- **How long it is kept.** `{RESULT_RETENTION_DAYS}` days by default; you can set a different default on your account, up to
  `{RESULT_RETENTION_MAX_DAYS}` days. After that the result is deleted from our active systems and its link says "expired".
- **What deletion means.** A result you delete, or that expires, is removed from our active systems promptly; copies in backups are
  removed within `{BACKUP_EXPIRY_DAYS}` days (or at the next backup rotation, where the operator says so). Making a result private does not take back a copy someone already has, and a post you
  published elsewhere is not removed when the result expires (you may delete it yourself).

**What we store about a job.** Your account; the repository and commit; the report; and **job metadata that CodeRipper produces with the
report**: when it ran, which rules ran, which runner, run time, CPU, memory and disk use, and the outcome. Metadata is kept
`{JOB_METADATA_RETENTION_DAYS}` days after the result expires or is deleted, whichever comes first, so that the queue ordering and abuse handling can work. We do not keep the
repository's source after the job. What our site records about your visits (addresses, sessions) is described in our
privacy notice (`{PRIVACY_POLICY_URL}`), not here. `[ASSUMPTION 7]`

**Community posts.** If `{COMMUNITY_NAME}` offers it, the service prepares a summary for you to post; you publish it, and it stays
under your control.

### 9. No warranty, and limits on our liability

The service and its reports are provided **as they are**. We do not promise that a report is complete or correct, that a repository is
safe, that a job will complete, or that the service will be available. We may change or end features, rules, supported languages and
infrastructure.

To the extent the law allows, we exclude liability for indirect or consequential loss, for loss of data, profit or business, and for
loss arising from your reliance on a report. **Our total liability to you for everything arising from the service is limited to
`{LIABILITY_CAP}`.** This does not exclude or limit liability for death or personal injury, for fraud, for wilful misconduct or gross
negligence, or for anything else the law does not allow to be excluded or limited, and it takes away no right you have by law that
cannot be waived. `[ASSUMPTION 4]` `[ASSUMPTION 6]`

**Your responsibility.** You are responsible for what you submit and publish. If someone other than you brings a claim against us
because you submitted a repository you had no right to submit, or because you broke these terms, you will compensate us for the loss
and the reasonable costs of dealing with that claim, **except to the extent our negligence, wilful misconduct or breach of these terms caused it**. `[ASSUMPTION 10]`

The authors of CodeRipper, the software we use, are not a party to these terms and are not responsible for this service;
`{OPERATOR_NAME}` alone runs it.

### 10. Takedown, abuse and vulnerability reports

Write to **`{CONTACT}`** and say which of these it is. These are separate because they need different things from us. **Times
below run from when we receive a report that identifies the result.**

| You are reporting | Send us | We do |
|---|---|---|
| **A copyright complaint**: a result contains material you own that you did not allow | a notice with the elements listed below the table (signature, the work, the result, your contact, and the two statements) | hide the result within `{TAKEDOWN_RESPONSE_HOURS}` hours; tell the submitting account and give it the notice; consider any reply from the account and restore the result if it shows a right to use it |
| **A request to remove a result about your repository, or naming you** (not a copyright claim) | the result link and how you are connected to the repository | restrict it while we look; hide it within `{TAKEDOWN_RESPONSE_HOURS}` hours if you are connected to the repository and it should not be public; tell the account (see the note below the table on what hiding means). If you own a repository, you may ask us not to scan it again |
| **Personal data or a confidential matter in a report** | the result link and what it exposes | restrict it at once, then delete it from active systems |
| **A secret in a report** | the result link | restrict it at once, then delete it; **rotate the secret yourself: a secret shown in a report is exposed** |
| **Someone is using our runners to attack or mine** | the time and the address or account you saw | suspend the account, kill its jobs, keep the job metadata, tell you what we can |
| **A weakness in the sandbox, a runner or the service** | what you found and how to reproduce it | acknowledge within `{VULN_ACK_DAYS}` days, fix it as soon as we can, and credit you if you want. An acknowledgement is not a fix date |

**Copyright notices and replies (rendered only where `{DMCA_AGENT}` is set; send to `{DMCA_AGENT}`).** A notice must be in writing
and contain: (1) a physical or electronic signature of a person authorised to act for the owner of the right; (2) the work claimed to
be infringed; (3) the result, with enough to find it; (4) how to contact you; (5) a statement that you believe in good faith that the
use is not authorised by the owner, its agent or the law; and (6) a statement that the notice is accurate and, **under penalty of
perjury**, that you are authorised to act for the owner. Knowingly misrepresenting that material is infringing, or that it was
removed by mistake, can make the person liable for damages. When we hide a result we tell the account. **The account may reply** in
writing with: its signature; the result and where it was; a statement under penalty of perjury that it believes in good faith the
result was hidden by mistake or misidentification; and its name, address and telephone number, its consent to the jurisdiction of
the federal court for its district, and its agreement to accept service from the person who gave the notice. We pass the reply to
the person who sent the notice and tell them we will restore the result in not less than 10 and not more than 14 business days,
unless they tell us first that they have started a court action against the account.

A result reported in these ways may be hidden or restricted without warning, while we look. **Hiding is not a decision that the
complaint is right.** For a complaint that is not a copyright claim we keep the right to restore the result, or not to remove it,
when the finding is true and relevant and nothing in it needs protecting (a secret, personal data). Where an account is repeatedly the subject of valid
complaints we warn, suspend or end the account. A person whose result was hidden may ask why at `{CONTACT}`. We keep a record of each
request and what we did. If you use a service run by someone else, write to them, not to the CodeRipper authors. `[ASSUMPTION 5]`

### 11. Suspension and ending

We may stop a job, remove a result, or suspend or end an account if you break these terms, if the law or a legal order requires it, if
you threaten the safety of our machines or other users, or if continuing is unsafe. In an urgent case we may do this first and tell you
afterwards; queued jobs of a suspended account are cancelled. You may ask us at `{CONTACT}` to review a decision. You may delete your
account at any time; its results are deleted as in section 8, and posts you published elsewhere remain under that service's rules.

### 12. Changes, law and contact

We may change these terms. A change that affects you is announced on the service, and by email where we have your address, before it applies, applies to jobs submitted after
it, and if you do not accept it you may stop using the service and delete your account. These terms are governed by
`{GOVERNING_LAW}`, and, to the extent the law allows, disputes are heard in `{VENUE}`; where the law gives you the right to go
to a court elsewhere, that right stays. If a part is found unenforceable, the rest stands: **each clause, and each limit and exclusion in section 9, stands or falls on its
own.** Contact: `{CONTACT}`.

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
container, on a machine that also holds personal files. CireSnave's 2026-10-13 ruling is containers for v1, and (same day) home PCs are
**in** v1 as runner hosts. That makes 10.5's conditions requirements for the runner image and its host documentation, not options: no
route to the home LAN, no personal credentials on the host, egress limited by the host. An image cannot enforce the host's network
segmentation, so the runner documentation must say it is the host owner's duty and the runner should refuse to register without
an attestation that it was done. These terms say "isolated runner" and name no technology. The design needs a reconciliation before Face B opens: a hardened
container runtime (a user-space kernel such as gVisor, or rootless with a seccomp profile, no capabilities and resource limits) on a
host with no route to a home LAN is closer to the design's intent than a plain container, and a home PC is the worse host. Also for the
design: the self-registering runner image (runners admitted to a pool, each result naming its runner), the intake interface that
carries the authorisation record, the result-and-metadata interface the site serves, and the installer/config that renders this
template.

**Launch gate (a third review, 2026-10-13, and CireSnave's home-PC ruling).** Face B does not open on any host class until the
hostile fixtures of the design's section 8 have been run against that class and passed. Home PCs are in v1 as runner hosts
(CireSnave, 2026-10-13) **on that condition**; a host that cannot pass is not used, and the terms' "isolated runner" is then not
claimed for it.

## Part D. Preliminary legal research and unresolved questions (web research, 2026-10-13; not legal advice; no lawyer)

CireSnave asked that the liability and compensation clauses be checked against Arizona and federal law, saying that a clause that is not
enforceable is not helpful. **This was desk research from case summaries and a few primary pages, by an AI agent, with no lawyer**. It
found what follows and it found gaps; the gaps are listed. Cases are named by citation, not by link, so that no link goes stale. **Every case citation below was checked on 2026-10-13
against a court-listing page (CourtListener or a reporter index): volume, page, court and date. The holdings were read from the
opinion (*Phelps*) or from case summaries (the others); the full text of *Ocotillo*, *Sirek*, *Salt River Project* and *Maxwell* was not
read.**

**Found, and used in section 9.** Each item carries a status: *[statute text read]*, *[constitutional text, summary read]*,
*[case law, opinion read]*, *[case law, summary read]*, *[secondary source]* or *[drafting judgment]*. **None is an enforceability
finding.** What the research did not settle is listed after.


- **A cap on liability is enforceable in Arizona if it is not against public policy.** *[case law, summary read]* *1800 Ocotillo, LLC v. WLB Group, Inc.*, 219 Ariz. 200,
  196 P.3d 222 (Ariz. 2008): a clause capping a surveyor's negligence liability at its fees was held enforceable, not against public policy, and
  *not* subject to the constitutional rule that assumption of risk is a jury question. The court distinguished a cap from a clause that
  removes liability altogether, and said a cap could be set **so low that it removes the incentive to take care**; it found no such
  problem there. That is why `{LIABILITY_CAP}` has no CodeRipper default and carries a warning. (That case was a commercial contract
  between businesses; a consumer setting is not decided by it.)
- **A clause that removes one's own liability for negligence is disfavoured and read narrowly.** *[case law, summary read]* *Salt River Project Agric. Improvement
  & Power Dist. v. Westinghouse Elec. Corp.*, 143 Ariz. 368, 694 P.2d 198 (1984), and *Sirek v. Fairfield Snowbowl, Inc.*, 166 Ariz. 183,
  800 P.2d 1291 (App. 1990): the intent must be spelled out, clearly and explicitly (*Sirek*, 166 Ariz. at 187), so that the signer is
  alerted to giving up a right. *Phelps* cites *Salt River Project* and does not overrule it (read in the opinion, 2026-10-13).
  Section 9 therefore says plainly what it excludes, and does not try to exclude more than the law allows.
- ***Phelps v. Firebird Raceway, Inc.*, 210 Ariz. 403, 111 P.3d 1003 (2005):** *[case law, opinion read]* an *express contractual assumption of risk* is a question of fact for a
  jury under Art. 18, section 5 of the Arizona Constitution. So a broad release a user "assumes the risk of" can be put to a jury and
  cannot be relied on as a rule. Section 9 is a cap and an exclusion of kinds of loss, not an assumption-of-risk release.
- **Death and personal injury.** *[constitutional text, summary read]* Arizona Const. Art. 2, section 31 bars laws limiting damages for death or injury, and Art. 18, section 6
  protects the right of action for injury from being abrogated. A software scan is unlikely to cause either, but the carve-out is there
  so the clause does not claim what the constitution forbids.
- **Fraud, wilful misconduct and gross negligence** *[drafting judgment]* are carved out as a conservative default. No Arizona authority was found on
  gross negligence in a limitation clause.
- **Compensation (indemnity).** *[case law, secondary source; drafting judgment]* Arizona enforces indemnity clauses, reads them strictly, and requires a clear statement before one covers
  the indemnitee's own negligence (*Salt River Project*, above, as summarised by secondary sources; no decision is cited for the
  second half beyond that). The clause in section 9 therefore covers third-party claims arising from the **user's** act and
  expressly **excludes our own fault**. Unconscionability in Arizona (*Maxwell v. Fidelity Fin. Servs., Inc.*, 184 Ariz. 82, 907 P.2d 51 (1995))
  can be shown by substantive unconscionability alone, which a one-sided clause invites; narrowing the clause is the protection.
- **Assent.** *[case law; general principle]* Courts across the country enforce a clickwrap (a positive agreement step with the terms linked) and refuse a browsewrap
  that gives no notice (*Specht v. Netscape Communications Corp.*, 306 F.3d 17 (2d Cir. 2002), decided 2002-10-01). The intake boilerplate therefore requires a positive step
  and a visible link, and the site records the text version.
- **Federal copyright.** *[statute text read]* To keep the safe harbour for material stored at a user's direction, 17 U.S.C. section 512(c) requires a
  designated agent (name, address, phone and email) given to the Copyright Office and published on the service's website, and section
  512(i)(1)(A) a repeat-infringer policy that is "adopted and reasonably implemented" and that the provider tells its users about
  (statute text read on 2026-10-13). Section 10 and Part A.2 item 5 put this on the operator. The statute lets the Register charge a
  fee for the directory; **the amount and any renewal period set by the Copyright Office's own rules were not verified.**

**Not found, so not relied on**

- Arizona authority on a **consumer's** indemnity of a service operator, and on gross negligence in a limitation clause.
- The text of Arizona's electronic-transactions statute (A.R.S. 44-7001 ff.) and an Arizona decision on clickwrap assent. The general rule
  above is national.
- Whether a free service's **nominal** cap is accepted. *Ocotillo* warns against one that removes the incentive; no number was found.
- Anything about the laws of the places users live. Section 9 ends with "or anything else the law does not allow", which is a
  fallback, not a check.
- Minors: Arizona's age of majority is 18, and a contract with a minor may be avoided by the minor. This is why `{MIN_AGE}` is required.

**Bottom line.** The cap, the exclusions of kinds of loss, the narrow compensation clause and the carve-outs are drafted to stay inside
what these sources say Arizona enforces. They are a reasoned best effort, not a guarantee, and the gaps above are genuine.
