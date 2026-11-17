// SPDX-License-Identifier: MIT OR Apache-2.0
//! REL-002: a retry loop has a cap and a delay that grows.
//!
//! **What this judges (Rust, from the source alone):** every tracked `.rs` file of a package (outside vendored directories, tests,
//! examples, tools and `build.rs`, and outside `#[cfg(test)]` items) is parsed with `syn`. A *retry loop* is a `loop`, `while` or `for`
//! whose own body (not a nested loop, closure or function) both **sleeps** (`thread::sleep`, `tokio::time::sleep`, `async_std::task::sleep`,
//! or any other call whose path ends in `sleep`) and **looks at a failure** (a `match` arm `Err(..)`, an `if let Err(..)`, a
//! call of `.is_err()`). Two things are reported, one **Medium**-severity, Low-confidence finding per loop and per thing:
//!
//! * **no cap:** a `loop`, a `while` or a `for` over an endless iterator (`0..`, `repeat`, `cycle`) in which no comparison names an
//!   attempt counter or a limit (an identifier holding `attempt`, `retr`, `tries`, `try`, `count`, `max`, `limit`, `budget`, `deadline`,
//!   `elapsed` or `timeout`);
//! * **no backoff:** every sleep in the loop waits a fixed time written out in the call (`sleep(Duration::from_millis(100))`, a
//!   `SCREAMING_CASE` constant), so the delay never grows with the attempts.
//!
//! **What it cannot see:** whether the operation is safe to repeat (idempotent), jitter, a retry budget shared across callers, retries
//! stacked across layers, a retry helper of a crate (`backoff`, `tokio-retry`, `again`, which is the better form and is not read), a
//! delay held in a variable (read as growing, so a fixed delay held in `let delay = ..` is missed), and the other languages. A file
//! `syn` cannot parse is named in a note, never read as clean.

use super::rust_package::{load, unparsed_note};
use super::{RepoView, Verdict};
use crate::checks::size_coupling::is_cfg_test;
use crate::finding::{Confidence, Finding, Location, Severity};
use syn::spanned::Spanned;
use syn::visit::Visit;

const ID: &str = crate::rule_id::REL_002;

const CAP_HINTS: &[&str] = &[
    "attempt", "retr", "tries", "try", "count", "max", "limit", "budget", "deadline", "elapsed",
    "timeout",
];

/// What one loop does in its own body.
#[derive(Default)]
struct Facts {
    sleeps: Vec<bool>,
    failure_seen: bool,
    cap_seen: bool,
}

fn hints_cap(name: &str) -> bool {
    let n = name.to_lowercase();
    CAP_HINTS.iter().any(|h| n.contains(h))
}

fn mentions_cap(e: &syn::Expr) -> bool {
    struct M(bool);
    impl<'ast> Visit<'ast> for M {
        fn visit_ident(&mut self, i: &'ast syn::Ident) {
            if hints_cap(&i.to_string()) {
                self.0 = true;
            }
        }
    }
    let mut m = M(false);
    m.visit_expr(e);
    m.0
}

/// A sleep whose time is written out: only literals and `SCREAMING_CASE` names, and calls and paths that join them.
fn is_fixed(e: &syn::Expr) -> bool {
    match e {
        syn::Expr::Lit(_) => true,
        syn::Expr::Paren(p) => is_fixed(&p.expr),
        syn::Expr::Group(g) => is_fixed(&g.expr),
        syn::Expr::Path(p) => p.path.segments.last().is_some_and(|s| {
            let n = s.ident.to_string();
            n.chars().any(char::is_uppercase) && !n.chars().any(char::is_lowercase)
        }),
        // a constructor written out (`Duration::from_millis(100)`, `time::Duration::from_secs(1)`): a path whose second-to-last
        // segment is a type name. A call of a function (`delay()`, `backoff(n)`) computes its time and is not read as fixed.
        syn::Expr::Call(c) => {
            let syn::Expr::Path(p) = &*c.func else {
                return false;
            };
            let segs: Vec<String> = p
                .path
                .segments
                .iter()
                .map(|s| s.ident.to_string())
                .collect();
            segs.len() >= 2
                && segs[segs.len() - 2].starts_with(|c: char| c.is_ascii_uppercase())
                && c.args.iter().all(is_fixed)
        }
        syn::Expr::MethodCall(m) => {
            // `Duration::from_millis(100).max(..)` style chains: fixed when the receiver and the arguments are
            is_fixed(&m.receiver) && m.args.iter().all(is_fixed)
        }
        syn::Expr::Binary(b) => is_fixed(&b.left) && is_fixed(&b.right),
        _ => false,
    }
}

