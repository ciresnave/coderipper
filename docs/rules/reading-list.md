# Reading list

Informed by the owner's knowledge base v2 (October 2026); paraphrased, not quoted.

Works behind the catalog, grouped by theme, with a note on why each matters. Years are first publication where known.
Check details against the original before citing formally.

## Decomposition and architecture

- Parnas (1972), *On the Criteria To Be Used in Decomposing Systems into Modules*: split by what is likely to change, not by processing steps.
- Dijkstra (1974), *On the role of scientific thought*: the origin of separation of concerns.
- Stevens, Myers and Constantine (1974), *Structured Design*: the original coupling and cohesion vocabulary.
- Brooks (1986), *No Silver Bullet*: essential complexity cannot be tooled away.
- Lakos (1996), *Large-Scale C++ Software Design*: physical dependency structure and cycles at scale.
- Martin (2003), *Agile Software Development: Principles, Patterns, and Practices*: the package principles behind stability metrics.
- Evans (2003), *Domain-Driven Design*: bounded contexts as a way to draw boundaries.
- Cockburn (2005), *Hexagonal Architecture*, and Palermo (2008), *Onion Architecture*: two statements of the inward dependency rule.
- Martin (2012/2017), *Clean Architecture*: one popular style, treated here as a choice, not a mandate.
- Lewis and Fowler (2014), *Microservices*: the case, and the costs, of fine-grained deployment.
- Conway (1968), *How Do Committees Invent?*: structure of the system follows structure of the organization.
- MacCormack, Rusnak and Baldwin (2006), *Exploring the Structure of Complex Software Designs*: measuring modularity from dependency structure.
- Ford, Parsons and Kua (2017), *Building Evolutionary Architectures*: fitness functions as automated architecture checks.

## Design and extensibility

- Meyer (1988), *Object-Oriented Software Construction*: the open-closed principle, design by contract, command-query separation.
- Liskov and Wing (1994), *A Behavioral Notion of Subtyping*: what substitutability actually requires.
- Gamma, Helm, Johnson and Vlissides (1994), *Design Patterns*: the shared vocabulary of recurring designs.
- Wadler (1998), *The Expression Problem*: why adding variants and adding operations pull in opposite directions.
- Bloch, *Effective Java* and *How to Design a Good API*: concrete advice on public surfaces.
- Metz (2014), *The Wrong Abstraction*: duplication can cost less than a bad abstraction.
- Ousterhout (2018), *A Philosophy of Software Design*: deep modules and the cost of splitting too far.
- Hickey (2011), *Simple Made Easy*: separating simple (unentangled) from merely familiar.

## Readability and complexity

- McCabe (1976), *A Complexity Measure*: the cyclomatic count and its limits.
- Chidamber and Kemerer (1994), *A Metrics Suite for Object Oriented Design*: the source of several class-level metrics.
- Fowler (1999; 2018), *Refactoring*: a catalog of behavior-preserving changes and the smells that suggest them.
- Hunt and Thomas (1999), *The Pragmatic Programmer*: the origin of the "single representation of knowledge" idea.
- McConnell, *Code Complete*: broad, evidence-aware construction practice.
- Campbell (2016-17), *Cognitive Complexity*: a white paper defining a readability-oriented count.
- Munoz Baron, Wyrich and Wagner (2020), *An Empirical Validation of Cognitive Complexity as a Measure of Source Code Understandability*: tests whether that count tracks comprehension.
- Herraiz and Hassan (2010), *Beyond Lines of Code: Do We Need More Complexity Metrics?*: much of metric value is just size.

## Types, errors and correctness

- Minsky, *Effective ML*: the habit of making illegal states unrepresentable.
- King (2019), *Parse, Don't Validate*: turn checked input into a type that carries the proof.
- Strom and Yemini (1986), *Typestate*: types that change with an object's protocol state.
- Bernhardt (2012), *Boundaries*: a pure core inside an effectful shell.
- Shore (2004), *Fail Fast*: the original argument for surfacing faults early.
- Armstrong (2003), *Making Reliable Distributed Systems in the Presence of Software Errors*: supervision and let-it-crash design.
- Claessen and Hughes (2000), *QuickCheck*: the origin of property-based testing.

