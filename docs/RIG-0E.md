# Rig assessment (0e): coderipper 0.33.3 at 6a2252ef on 0xPlaygrounds/rig 44f8d5b8
Scratch paths are not kept; this file is the record.
Command: sweep --profile portfolio --workspace --deny none --message-format json, Windows host. Exit 3: 2621 findings, 42 errors. Read-only; nothing posted upstream.
Sample: seeded random (20261104), up to 30 per rule, all if fewer; 329 findings judged by Sonnet reviewers reading the pinned source (model judgment; I re-checked 3 reachability FALSE verdicts by hand: RUN_STEEL, query, save_state have direct callers).
Verdicts TRUE = accurate and actionable; ARGUABLE = accurate but a maintainer would decline; FALSE = claim wrong.
rule | population | n | TRUE | FALSE | ARGUABLE
reachability | 1566 | 30 | 0 | 23 | 7
unused-return-values | 71 | 30 | 1 | 0 | 29
lint-suppression | 221 | 30 | 1 | 3 | 26
OPS-011 | 73 | 30 | 0 | 0 | 30
dependency-freshness | 475 | 30 | 22 | 0 | 8
version-consistency | 22 | 22 | 0 | 0 | 22
SUP-004 | 19 | 19 | 2 | 0 | 17
MOD-005 | 56 | 30 | 6 | 0 | 24
MOD-003 | 36 | 30 | 0 | 0 | 30
MOD-002 | 34 | 30 | 11 | 0 | 19
MOD-008 | 22 | 22 | 0 | 2 | 20
MOD-001 | 17 | 17 | 10 | 0 | 7
OPS-012 | 4 | 4 | 0 | 0 | 4
CON-006, ARC-008 | 1 each | 1 | 0 | 1 each | 0
licence, WSP-001, patent-sweep-record | 1 each | 1 | 0 | 0 | 1 each
Defects in CodeRipper found:
1. reachability false positives (23/30 sample; 1566 findings = 60% of the run): misses callers through Bevy registration (add_systems/add_observer/Plugin::build), types appearing only as parameters of registered handlers (On<ToolCalled<X>>), marker components in With<X>, method-call syntax, and some plain direct calls (RUN_STEEL in a call argument list, query inside a move closure, save_state). The false ones are concentrated in rig-steel and corpus-oc/sp, where the check did run.
2. lint-suppression FALSE (3/30): a reason comment exists on the same line, the line above, or two lines above.
3. MOD-001 on rig-tools emitted a finding claiming an absence with no positive control (run error "claims an absence").
4. CON-006 (rig-scylladb): guard moved into a closure and dropped there before the else takes the write lock. ARC-008 ErasedModel: blanket impl behind a dyn erasure. MOD-008: Bevy systems differing only by Query filter.
5. OPS-011 flags demo examples with no --help/--version; summary does not name the failed probes (unverifiable).
Not a defect in the check: 42 errors = ci-protection-presence (merge settings need admin), corpus/example crates that cannot build on Windows (os::unix), positive-control failures on rig-claude-cloak, rig-cassette-minimal/frozen_api/rig-service-tests missing workspace files.
Real hits: dependency-freshness 22/30 (8 ARGUABLE are deliberate pins: Bevy =0.20.0); SUP-004 2 (rig-harness tests include_str! into rig-memory, no exclude, packaged tests cannot compile); MOD-002 11 and MOD-001 10 and MOD-005 6 TRUE; lint-suppression 1, unused-return-values 1.
Limits: judgments are model-made on a 329-finding sample, I verified only 3 myself; MOD-002/MOD-005/MOD-001 counts were not re-counted by the reviewers; Windows host only.
