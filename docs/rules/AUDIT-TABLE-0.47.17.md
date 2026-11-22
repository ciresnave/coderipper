# Claims audit of the 0.47.12 rules (coderipper-c, 2026-11-17, branch pkg-c-next @ d47237a)
Claims taken from each rule's module doc (src/module/neutral/<id>.rs), catalog `checked_today` (rules/*.toml), docs row (docs/rules/*.md) and changelog fragment (the last three paraphrase the first). Test prefix `claims::` = `module::neutral::<mod>::claims::`; `tests::` = the rule's own earlier test module; `batch::` = `module::neutral::tests::`; `conf` = `cargo test --test conformance` fixture pair `conformance/<ID>/{defective,clean}`.
Verdicts: PROVEN = a named test fails if the claim is false. WRONG->FIXED = the probe failed, code or text corrected on #363. UNPROVEN = no test can check it (reason given).

| rule | claim | proving test | verdict |
|---|---|---|---|
| RDB-002 | fn/method/binding/parameter names snake_case | claims::every_documented_kind_of_name_is_read_including_methods_unions_and_parameters; tests::each_kind_of_name_is_checked_against_its_own_convention | PROVEN |
| RDB-002 | struct/enum/union/trait/type/variant UpperCamelCase, and the exact camel rule (not lower first, no `__`, no `_` beside a letter) | claims::the_camel_case_rule_is_start_not_lower_no_double_underscore_and_no_underscore_beside_a_letter | PROVEN |
| RDB-002 | const/static SCREAMING_SNAKE_CASE; snake = no upper-case letter | claims::the_snake_and_screaming_rules_are_no_upper_case_and_no_lower_case_letter | PROVEN |
| RDB-002 | leading `_` and `r#` ignored | tests::conforming_names_are_not_reported; claims (the `_private`, `r#type` cases) | PROVEN |
| RDB-002 | item-level allow of the matching lint or nonstandard_style clears it; another lint's allow does not | claims::an_allow_of_the_matching_lint_or_nonstandard_style_clears_an_item_and_another_lint_does_not | PROVEN |
| RDB-002 | inner allow on a file or module is honoured | tests::an_inner_allow_of_a_file_or_module_covers_only_its_own_lint_and_its_scope | WRONG->FIXED [FIX 1] |
| RDB-002 | no_mangle / export_name items, extern blocks, trait-impl methods, macro bodies not read | claims::no_mangle_export_name_extern_blocks_trait_impl_methods_and_macro_bodies_are_not_read | PROVEN for the bare forms; `#[unsafe(no_mangle)]` WRONG->FIXED [FIX 2] |
| RDB-002 | test code skipped | tests::test_code_is_not_read | PROVEN |
| RDB-002 | Low severity, High confidence, at the name | claims::a_finding_is_low_severity_high_confidence_at_the_name_and_an_unparsable_file_is_noted | PROVEN |
| RDB-002 | unparsable file named in a note, never read as clean | same + batch::a_repository_whose_only_rust_file_does_not_parse_gets_a_note... | mixed repo PROVEN; sole-file case WRONG->FIXED [FIX 4] |
| RDB-002 | opt-in; Rust only; fixture pair | batch::every_rule_of_the_batch_is_opt_in_and_registered; batch::the_rust_rules_of_the_batch_read_rust_only...; conf | PROVEN |
| RDB-002 | "rustc's three lints are the same idea at type-checker precision" | none (comparison with another tool) | UNPROVEN: prior-art statement, not checkable by a test |
| REL-002 | retry loop = loop/while/for whose OWN body sleeps and looks at a failure (nested loop/closure/fn excluded) | claims::a_sleep_in_a_nested_loop_closure_or_function_belongs_to_that_unit; tests::a_loop_that_sleeps_without_looking_at_a_failure_is_not_a_retry | PROVEN |
| REL-002 | the three failure forms (`Err` arm, `if let Err`, `.is_err()`) and a path ending in `sleep` (std, tokio, async_std) | claims::the_three_failure_forms_and_a_path_ending_in_sleep_make_a_retry | PROVEN |
| REL-002 | no cap: loop, while with no counter in the condition, for over `0..`/repeat/cycle; a comparison naming a counter is a cap | claims::a_while_condition_that_names_a_counter_is_a_cap; claims::a_for_over_an_endless_iterator_is_uncapped; tests::a_capped_loop_with_a_growing_delay_is_not_reported | PROVEN |
| REL-002 | no backoff: every sleep is a fixed time written out in the call (literals, SCREAMING_CASE) | claims::a_function_call_is_a_computed_delay_and_a_screaming_constant_is_a_fixed_one | WRONG->FIXED [FIX 3] (a call `delay()` was read as fixed) |
| REL-002 | a delay held in a variable is read as growing (stated limitation) | claims::a_delay_held_in_a_variable_is_read_as_growing_which_the_documentation_says | PROVEN |
| REL-002 | one Medium/Low finding per loop, both faults in it | claims::a_finding_is_medium_severity_low_confidence_one_per_loop_with_both_faults_in_it | PROVEN |
| REL-002 | test code skipped; unparsable named; opt-in; Rust only; fixture | tests::test_code_is_not_read; batch::*; conf | PROVEN ([FIX 4] for sole file) |
| REL-002 | idempotency, jitter, shared budget, stacked layers, retry crates NOT judged | none (absence of a feature) | UNPROVEN: stated non-goals; no test can prove a rule does not look at something beyond the cases above |
| ERR-002 | catch_unwind call forms: std::panic::, panic::, bare, `.catch_unwind()` | claims::every_documented_call_form_is_a_panic_catcher | PROVEN; doc said "a bare one the file imports" but any bare call counts: WRONG->FIXED [FIX 5] (wording) |
| ERR-002 | boundary = main, binary target fn (src/main.rs, src/bin/), boundary word among `_`-words | claims::every_boundary_word_makes_a_function_a_boundary_and_a_word_inside_another_word_does_not; claims::binary_targets_are_boundaries_by_path; claims::a_method_of_a_boundary_named_function... | PROVEN |
| ERR-002 | justifying comment on the line or in the three lines above, listed words; not four lines up | claims::a_justifying_comment_counts_on_the_line_and_up_to_three_lines_above_and_not_four | PROVEN |
| ERR-002 | swallowing `Err(_)` arm is ERR-001's, not read | claims::a_swallowing_err_arm_is_not_this_rules_and_the_finding_is_medium_medium | PROVEN |
| ERR-002 | Medium/Medium at the call; test code skipped; opt-in; Rust only; fixture; unparsable | same; tests::test_code_is_not_read; batch::*; conf | PROVEN ([FIX 4] for sole file) |
| TYP-004 | bare string types: String, &str, &String, Cow<str>, impl AsRef<str>, impl Into<String>; other shapes not | claims::every_documented_bare_string_type_is_one_and_other_shapes_are_not | PROVEN |
| TYP-004 | the 14 domain words, as whole `_`-words (curl/emailer/ipv6 not) | claims::every_documented_domain_word_is_read_as_a_whole_underscore_word | PROVEN |
| TYP-004 | the 19 entry words in the function name clear it | claims::every_documented_entry_word_in_the_function_name_clears_it | PROVEN |
| TYP-004 | only `pub` fns and pub inherent methods outside trait impls; pub(crate), private, trait decls not | claims::only_pub_functions_and_pub_inherent_methods_outside_trait_impls_are_read | PROVEN |
| TYP-004 | Medium/Low at the function; contested | claims::a_finding_is_medium_severity_low_confidence_at_the_function; `contested = true` in rules/typ.toml | PROVEN (severity); contested flag is a catalog attribute, not tested by a rule test (catalog suite checks the file parses) |
| TYP-004 | test code skipped; opt-in; Rust only; fixture; unparsable | tests::test_code_is_not_read; batch::*; conf | PROVEN ([FIX 4] for sole file) |
| TYP-004 | "a narrow approximation; where data really enters is not seen" | none | UNPROVEN: stated limitation |
| ERR-007 | release forms: 5 methods; 7 functions with `x`, `&x`, `&mut x` | claims::every_documented_release_method_and_function_form_is_read | PROVEN |
| ERR-007 | release may be unwrapped, `?`-ed, awaited, in a single-statement unsafe block | claims::a_release_may_be_unwrapped_question_marked_awaited_or_in_an_unsafe_block; tests::a_question_mark_or_return_between... | PROVEN |
| ERR-007 | binding may be mut/typed; only the same block counts | claims::the_binding_may_be_mutable_or_typed_and_only_the_same_block_counts | PROVEN |
| ERR-007 | `?` or `return` between skips; closure or nested item between does not | claims::a_return_or_question_mark_between_skips_and_a_closure_or_item_between_does_not | PROVEN |
| ERR-007 | Medium/Low at the release call; subject file:function:resource | claims::a_finding_is_medium_severity_low_confidence_at_the_release_call | PROVEN |
| ERR-007 | test code, opt-in, Rust only, fixture, unparsable | tests::test_code_is_not_read; batch::*; conf | PROVEN ([FIX 4] for sole file) |
| ERR-007 | "most Rust types release on drop, so Low confidence" | none | UNPROVEN: rationale, not behaviour |
| OPS-007 | registry file names x extensions (7 stems x 4 ext); other names not | claims::every_documented_file_name_and_extension_is_a_registry | PROVEN |
| OPS-007 | 11 excluded directories | claims::every_excluded_directory_hides_a_registry | PROVEN |
| OPS-007 | list keys (4) and name keys (4, else `#n`) | claims::every_documented_list_key_and_every_name_key_is_read | PROVEN |
| OPS-007 | 7 owner keys and 11 sunset keys, lower-cased, `-` read as `_` | claims::every_owner_key_and_every_sunset_key_satisfies_its_half_in_any_case_with_dashes | PROVEN |
| OPS-007 | empty/null/blank/[] value records nothing; bare boolean map entry records nothing | claims::an_empty_null_or_blank_value_records_nothing; tests::a_map_of_flags_json_and_toml_are_read_and_a_bare_boolean_records_nothing | PROVEN |
| OPS-007 | Low/Medium at the line naming the flag; no registry = not applicable; unparsable registry = note | claims::a_finding_is_low_severity_medium_confidence_at_the_line_that_names_the_flag; tests::no_registry_is_not_applicable_and_an_unparsable_one_is_noted | PROVEN |
| OPS-007 | opt-in; reads any language (not in READS_ONLY); fixture | batch::*; conf | PROVEN |
| OPS-002 | read forms: env::var/var_os (std:: or not) with a literal; clap `env = "X"` in #[arg] and #[clap] with other keys; NOT run-time names, builder `.env`, `env!` | claims::the_documented_read_forms_are_collected_and_the_undocumented_ones_are_not | PROVEN |
| OPS-002 | name shape (upper-case, digits, `_`, >= 3) and the 50 platform/toolchain names + 6 prefixes left out | claims::a_name_must_look_like_a_key_and_no_platform_or_toolchain_name_is_configuration | PROVEN |
| OPS-002 | documentation = .md .markdown .txt .rst .adoc .env* *.example *.sample; not CHANGELOG, not vendor/test/fixture/conformance dirs; whole-word match | claims::every_documented_documentation_file_kind_documents_and_the_excluded_ones_do_not; tests::whole_word_matching_and_the_excluded_files_are_honoured | PROVEN |
| OPS-002 | one finding per name at its first read | claims::a_name_read_twice_is_one_finding_at_its_first_read | PROVEN |
| OPS-002 | Low/Medium; test code skipped; opt-in; Rust only; fixture; unparsable | claims::a_finding_is_low_severity_medium_confidence; tests::platform_and_toolchain_names_dynamic_names_and_test_code_are_not_read; batch::*; conf | PROVEN ([FIX 4] for sole file) |
| OPS-002 | measurement: 60 names in 2 of 4 repos (auth-framework 57, PersistAnt 3), Unpopped and agent-loop clean, coderipper clean (2026-11-16) | none (a measurement, not a behaviour) | UNPROVEN by test; MEASURED once; to be re-run on 11-19 against main |
| MOD-011 | extern block `pub` fns, statics and types are recorded; private ones and `doc(hidden)` are not | extern_block_symbols_are_kept_apart_from_the_budget; extern_block_functions_statics_and_types_are_all_recorded_when_pub; a_doc_hidden_extern_symbol_is_not_recorded | PROVEN |
| MOD-011 | only in a module reachable through `pub` modules | an_extern_block_in_a_module_not_reachable_through_pub_modules_is_not_recorded; classify::an_extern_block_in_a_private_module_is_neither_api_nor_exported_until_a_pub_use_names_it | PROVEN |
| MOD-011 | documented = library API, undocumented = exported; private neither | classify::a_pub_symbol_of_an_extern_block_is_exported_and_a_private_one_is_not | PROVEN |
| MOD-011 | the API budget does not change | extern_block_symbols_are_kept_apart_from_the_budget; the_foreign_sets_stay_apart_from_the_budget_counts_for_every_symbol_kind | PROVEN |
| MOD-011 | a finding at one is Info (exported) not Medium; findings only drop, none new | none end to end (reachability needs a throwaway build) | UNPROVEN by unit test; follows from the classifier tests above; MEASURED by the coderipper lane (Medium 2855 to 4 on five -sys crates, prototype build, --no-config) |