fn is_failure_pat(p: &syn::Pat) -> bool {
    match p {
        syn::Pat::TupleStruct(t) => t.path.is_ident("Err"),
        syn::Pat::Or(o) => o.cases.iter().any(is_failure_pat),
        _ => false,
    }
}

impl<'ast> Visit<'ast> for Facts {
    // A nested loop, closure or function is another unit: its sleep and its cap are its own.
    fn visit_expr_loop(&mut self, _: &'ast syn::ExprLoop) {}
    fn visit_expr_while(&mut self, _: &'ast syn::ExprWhile) {}
    fn visit_expr_for_loop(&mut self, _: &'ast syn::ExprForLoop) {}
    fn visit_expr_closure(&mut self, _: &'ast syn::ExprClosure) {}
    fn visit_item(&mut self, _: &'ast syn::Item) {}

    fn visit_expr_call(&mut self, c: &'ast syn::ExprCall) {
        if let syn::Expr::Path(p) = &*c.func {
            if p.path.segments.last().is_some_and(|s| s.ident == "sleep") {
                self.sleeps.push(c.args.iter().all(is_fixed));
            }
        }
        syn::visit::visit_expr_call(self, c);
    }

    fn visit_expr_method_call(&mut self, m: &'ast syn::ExprMethodCall) {
        if m.method == "is_err" {
            self.failure_seen = true;
        }
        syn::visit::visit_expr_method_call(self, m);
    }

    fn visit_arm(&mut self, a: &'ast syn::Arm) {
        if is_failure_pat(&a.pat) {
            self.failure_seen = true;
        }
        syn::visit::visit_arm(self, a);
    }

    fn visit_expr_let(&mut self, l: &'ast syn::ExprLet) {
        if is_failure_pat(&l.pat) {
            self.failure_seen = true;
        }
        syn::visit::visit_expr_let(self, l);
    }

    fn visit_expr_binary(&mut self, b: &'ast syn::ExprBinary) {
        use syn::BinOp::{Eq, Ge, Gt, Le, Lt};
        if matches!(b.op, Lt(_) | Le(_) | Gt(_) | Ge(_) | Eq(_))
            && (mentions_cap(&b.left) || mentions_cap(&b.right))
        {
            self.cap_seen = true;
        }
        syn::visit::visit_expr_binary(self, b);
    }
}

struct Site {
    line: u32,
    function: String,
    loop_kind: &'static str,
    uncapped: bool,
    fixed_delay: bool,
}

struct Walk {
    functions: Vec<String>,
    sites: Vec<Site>,
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs
        .iter()
        .any(|a| a.path().segments.last().is_some_and(|s| s.ident == "test"))
}

fn endless(e: &syn::Expr) -> bool {
    match e {
        syn::Expr::Range(r) => r.end.is_none(),
        syn::Expr::Paren(p) => endless(&p.expr),
        syn::Expr::Call(c) => matches!(&*c.func, syn::Expr::Path(p)
            if p.path.segments.last().is_some_and(|s| s.ident == "repeat" || s.ident == "repeat_with")),
        syn::Expr::MethodCall(m) => m.method == "cycle" || endless(&m.receiver),
        _ => false,
    }
}

impl Walk {
    fn judge_loop(
        &mut self,
        span: proc_macro2::Span,
        kind: &'static str,
        body: &syn::Block,
        cond: Option<&syn::Expr>,
        bounded_by_construction: bool,
    ) {
        let mut facts = Facts::default();
        facts.visit_block(body);
        if let Some(c) = cond {
            facts.visit_expr(c);
            if mentions_cap(c) {
                facts.cap_seen = true;
            }
        }
        if facts.sleeps.is_empty() || !facts.failure_seen {
            return;
        }
        let uncapped = !bounded_by_construction && !facts.cap_seen;
        let fixed_delay = facts.sleeps.iter().all(|f| *f);
        if !(uncapped || fixed_delay) {
            return;
        }
        self.sites.push(Site {
            line: u32::try_from(span.start().line).unwrap_or(0),
            function: self
                .functions
                .last()
                .cloned()
                .unwrap_or_else(|| "<module>".into()),
            loop_kind: kind,
            uncapped,
            fixed_delay,
        });
    }
}

impl<'ast> Visit<'ast> for Walk {
    fn visit_item_mod(&mut self, i: &'ast syn::ItemMod) {
        if is_cfg_test(&i.attrs) {
            return;
        }
        syn::visit::visit_item_mod(self, i);
    }

    fn visit_item_fn(&mut self, i: &'ast syn::ItemFn) {
        if is_cfg_test(&i.attrs) || has_test_attr(&i.attrs) {
            return;
        }
        self.functions.push(i.sig.ident.to_string());
        syn::visit::visit_item_fn(self, i);
        self.functions.pop();
    }

    fn visit_impl_item_fn(&mut self, i: &'ast syn::ImplItemFn) {
        if is_cfg_test(&i.attrs) || has_test_attr(&i.attrs) {
            return;
        }
        self.functions.push(i.sig.ident.to_string());
        syn::visit::visit_impl_item_fn(self, i);
        self.functions.pop();
    }

    fn visit_expr_loop(&mut self, l: &'ast syn::ExprLoop) {
        self.judge_loop(l.span(), "loop", &l.body, None, false);
        syn::visit::visit_expr_loop(self, l);
    }

    fn visit_expr_while(&mut self, w: &'ast syn::ExprWhile) {
        self.judge_loop(w.span(), "while", &w.body, Some(&w.cond), false);
        syn::visit::visit_expr_while(self, w);
    }

    fn visit_expr_for_loop(&mut self, f: &'ast syn::ExprForLoop) {
        self.judge_loop(f.span(), "for", &f.body, None, !endless(&f.expr));
        syn::visit::visit_expr_for_loop(self, f);
    }
}

pub(super) fn judge(view: &RepoView) -> Verdict {
    let loaded = load(view);
    if loaded.packages.is_empty() {
        return super::rust_package::nothing_to_read(ID, &loaded);
    }
    let mut findings = Vec::new();
    for p in &loaded.packages {
        let files = p.sources.len();
        for s in &p.sources {
            let mut walk = Walk {
                functions: Vec::new(),
                sites: Vec::new(),
            };
            walk.visit_file(&s.file);
            for site in walk.sites {
                let mut what = Vec::new();
                if site.uncapped {
                    what.push("has no cap on its attempts");
                }
                if site.fixed_delay {
                    what.push("waits the same fixed time every attempt (no backoff)");
                }
                findings.push(
                    Finding::new(
                        ID,
                        Severity::Medium,
                        Confidence::Low,
                        view.name(),
                        format!(
                            "the retry `{}` loop in `{}` {}",
                            site.loop_kind,
                            site.function,
                            what.join(" and ")
                        ),
                        "A retry with no cap never gives up, and one with a fixed delay keeps hitting a struggling dependency at full rate. Cap the attempts, grow the delay exponentially with jitter, and retry only operations that are safe to repeat; or use a retry crate that does so.",
                    )
                    .location(Location::new(s.path.as_str(), Some(site.line)))
                    .subject(format!(
                        "{}:{}:{}@{}",
                        s.path, site.function, site.loop_kind, site.line
                    ))
                    .positive_control(format!(
                        "{files} Rust source file(s) of the package were parsed with the same walk that reads loops, their sleeps and their failure arms"
                    )),
                );
            }
        }
    }
    findings.sort_by(|a, b| {
        let key = |f: &Finding| {
            f.location
                .as_ref()
                .map(|l| (l.file.clone(), l.line))
                .unwrap_or_default()
        };
        key(a).cmp(&key(b))
    });
    match unparsed_note(ID, &loaded.unparsed) {
        Some(note) => Verdict::Noted(findings, vec![note]),
        None => Verdict::Findings(findings),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MANIFEST: &str = "[package]\nname = \"p\"\nversion = \"0.1.0\"\n";

    fn found(src: &str) -> Vec<Finding> {
        match judge(&RepoView::from_files(&[
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", src),
        ])) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    #[test]
    fn an_endless_loop_with_a_fixed_sleep_is_reported_for_both_faults() {
        let src = "pub fn f() {
    loop {
        match connect() {
            Ok(c) => return c,
            Err(_) => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}
";
        let f = found(src);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].summary.contains("no cap") || f[0].summary.contains("cap"));
        assert!(f[0].summary.contains("no backoff"), "{}", f[0].summary);
    }

    #[test]
    fn a_capped_loop_with_a_growing_delay_is_not_reported() {
        let src = "pub fn f() {
    let mut attempt = 0;
    let mut delay = Duration::from_millis(100);
    loop {
        if let Err(e) = connect() {
            attempt += 1;
            if attempt >= MAX_TRIES { return; }
            std::thread::sleep(delay);
            delay *= 2;
        } else { return; }
    }
}
";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn a_for_loop_over_a_range_is_capped_but_a_fixed_delay_is_still_reported() {
        let src = "pub fn f() {
    for _ in 0..5 {
        if connect().is_err() { thread::sleep(Duration::from_secs(1)); }
    }
}
";
        let f = found(src);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(!f[0].summary.contains("no cap"), "{}", f[0].summary);
    }

    #[test]
    fn a_loop_that_sleeps_without_looking_at_a_failure_is_not_a_retry() {
        let src = "pub fn f() { loop { tick(); std::thread::sleep(Duration::from_secs(1)); } }\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn test_code_is_not_read() {
        let src = "#[cfg(test)]\nmod t { pub fn f() { loop { if g().is_err() { sleep(Duration::from_secs(1)); } } } }\n";
        assert!(found(src).is_empty(), "{:?}", found(src));
    }

    #[test]
    fn every_finding_validates() {
        let src = "pub fn f() { loop { if g().is_err() { sleep(Duration::from_secs(1)); } } }\n";
        for f in found(src) {
            f.validate().unwrap();
        }
    }
}

/// Claims audit (2026-11-17): one test per claim of the module documentation, the catalog `checked_today` and the changelog entry.
#[cfg(test)]
mod claims {
    use super::*;

    const MANIFEST: &str = "[package]\nname = \"p\"\nversion = \"0.1.0\"\n";

    fn found(src: &str) -> Vec<Finding> {
        match judge(&RepoView::from_files(&[
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", src),
        ])) {
            Verdict::Findings(f) | Verdict::Noted(f, _) => f,
            Verdict::NotApplicable(r) => panic!("not applicable: {r}"),
        }
    }

    #[test]
    fn a_function_call_is_a_computed_delay_and_a_screaming_constant_is_a_fixed_one() {
        // `delay()` computes its time: with a cap in the condition nothing is reported
        let computed = "pub fn f() { let mut attempts = 0; while attempts < 5 { if g().is_err() { attempts += 1; sleep(delay()); } } }";
        assert!(found(computed).is_empty(), "{:?}", found(computed));
        let constant = "pub fn f() { for _ in 0..3 { if g().is_err() { sleep(RETRY_DELAY); } } }";
        let f = found(constant);
        assert_eq!(f.len(), 1, "{f:?}");
        assert!(f[0].summary.contains("no backoff"), "{}", f[0].summary);
    }

    #[test]
    fn a_while_condition_that_names_a_counter_is_a_cap() {
        let src = "pub fn f() { let mut attempts = 0; while attempts < 5 { if g().is_err() { attempts += 1; sleep(grow(attempts)); } } }";
        assert!(found(src).is_empty(), "{:?}", found(src));
        let no_cap = "pub fn f() { while !done() { if g().is_err() { sleep(grow(1)); } } }";
        assert_eq!(found(no_cap).len(), 1, "a while with no counter has no cap");
    }

    #[test]
    fn a_for_over_an_endless_iterator_is_uncapped() {
        for head in ["0..", "std::iter::repeat(1)", "v.iter().cycle()"] {
            let src = format!(
                "pub fn f() {{ for _ in {head} {{ if g().is_err() {{ sleep(grow()); }} }} }}"
            );
            let f = found(&src);
            assert_eq!(f.len(), 1, "{head}: {f:?}");
            assert!(f[0].summary.contains("no cap"), "{head}: {}", f[0].summary);
        }
        // positive control: the same body over a bounded range is capped
        assert!(
            found("pub fn f() { for i in 0..5 { if g().is_err() { sleep(grow(i)); } } }")
                .is_empty()
        );
    }

    #[test]
    fn a_sleep_in_a_nested_loop_closure_or_function_belongs_to_that_unit() {
        let nested =
            "pub fn f() { loop { for i in 0..3 { if g().is_err() { sleep(grow(i)); } } } }";
        assert!(found(nested).is_empty(), "{:?}", found(nested));
        let closure =
            "pub fn f() { loop { let c = || { sleep(grow()); }; if g().is_err() { c(); } } }";
        assert!(found(closure).is_empty(), "{:?}", found(closure));
    }

    #[test]
    fn the_three_failure_forms_and_a_path_ending_in_sleep_make_a_retry() {
        for failure in [
            "match g() { Ok(_) => {}, Err(_) => sleep(FIXED) }",
            "if let Err(_) = g() { sleep(FIXED); }",
            "if g().is_err() { sleep(FIXED); }",
        ] {
            let src = format!("pub fn f() {{ loop {{ {failure} }} }}");
            assert_eq!(found(&src).len(), 1, "{failure}");
        }
        for sleep in [
            "std::thread::sleep(FIXED)",
            "tokio::time::sleep(FIXED).await",
            "async_std::task::sleep(FIXED).await",
        ] {
            let src = format!("pub async fn f() {{ loop {{ if g().is_err() {{ {sleep}; }} }} }}");
            assert_eq!(found(&src).len(), 1, "{sleep}");
        }
    }

    #[test]
    fn a_delay_held_in_a_variable_is_read_as_growing_which_the_documentation_says() {
        let src = "pub fn f() { let d = Duration::from_millis(5); for _ in 0..3 { if g().is_err() { sleep(d); } } }";
        assert!(
            found(src).is_empty(),
            "documented limitation: {:?}",
            found(src)
        );
    }

    #[test]
    fn a_finding_is_medium_severity_low_confidence_one_per_loop_with_both_faults_in_it() {
        let f = found("pub fn f() { loop { if g().is_err() { sleep(Duration::from_secs(1)); } } }");
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].severity, Severity::Medium);
        assert_eq!(f[0].confidence, Confidence::Low);
        assert!(
            f[0].summary.contains("no cap") && f[0].summary.contains("no backoff"),
            "{}",
            f[0].summary
        );
    }
}