## Testing

- Beck (2002), *Test-Driven Development*: the test-first workflow.
- Meszaros (2007), *xUnit Test Patterns*: names and smells for test code.
- Fowler (2007), *Mocks Aren't Stubs*: the two schools of test doubles.
- Inozemtseva and Holmes (2014), *Coverage Is Not Strongly Correlated with Test Suite Effectiveness*: why coverage alone is a weak target.
- Just et al. (2014), *Are Mutants a Valid Substitute for Real Faults?*: support for mutation score as a signal.
- Luo et al. (2014), *An Empirical Analysis of Flaky Tests*: what makes tests nondeterministic.

## Reliability, concurrency and operations

- Nygard (2007; 2018), *Release It!*: stability patterns such as timeouts and circuit breakers.
- Beyer et al. (2016), *Site Reliability Engineering*: service objectives and operational practice.
- Dean and Barroso (2013), *The Tail at Scale*: why latency outliers dominate large systems.
- Garcia-Molina and Salem (1987), *Sagas*: long transactions as compensated steps.
- Goetz et al. (2006), *Java Concurrency in Practice*: the standard treatment of shared-state hazards.
- Smith (2018), *Notes on Structured Concurrency*: tasks scoped like blocks.
- Brooker (2015), *Exponential Backoff and Jitter*: how randomized delay prevents retry storms.
- Wiggins (2011), *The Twelve-Factor App*: conventions for configuration and deployable services.

## Security and supply chain

- Saltzer and Schroeder (1975), *The Protection of Information in Computer Systems*: enduring secure-design principles.
- OWASP ASVS, Top 10 and SAMM: verification requirements, common weaknesses and a maturity model.
- NIST SP 800-218 (SSDF): a secure development framework.
- SLSA, Sigstore, SPDX, CycloneDX and OpenSSF Scorecard: build provenance, signing, bills of materials and project health signals.
- Pearce et al. (2021-22), *Asleep at the Keyboard?*: security of code written by an assistant.

## History and evolution

- Lehman (1980), *Programs, Life Cycles, and Laws of Software Evolution*: why systems keep growing and decaying.
- Gall, Hajek and Jazayeri (1998), *Detection of Logical Coupling Based on Product Release History*: coupling revealed by shared change.
- Zimmermann et al. (2004), *Mining Version Histories to Guide Software Changes*: co-change as a prediction.
- Nagappan and Ball (2005), *Use of Relative Code Churn Measures to Predict System Defect Density*: relative churn as a defect signal.
- Bird et al. (2011), *Don't Touch My Code!*: ownership and defects.
- Tornhill (2015), *Your Code as a Crime Scene*: hotspot analysis from version control.

## Performance and process

- Knuth (1974), *Structured Programming with go to Statements*: the source of the premature-optimization caution.
- Pike (1989), *Notes on Programming in C*: measure before tuning; prefer plain code.
- Thompson (2011), *Mechanical Sympathy*: write with the hardware in mind.
- Acton (2014), *Data-Oriented Design and C++*: a case for layout-driven design.
- Muratori (2023), *"Clean" Code, Horrible Performance*: the cost of heavy abstraction in hot code.
- Winters, Manshreck and Wright (2020), *Software Engineering at Google*: practice at large scale.
- Nygard (2011), *Documenting Architecture Decisions*: short decision records.
- Procida, *Diataxis*: four kinds of documentation and how to keep them apart.
- Sato (2014), *Parallel Change*: staged, compatible migration of an interface.

## Also named in the catalog

- Postel's law and its critics, the 2014 test-first debate, and the "wrong abstraction" argument appear in the register of
  contested guidance ([cross-cutting.md](cross-cutting.md)).