## Batch 2026-11-21 (pkg-c-audit-1122 @ 1bf86b2) — NOTE: ERR-001, ERR-009, COR-006 are opt-in (in no profile), so not "denied by portfolio"; pick default-on Medium+ rules next
| ERR-001 | "read, write and flush methods" on the if-let-Ok list | the_listed_io_methods_are_found_and_a_bare_read_or_write_is_not (new) | WRONG->FIXED [FIX 6] (probe: `if let Ok(n) = s.write(..)` not reported; list is read_to_string, read_to_end, write_all, flush; text corrected in err001.rs doc + rules/err.toml) |
| ERR-001 | the other four drop forms, comment-as-reason (same line, statement end, line above) | probe on a scratch crate + each_way_of_dropping_a_failure_is_found, a_comment_beside_the_site_is_the_stated_reason | PROVEN |
| ERR-001 | catalog default_severity "high" vs findings Medium/Medium | none | UNPROVEN: attribute vs emitted severity differ; to ask |
| ERR-003, ERR-009, COR-006, CON-001 | not yet audited | - | TODO (ERR-009/COR-006 opt-in) |
| CON-002 | guard forms lock/read/write (no args)/borrow/borrow_mut, with unwrap/expect/unwrap_or_else/?; let-bound held to block end or drop(g); temporary to statement end; awaited acquisition not a guard; async fn/block/closure only; Medium/Medium | probe of 16 shapes on a scratch crate (all as documented) + module tests a_guard_dropped_or_scoped_before_the_await_or_an_async_lock_is_clean etc. | PROVEN (a `drop(g)` in one branch ends the hold: a false negative, stated in the module doc line 13) |
| CON-006 | acquisition receivers self.field / self.a.b / self.s[i] / static; opposite-order nesting = finding; same lock re-taken = finding except per-index; temporary ends at statement; local/param receivers not seen; Low confidence | probe of 11 shapes (f1/f2, g1/g2, h, t/u/v, w, l/l2) + a_lock_taken_twice_is_reported_but_two_indexes_of_a_shard_array_are_not | PROVEN (finding severity High, confidence Low, as the text says) |
| SUP-002, SUP-008, API-001, API-006, OPS-006, SEC-009 | delegated to osv-scanner/zizmor/semver-checks/buf/gitleaks/checkov | - | NOT AUDITED: the claim is the external tool's; audit = delegation wiring, own batch |
| SEC-004 | literal key to from_secret/new_from_slice/Key::from_slice/hmac::Key::new/UnboundKey::new (string, byte literal, int array, local bound to one); empty literal, parameter, const not reported; danger_accept_* unless literal false (non-literal = Medium confidence); rustls .dangerous() | probe of 15 shapes on a scratch crate, all as documented | PROVEN |
| TYP-007 | >= 2 bare bool params; &bool, Option<bool>, one bool, alias, impl Trait for Type, cfg(test) not; inherent and trait-declared methods and private fns read | probe of 11 shapes, all as documented | PROVEN |
| TST-003 | empty / tautology-only / calls-nothing tests; smoke test, #[ignore], #[should_panic], assert_eq!(x,x) not | probe of 15 tests, all as documented | PROVEN |
| SEC-012 | derived Debug on secret field; field-name rules (bare key, password_hash, password_len, bool, secret_ttl, token_type not); token reported without vocab segment | probe of 18 structs, all as documented | PROVEN |
| SEC-012 | hand-written Debug that prints the field is reported, a masked one is not | a_hand_written_debug_that_prints_the_field_is_reported_and_a_masked_one_is_not; probe (I reported, J not) | PROVEN (a probe struct with a trailing `// no redact` comment was not reported: comment text counts as the word, an acceptable limit, not claimed) |
| REL-001 | opt-in (not in the portfolio profile) | - | skipped, not denied |

