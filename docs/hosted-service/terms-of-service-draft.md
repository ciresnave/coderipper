# CodeRipper hosted service: Terms of Service and takedown policy — DRAFT

> **Status: a draft for CireSnave's approval. It is not legal advice and it has not been reviewed by a lawyer.** It was written by an AI
> agent (the CodeRipper lane) from CireSnave's rulings of 2026-10-12 and the design in
> `docs/superpowers/plans/2026-10-12-hosted-service.md`. Nothing here is in force and no service exists yet.
>
> **How to read the marks.** `[ASSUMPTION n]` marks a legal assumption the author could not verify; the list is in section A and each
> one needs a person qualified to answer it. `{CONTACT}` is the abuse and takedown contact: **only CireSnave supplies it.**
> `{NAME}` marks another value only CireSnave can supply. The user-facing text starts at section 1.

## A. Legal assumptions to resolve before this is published

| # | Assumption this draft makes | What would change if it is wrong |
|---|---|---|
| 1 | Findings about a repository, with short quoted file paths and lines, are not a copy of the repository's source and do not need a licence from its owner. | Results would have to be limited to counts and locations. |
| 2 | A user who can **push** to a public repository is entitled to have it scanned and to publish the results. (A fork's or an organisation's rules may differ.) | The push check in section 3 would need an owner check or an organisation opt-in. |
| 3 | Running a repository's build scripts in an isolated runner at the owner's request is not unauthorised access. | The service could not run build-time code at all (face A only). |
| 4 | The service is a conduit for user-requested processing and is not responsible for what a repository's code does inside the sandbox, provided the sandbox and abuse policy are applied as written. | Liability and insurance questions would have to be answered first. |
| 5 | A takedown procedure in the shape of section 9 is enough; no statutory notice-and-takedown regime (the US DMCA, the EU DSA) applies, because the service hosts findings and not the repository's content. | A designated agent and a statutory counter-notice flow would be needed. |
| 6 | The limitation of liability and the disclaimer of warranty in section 8 are enforceable in the places users live. | Local consumer law may override them. |
| 7 | The retention and logging in section 7 are compatible with the privacy law of the places users live (the draft stores an account, a repository id, a commit and job metadata, not the repository's source). | A privacy notice, a lawful-basis statement and a data-subject request process would be needed. |
| 8 | Minimum age and the governing law/venue are left as `{NAME}`. | — |
| 9 | Running untrusted jobs on the operator's own machines (a VPS and home PCs, per CireSnave 2026-10-12) does not breach any terms of those machines' internet providers or hosts. | Runner locations would have to change. |

## 1. What this service is

CodeRipper reads a software repository and reports what automatic linters and compilers do not: unused public functions, ignored
return values, unused parameters, inconsistent versions, missing CI protection, and a set of file-level rules. The hosted service
runs it for you in one of two ways:

- **From your own CI (face A).** Your workflow runs CodeRipper on your CI runner and uploads the result. We never run your code.
- **By URL (face B).** You name a repository; we fetch it and run CodeRipper in an isolated runner. This runs your repository's
  build scripts and procedural macros, because checking Rust code means compiling it.

**What it is not.** It is not a security audit, a licence review or a guarantee. A report with nothing in it means "every rule
that could run did run and found nothing", and the report says which rules could not run and why.

## 2. Your account

- You need an account and must be logged in. `{NAME: minimum age and who may open an account}`.
- You are responsible for what your account submits.
- One person, one account. Do not create accounts to get around the fair-share ordering in section 6.

## 3. What you may submit (face B)

You may submit a repository only if **all** of these are true:

1. it is **public**;
2. you can **push** to it (we check through GitHub when you submit);
3. you are not submitting it to harm someone, to probe our infrastructure, or to use our machines for anything but the scan.

Private repositories are not accepted by URL in this version; use face A, which runs on your own CI.

## 4. What we do with a submission

- We fetch the repository at one commit, run CodeRipper in a fresh, isolated runner, and destroy the runner afterwards.
- **The build runs with no network access.** A build script that needs the network fails and the report says so; it is a gap, not a
  pass.
- The repository's source is not kept after the job. We keep the report, the commit id and job metadata (section 7).
- Output from your build scripts and tools is **not shown verbatim**: known secret shapes are removed and build-script output is
  replaced by a fixed sentence. This is a safeguard, not a promise that no secret can ever appear; do not put secrets in build output.

## 5. Acceptable use

Do not: submit a repository you cannot push to; try to escape, probe or overload the sandbox or the scheduler; use the runners to
mine, to attack, to host, or to send traffic; submit repositories whose only purpose is to attack the runner; scan at a rate meant to
crowd out other users; or upload a result you know is fabricated through face A.

**Security research on the sandbox itself** is welcome by arrangement: write to `{CONTACT}` first, and we will tell you what is in
scope. Without that, an attempt to escape the sandbox is a breach of these terms. `[ASSUMPTION 3]`

## 6. Fair use and the queue

There is **no quota** on how much you use. When the runners are busy, the queue is ordered **first by who has run the fewest jobs in
the last month and, second, by who has used the least resource**, so that only heavy users wait. Each account may have **at most two
jobs running at once**; that is a safety limit on the machines, not a cap on how much you may use in total. When the queue is full
we say so and ask you to try again later; we do not discard a job we accepted. We may change the numbers; the ordering rule will be
published.

## 7. Results, retention and what we store

- **Visibility is your choice** per result. Nothing is public unless you make it public.
- **Retention:** results are kept for **30 days** by default; you can set a different default on your account, up to **365 days**.
  After that the result is deleted and its link says "expired".
- **Posting to the community is done by you.** The service prepares a summary; you publish it. A post you published stays under your
  control and is not removed when the result expires (you may delete it yourself).
- **What we store:** your account, the repository id and commit, the report, and job metadata (when it ran, time, CPU and memory used,
  outcome) for `{NAME: job-metadata retention, proposed 90 days}`. We do not store the repository's source after the job.
  `[ASSUMPTION 7]`

## 8. No warranty, and limits on our liability

The service and its reports are provided **as they are**. We do not promise that a report is complete or correct, that a repository
is safe, or that the service will be available. To the extent the law allows, we are not liable for loss arising from your use of
the service or from relying on a report, and `{NAME: liability cap}`. `[ASSUMPTION 4]` `[ASSUMPTION 6]`

## 9. Takedown, abuse and vulnerability reports

Write to **`{CONTACT}`** and say which of these it is.

| You are reporting | Send us | We do |
|---|---|---|
| **A result or post that is about your repository or names you, and you want it removed** | the result link or post link, and how you are connected to the repository | hide the result and our dashboard copy within `{NAME: response time}`; ask the Community team to hide a post of ours; tell the account that submitted it |
| **Someone is using our runners to attack, or to mine** | the time, the address or the account you saw | suspend the account, kill its jobs, keep the job metadata, tell you what we can |
| **A weakness in the sandbox or the service** | what you found and how it can be reproduced | acknowledge within `{NAME: response time}`, fix, and credit you if you want |
| **A secret that appears in a report** | the report link | delete the report at once, then rotate the secret yourself (a secret shown in a report must be treated as exposed) |

We may hide or delete a result without warning where it is reported as above. We keep a record of each request and what we did.
A person whose result was hidden may ask us why at `{CONTACT}`. `[ASSUMPTION 5]`

## 10. Suspension and ending

We may stop a job, remove a result, or suspend an account that breaks section 5. You may delete your account at any time; your
results are deleted with it, and posts you published on the community remain under that service's own rules.

## 11. Changes, law and contact

We may change these terms; a change that affects you is announced on the service before it applies. These terms are governed by
`{NAME: governing law and venue}`. Contact: `{CONTACT}`.
