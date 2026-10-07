//! Writing your own check and running it through the host.
//!
//! ```text
//! cargo run --example custom_check
//! ```
//!
//! The check reports a `TODO.md` in the project root. Running it with [`coderipper::run_checks_with`] (rather than
//! calling `Check::run` yourself) gets you the host's validation and the project's `.coderipper.toml` allowlist.

use coderipper::check::{Check, CheckContext, Network, Scope, Tier};
use coderipper::finding::{Confidence, Finding, Location, Severity};

struct TodoFile;

impl Check for TodoFile {
    fn id(&self) -> &'static str {
        "todo-file"
    }
    fn scope(&self) -> Scope {
        Scope::Project
    }
    fn network(&self) -> Network {
        Network::LocalOnly
    }
    fn run(&self, ctx: &CheckContext) -> anyhow::Result<Vec<Finding>> {
        if !ctx.project_root.join("TODO.md").exists() {
            return Ok(Vec::new());
        }
        // A check returns RAW findings; the host does the suppression. To be suppressible a finding needs a location
        // and a subject, which together with the check id are what an allowlist entry names.
        Ok(vec![Finding::new(
            "todo-file",
            Severity::Low,
            Confidence::High,
            "demo",
            "the project keeps a TODO.md",
            "TODO.md is for the issue tracker",
        )
        .location(Location::new("TODO.md", None))
        .subject("TODO.md")])
    }
}

fn main() -> anyhow::Result<()> {
    let project = tempfile::tempdir()?;
    std::fs::write(project.path().join("TODO.md"), "- later\n")?;
    let ctx = CheckContext::new(project.path());
    let checks: Vec<Box<dyn Check>> = vec![Box::new(TodoFile)];

    let result = coderipper::run_checks_with(&checks, &ctx, Tier::Fast, None);
    for finding in &result.findings {
        println!("found: {} ({})", finding.summary, finding.check_id);
    }
    assert_eq!(result.findings.len(), 1);

    // The project can allow it, with a reason; the host then suppresses the finding.
    std::fs::write(
        project.path().join(".coderipper.toml"),
        "[[allow]]\ncheck = \"todo-file\"\nfile = \"TODO.md\"\nsymbol = \"TODO.md\"\nreason = \"kept on purpose\"\n",
    )?;
    let result = coderipper::run_checks_with(&checks, &ctx, Tier::Fast, None);
    println!(
        "after the allowlist entry: {} findings",
        result.findings.len()
    );
    assert!(result.findings.is_empty());
    Ok(())
}