## Batch 3 (delegated rules, wiring)
| SUP-002, SEC-006 | osv-scanner: finding at the lockfile; SEC-006 narrowed to CVSS >= 7 or no score and says reachability is unchecked; no lockfile = gap; unreadable lockfile = error; subdir lockfile named relative; missing tool = gap, nothing installed | tests/delegated_osv.rs against the REAL pinned tool (the_claims_are_earned_..., sec_006_reports_only_the_serious_advisories..., etc.) | PROVEN (end to end; needs network/tool, skipped offline as unproven, not failed) |
| SUP-008, API-006, API-007, API-001, DOC-010, SEC-002, SEC-009, REL-005 | zizmor / buf / squawk / semver-checks / lychee / gitleaks / checkov | tests/delegated_{zizmor,buf,squawk,semver,lychee,gitleaks,checkov}.rs (a file per tool; REL-005 via checkov) | file present; per-claim read NOT done in this batch |
| SEC-001, SUP-003, RDB-001 | "source only" (no check built) | - | n/a |

## Batch 4
| MOD-009 | chain of >= 4 hops (method chain and field chain) is one Info/Low finding at the first hop; 3 hops not; a method taking an argument ends the run | probe of 4 chains on a scratch crate (f, h reported; g, i not) | PROVEN |
| EXT-007 | non-pub trait with items, exactly one impl for a package type, no test double, no #[automock]; pub trait, two impls, impl for u32, blanket impl, empty trait not; pub(crate) counts; Low/Low | probe of 11 traits on a scratch crate (Only, Crate reported; the rest not) | PROVEN |

## Batch 5
| TYP-001 | money/duration (alone) and identifier (two same-primitive ids) classes; unit-stating names, lone id, ids of different types, newtypes, Duration, private fn, impl Trait for Type, doc(hidden) not; pub trait methods read; Low/Low | probe of 17 signatures on a scratch crate (7 reported, all as documented) | PROVEN |
| MOD-001 | undocumented public items counted (/// or #[doc]; doc(hidden) left out; pub use leaf counted; pub items in a private mod not), Info/Medium below 20 items; Low from 50% of >= 20 | probe (5 of 8, Info/Medium, first at the right line) + public_surface/tests.rs::most_of_a_large_surface_undocumented_is_low_and_a_small_one_never_is | PROVEN |
