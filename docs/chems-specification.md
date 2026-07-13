# `.chems` language specification

## Status and authority

This document defines the intended end-state of `.chems`. It is the design
authority for future language work.

The normative grammar is [`grammar/chems.ebnf`](../grammar/chems.ebnf). There is
no legacy language contract or compatibility surface; implementation is built
directly against this specification.

Specification chapters are reviewed and locked before implementation
resumes. A locked chapter may change only through an explicit language-design
decision that records its compatibility impact.

Current chapter status:

| Chapter | Status |
| --- | --- |
| Language charter and authority boundaries | Locked |
| Source and semantic entity model | Locked |
| Lexical structure, grammar, names, scopes, and files | Locked |
| Quantities, dimensions, formulae, species, and materials | Locked |
| Vessels, conditions, procedures, stages, and transitions | Locked |
| Claims, holes, goals, assumptions, and proofs | Locked |
| Kernel judgments, tactics, derivations, and result states | Locked |
| Catalogue resolution, provenance, diagnostics, and formatting | Locked |
| IR schemas, compatibility, conformance, and implementation slices | Locked |

## Language charter

`.chems` is a versioned, total, decidable language for specifying bounded
virtual chemistry experiments, expressing claims about their outcomes, and
proving those claims against a trusted empirical catalogue.

A successfully checked program elaborates into an immutable, proof-carrying
`ValidatedExperiment`. Humans and agents author the same source language.
Partial programs are valid drafting artifacts, but they cannot produce a
validated result or drive a simulation.

The language is designed to support three educational activities without
separate execution modes:

- **Predict:** leave a typed hole and ask the trusted engine to derive it.
- **Check:** write an expected result and ask the engine to prove or reject it.
- **Explain:** inspect the proof goals, premises, transformations, assumptions,
  and final derivation.

## Authority boundaries

The language has five distinct authorities:

| Authority | Decides |
| --- | --- |
| Source author | Experimental inputs, procedure, claims, requested derivations, and explicit assumptions |
| Language elaborator | Syntax meaning, names, types, dimensions, scopes, and construction of proof goals |
| Versioned catalogue | Empirical premises such as identity, dissociation, solubility, observable properties, and supported conditions |
| Trusted kernel | Whether every required judgment follows from inputs, catalogue premises, rules, and permitted assumptions |
| Application | Editing, workflow, explanations, and presentation of validated states |

The agent has no special authority. It is one possible source author. Its text
is accepted, rejected, or left incomplete under the same rules as human text.

The simulation has no chemical authority. It consumes the state timeline in a
`ValidatedExperiment` and cannot infer, amend, or extend it.

## Non-goals

`.chems` is not:

- a general-purpose programming language;
- a language for arbitrary computation, loops, branches, recursion, or I/O;
- a mechanism for declaring new empirical facts or editing the catalogue;
- a container for URLs, research prose, prompts, or model instructions;
- a renderer, particle, animation, camera, or shader scripting language;
- a real-world laboratory automation or equipment-control language;
- an unrestricted procedural guide for physical experimentation;
- a molecular-dynamics, kinetics, or equilibrium solver specification;
- a plugin or user-defined tactic system.

These exclusions keep execution terminating, proof review tractable, agent
output constrained, and the trust boundary visible.

## Compilation and validation model

The complete pipeline is:

```text
UTF-8 source
    -> lossless concrete syntax tree
    -> source AST
    -> name and catalogue resolution
    -> dimensionally typed HIR
    -> experiment state-transition IR
    -> claims and proof goals
    -> closed tactic evaluation
    -> kernel-checked derivation
    -> ValidatedExperiment
```

Each arrow is an explicit fallible boundary with stable diagnostics. Later
stages never reinterpret source text directly.

The compiler may preserve useful intermediate representations for the editor,
agent repair loop, explanations, and tests. Only the kernel may construct the
validated artifact accepted by simulation.

## Source unit

A `.chems` source file represents exactly one experiment.

Every file contains, in semantic order:

1. a required language-version declaration;
2. one required catalogue-bundle selection;
3. one named experiment;
4. its environmental conditions;
5. zero or more explicit assumptions;
6. its initial materials;
7. its virtual vessels;
8. an ordered procedure;
9. zero or more outcome claims or requested derivations;
10. a proof script.

The exact surface grammar and concise forms are specified by the lexical and
grammar chapter below. This section fixes the entities and their relationships.

One-experiment-per-file is deliberate:

- the file remains the independently editable source of one simulation;
- validation, caching, provenance, and stale-result detection use one digest;
- agent patches have a narrow ownership boundary;
- classroom collections can group files without adding language modules;
- imports and cross-file evaluation cannot undermine reproducibility.

Lesson packs, fixture collections, and application projects are containers of
`.chems` files, not constructs inside the language.

## Source entity model

```text
Document
├── LanguageVersion
├── CatalogueSelection
└── Experiment
    ├── ExperimentName
    ├── Conditions
    ├── Assumptions[]
    ├── MaterialDeclaration[]
    ├── VesselDeclaration[]
    ├── Procedure
    │   └── Operation[]
    ├── Expectation
    │   └── Claim[]
    └── Proof
        └── Tactic[]
```

Every source entity retains its exact byte span and concrete syntax origin.
Names, syntactic sugar, comments, and holes remain traceable through
elaboration so diagnostics and derivation steps can refer back to authored
source.

### Language version

The source explicitly selects its `.chems` language major version. A compiler
does not guess syntax or semantics from file contents.

The language version is distinct from the catalogue version. Updating facts
does not change grammar; updating grammar does not silently select new facts.

### Catalogue selection

A source file selects exactly one immutable catalogue bundle by stable name and
version. Resolution records the bundle's content digest as well as its declared
version.

A catalogue bundle may internally compose multiple reviewed datasets, but
source ordering cannot be used to override facts. Conflicting catalogue facts
make the bundle invalid before a `.chems` program is evaluated.

The source may refer to catalogue entities and assumption kinds. It may not
declare or patch catalogue facts.

### Experiment

The experiment is the root semantic scope. It owns every name and declaration
used by validation.

Its identity is the pair of its source digest and declared experiment name.
Renaming or editing an experiment invalidates any previously validated
artifact.

### Conditions

Conditions describe the initial environmental context in which catalogue facts
and reaction rules are interpreted. They are typed values rather than free-form
metadata.

The semantic model includes at least:

- temperature;
- pressure;
- chemical medium or solvent context;
- whether the environment is open or closed where this is globally known.

Conditions may later change through explicit procedure operations. There are
no invisible condition changes.

### Assumptions

An assumption is an explicit request to admit one catalogue-defined premise
that cannot otherwise be established from the experiment.

Assumptions cannot contain arbitrary propositions. Each assumption resolves to
a stable, typed assumption kind understood by the kernel and states the source
entities or stage to which it applies.

Every used assumption:

- remains visible in the source;
- becomes a premise node in the derivation;
- appears prominently in the validated artifact and application;
- changes the outcome state from `Validated` to
  `ValidatedWithAssumptions`.

Unused assumptions are diagnostics. Hidden default assumptions are forbidden.

### Material declarations

A material declaration introduces a finite initial inventory available to the
procedure. A material is not merely a formula.

The source model supports three material forms:

- **Sample:** an amount or mass of a resolved species or catalogued substance.
- **Solution:** a solvent context plus one or more dissolved components with
  exact concentration or amount and a total volume.
- **Prepared composition:** explicit quantities of resolved components forming
  one initial material.

Concise chemistry-native syntax may elaborate into these forms. For example,
`50 mL of 0.100 mol/L AgNO3(aq)` elaborates into a solution rather than being
stored as an opaque phrase.

A material declaration states an experimental input. It does not assert facts
about dissociation, solubility, hazards, colour, or likely reactions.

Each material has a linear inventory identity. Procedure operations may move,
combine, divide, or consume that inventory, but cannot duplicate it.

### Vessel declarations

A vessel is a logical location in the virtual experiment. Vessel state matters
only where it affects chemical interpretation or safe presentation.

A vessel declaration may contain:

- a stable local name;
- capacity;
- open or closed state;
- initial temperature or pressure when different from global conditions;
- a catalogue-defined or language-defined vessel kind where chemically
  relevant.

Shape, screen position, colour, camera framing, and renderer configuration do
not belong in `.chems`.

Surface syntax may provide an anonymous default vessel for the simplest
two-material experiment, but elaboration always produces an explicit vessel.

### Procedure

A procedure is a finite ordered list of typed operations. It describes virtual
experimental state transitions, not implementation instructions for the
simulation.

The intended closed operation families are:

| Family | Operations |
| --- | --- |
| Placement | place, add, combine, transfer |
| Mixing | stir |
| Conditions | heat, cool, wait, seal, open |
| Separation | filter, decant |

Exact operations and their operands are fixed by the procedure chapter. The
language has no loops, conditional branches, concurrency, callbacks, or
user-defined operations.

Every operation consumes one `Stage` and produces the next. Operations can
move inventories or change declared conditions. Chemical transformations are
not authored as procedure operations: the kernel derives them when a stage
creates a supported reaction opportunity.

### Expectation and claims

The expectation section contains propositions about named stages or the final
stage. A claim does not become true because it appears in source.

The intended closed claim families are:

| Family | Examples of meaning |
| --- | --- |
| Classification | reaction class or no net reaction |
| Identity | species produced, consumed, remaining, or acting as spectator |
| Equation | molecular, complete ionic, net ionic |
| Quantity | produced amount, remaining amount, reaction extent, limiting reagent |
| Phase and state | precipitate, gas, dissolved species, vessel contents |
| Observation | colour, gas evolution, precipitate, supported temperature direction |

Every claim targets a stage. Omitting a stage selector means the final stage.

A claim value has exactly three source states:

1. **Explicit value:** the author asserts a result; elaboration creates a goal
   to prove equality with the derived result.
2. **Typed hole:** the author requests that value; elaboration creates a goal
   to derive and expose it.
3. **Omitted claim:** the author makes no claim about that field.

Omission is not a hole and is not an assertion of absence. The trusted engine
still derives every result required for simulation and conservation checks.

### Proof

The proof is a finite sequence of tactics from a closed language-defined
vocabulary. Tactics transform proof goals; they do not mutate experiment state
or empirical catalogue data.

The proof section may ask the engine to:

- resolve supported dissociation;
- infer products from a named supported rule family;
- normalize or balance equations;
- establish atom and charge conservation;
- derive ionic equations and eliminate spectators;
- solve supported stoichiometry;
- discharge observation or phase claims from catalogue premises;
- close all remaining goals using a bounded deterministic strategy.

A proof script can fail, leave goals open, use an inapplicable tactic, or reach
an unsupported chemistry boundary. It cannot weaken the kernel's judgments.

## Semantic entity model

The typed semantic model deliberately separates concepts that are often
conflated in informal chemistry notation.

| Entity | Meaning | Not equivalent to |
| --- | --- | --- |
| `Element` | Resolved periodic-table identity | Its one- or two-letter spelling |
| `Formula` | Normalized element/group composition | A catalogued substance |
| `Species` | Formula plus charge, phase, and resolved identity where available | A finite quantity |
| `Substance` | Catalogue identity with empirical properties and aliases | Formula alone |
| `Quantity` | Exact numeric value with dimension and unit provenance | Floating-point display text |
| `Material` | Finite component inventory under conditions | Species or substance |
| `Vessel` | Logical location containing material state | A rendered container |
| `Stage` | Immutable experiment snapshot at one procedure boundary | Animation frame |
| `Operation` | Typed transition request between stages | Chemical reaction rule |
| `Claim` | Proposition over a stage or derivation | Trusted result |
| `Hole` | Typed request for a value or proof | Wildcard or ignored field |
| `Goal` | Kernel judgment still requiring derivation | Diagnostic message |
| `Premise` | Catalogue fact, input fact, rule, or explicit assumption used by proof | Agent research prose |
| `Derivation` | Checkable DAG of premises and judgments | Tactic log alone |
| `ValidatedExperiment` | Immutable checked timeline and derivation | Parsed or elaborated source |

### Identity and equality

Different equality relations serve different layers:

- source equality compares authored syntax where needed by editing;
- formula equality compares normalized elemental composition;
- species equality compares normalized formula, charge, phase, and resolved
  identity;
- quantity equality compares exact values after dimensional conversion;
- material equality compares finite normalized inventories and conditions;
- stage equality compares complete semantic state, not presentation;
- validated-artifact equality also includes language version, catalogue digest,
  assumptions, and derivation identity.

The following quantitative, state, and kernel chapters define these relations
formally. Implementations must not use display strings as semantic identities.

### State and stages

`Stage[0]` is constructed from declared materials, vessels, and initial
conditions before procedure operations run.

For each operation `op[n]`:

```text
Stage[n]
    -> type-check operation operands
    -> apply explicit inventory/location/condition transition
    -> identify supported reaction opportunities
    -> derive maximum supported chemical transformation
    -> establish invariants and claims for this boundary
    -> Stage[n + 1]
```

One procedure stage may produce multiple explanatory simulation phases, but an
animation frame cannot become a new semantic stage.

Every stage preserves:

- elemental inventory, except for explicitly unsupported domains where no
  validated stage is produced;
- total charge under the supported model;
- non-negative material quantities;
- linear ownership of every finite input inventory;
- explicit vessel location for every remaining material;
- a trace to the operation and premises that produced it.

### Reaction opportunities

A reaction opportunity is a kernel-internal question created when compatible
materials become co-located under known conditions. It is not source syntax.

The engine may conclude that the opportunity is:

- a supported reaction with a derived extent;
- a supported no-net-reaction result;
- dependent on one or more explicit permitted assumptions;
- unsupported by the selected catalogue and kernel;
- invalid because required invariants or authored claims fail.

The absence of a supported reaction rule is never interpreted as proof that no
reaction occurs.

## Elaboration products

Elaboration produces a typed HIR containing at least:

```text
TypedExperiment
  language version
  catalogue name, version, and digest
  experiment identity
  normalized conditions
  typed explicit assumptions
  resolved initial material inventories
  explicit logical vessels
  typed finite procedure
  stage-targeted claims
  typed holes
  tactic program
  source-origin map
```

The HIR may contain holes and unresolved proof goals, but no unresolved names,
units, dimensions, formula syntax, catalogue references, or operation operands.

Failure to resolve a language name, unit, dimension, element symbol, operation
operand, or explicit qualified catalogue reference is an elaboration error. A
well-formed species whose elements resolve but which is outside the selected
catalogue's supported substance set instead produces `Unsupported`. The
quantitative and chemical type chapter defines this boundary precisely.

## Validated artifact requirements

A `ValidatedExperiment` contains at least:

```text
ValidatedExperiment
  source digest and experiment identity
  language version
  catalogue identity and content digest
  normalized initial conditions and inventories
  typed procedure operations
  complete semantic stage timeline
  supported reaction classification at each applicable stage
  normalized equations and reaction extents
  consumed, produced, remaining, and spectator inventories
  evaluated authored claims
  filled requested holes
  explicit assumptions actually used
  proof goals and their discharged judgments
  checkable derivation DAG
  source-origin and premise-provenance maps
```

It is impossible for ordinary parser, agent, application, or simulation code to
construct this type through public fields.

## Result-state model

The language pipeline distinguishes:

| State | Meaning |
| --- | --- |
| `Malformed` | A complete source AST could not be produced |
| `IllTyped` | Names, dimensions, operations, or catalogue references did not elaborate |
| `Incomplete` | Source is meaningful but contains holes or open proof goals |
| `Invalid` | An authored claim is disproved or a required invariant fails |
| `Unsupported` | The program is well-formed, but the trusted catalogue/kernel cannot decide required chemistry |
| `ValidatedWithAssumptions` | All goals close using one or more explicit permitted assumptions |
| `Validated` | All goals close without admitted assumptions |

These states are mutually exclusive final outcomes of a particular source and
catalogue digest. Provider or network failure is an application workflow state,
not a language result.

Only the final two states may produce a simulation-capable artifact.

## Illustrative source shape

This example demonstrates the entity model. The canonical complete source and
normative productions appear in the lexical and grammar chapter below.

```chems
chems 1
use catalog ChemSpec.Aqueous@1

experiment SilverChloridePrecipitation where
  conditions
    temperature := 25 degC
    pressure := 1 atm
    medium := aqueous

  given
    silverNitrate := 50 mL of 0.100 mol/L AgNO3(aq)
    sodiumChloride := 50 mL of 0.100 mol/L NaCl(aq)

  vessels
    reaction := open vessel 250 mL

  procedure
    add silverNitrate to reaction
    add sodiumChloride to reaction
    stir reaction

  expect at final
    class := precipitation
    molecular := ?
    completeIonic := ?
    netIonic := ?
    amount AgCl(s) := ?

    observe
      precipitate AgCl(s)
      colour := white

  by
    dissociate aqueous
    infer products using solubilityRules
    balance molecular
    derive completeIonic
    cancel spectators
    solve stoichiometry
```

## Decisions fixed by this chapter

This chapter fixes the following design decisions:

- `.chems` is a declarative, terminating experiment-and-proof language.
- One source file owns exactly one experiment.
- Language version and catalogue identity are explicit and independent.
- Source cannot declare empirical catalogue facts.
- The semantic model includes materials, logical vessels, finite procedures,
  immutable stages, claims, holes, goals, tactics, and derivations.
- Procedures contain bounded state transitions, not authored reaction results
  or simulation instructions.
- Explicit claim, typed hole, and omitted claim have distinct meanings.
- Assumptions are typed, explicit, catalogue-defined, traced, and result in
  `ValidatedWithAssumptions` when used.
- A proof is a closed tactic program checked by a kernel.
- Unsupported chemistry is distinct from invalid source or false claims.
- Only proof-carrying validated artifacts can drive simulation.

## Lexical structure and normative grammar

The normative grammar is
[`grammar/chems.ebnf`](../grammar/chems.ebnf). This section defines the
lexer's required behaviour, the grammar notation, name resolution at the source
level, and canonical source shape.

The grammar is normative for syntactic acceptance. Later chapters assign types
and proof meaning to the constructs it produces. A string matching the grammar
can still be ill-typed, incomplete, invalid, or unsupported.

### File representation

A `.chems` source file:

- uses the `.chems` extension;
- is UTF-8 without a byte-order mark;
- contains exactly one language header, catalogue selection, and experiment;
- accepts LF or CRLF input newlines;
- normalizes canonical output to LF;
- has a canonical final newline;
- contains no NUL code point;
- contains no tab outside a comment;
- is interpreted as Unicode only in comments; all language tokens are ASCII.

Invalid UTF-8, a byte-order mark, NUL, or a tab outside a comment is a lexical
error with an exact byte span. Implementations may enforce documented resource
limits, but limits do not change the meaning of an otherwise valid file.

### Character classes

The grammar uses these ASCII classes:

```text
UPPER          = A through Z
LOWER          = a through z
LETTER         = UPPER or LOWER
DIGIT          = 0 through 9
NON_ZERO_DIGIT = 1 through 9
```

Identifiers deliberately remain ASCII. This makes agent generation,
catalogue lookup, confusable detection, formatting, and cross-platform tooling
predictable. Human-language titles and educational descriptions belong in app
or lesson-pack metadata rather than semantic identifiers.

Comments may contain any valid Unicode text.

### Tokens and maximal matching

The lexer recognizes these multi-character tokens before their prefixes:

```text
--    line-comment opener
/-    block-comment opener
-/    block-comment closer
:=    assignment
->    reaction arrow
```

It then recognizes single-character punctuation:

```text
@  .  :  ^  +  -  *  /  %  (  )  ?
```

Tokenization uses maximal matching. For example, `->` is one arrow token rather
than a minus followed by another token, and `:=` is one assignment token.

Chemical formulae are parsed from ordinary element, integer, grouping, dot,
charge, and phase tokens in species context. Formulae are not opaque lexer
strings.

### Horizontal whitespace

One or more ASCII spaces separate adjacent word-like or numeric tokens where
they would otherwise join. Horizontal whitespace is otherwise insignificant
outside formulae and unit expressions.

Canonical formatting uses exactly one space:

- between keywords and operands;
- around `:=`, `->`, and equation `+`;
- between a decimal and its unit expression;
- between an equation coefficient and species;
- around procedure prepositions such as `to`, `with`, `in`, and `into`.

Canonical formatting emits no trailing whitespace.

Tabs are rejected rather than assigned a display width. Tabs inside comments
are preserved as comment content until the formatter chapter decides canonical
comment treatment.

### Comments

`.chems` supports two comment forms:

```chems
-- A line comment ends before the newline.

/- A block comment may span lines.
   /- Block comments may nest. -/
-/
```

Comments behave as whitespace and never become semantic entities. The lossless
syntax tree retains their text and placement.

Rules:

- `--` consumes through the last character before LF, CRLF, or EOF;
- `/-` and `-/` delimit a nestable block comment;
- an unclosed block comment is a lexical error spanning from its opener to EOF;
- an unmatched `-/` is a lexical error;
- comment delimiters are not recognized inside another token;
- indentation on lines containing only comment trivia does not affect layout;
- a block comment may appear between ordinary tokens but not inside an
  identifier, number, formula element symbol, operator, or unit symbol.

There are no semantic or documentation-comment variants.

### Newlines and layout

NEWLINE is syntactic. INDENT and DEDENT are synthetic tokens produced by the
layout lexer.

Layout rules:

1. The initial indentation stack contains column zero.
2. A nonblank logical line may begin only at an existing indentation level or
   exactly two spaces deeper than the current level.
3. Moving exactly two spaces deeper emits one INDENT.
4. Moving to an earlier level emits one DEDENT per exited level.
5. Any other indentation width is an error; the lexer does not silently invent
   intermediate blocks.
6. Blank and comment-only lines emit NEWLINE but no INDENT or DEDENT.
7. At EOF, the lexer emits a synthetic final NEWLINE when required, followed by
   all outstanding DEDENT tokens and EOF.

Every grammar block therefore has one visually unambiguous parent. Arbitrary
alignment is not syntax. Canonical indentation is always two spaces per level.

Equations have one limited continuation rule: within an equation value, a line
may end before `->`, after `->`, or before/after a `+`. Continuation lines remain
at the equation's existing indentation level and do not emit INDENT. No other
construct has implicit line continuation.

Canonical multiline equations place the arrow at the equation indentation:

```chems
molecular :=
  AgNO3(aq) + NaCl(aq)
  -> AgCl(s) + NaNO3(aq)
```

### Grammar notation

The EBNF uses:

| Notation | Meaning |
| --- | --- |
| `"text"` | Literal token or keyword |
| `name` | Another production |
| `a, b` | Sequence |
| `a \| b` | Alternative |
| `[ a ]` | Zero or one |
| `{ a }` | Zero or more |
| `( a )` | Grouping |

`NEWLINE`, `INDENT`, `DEDENT`, and `EOF` are layout tokens, not source text.
Comments and non-layout horizontal whitespace are omitted from productions.

### Language and catalogue headers

Every file begins with:

```chems
chems 1
use catalog ChemSpec.Aqueous@1
```

`chems 1` selects language major version 1. Minor compatible evolution is
handled by the compatibility policy rather than a source minor version.

The catalogue version may contain one or more non-negative integer components.
The catalogue implementation also resolves and records an immutable content
digest.

A headerless file is not valid source. The compiler does not guess which
language major an input intended to use.

### Required section order

The experiment's outer sections appear in this order:

```text
conditions
assuming       optional
given
vessels
procedure
expect         zero or more stage-targeted blocks
by
```

The order expresses the dependency direction from context and inputs to
operations, claims, and proof. Reordering outer sections is a syntax error.

Entries inside `conditions` may appear in any order. Static checking requires
exactly one `temperature`, `pressure`, and `medium`; the formatter emits that
canonical order.

All other repeated declarations preserve source order because procedure,
claim, proof, and comment ordering is meaningful to humans even where the
kernel later normalizes it.

### Assumption syntax

An assumption entry names a catalogue-defined assumption kind and may select
one target and one stage:

```chems
assuming
  idealSolution for reaction
  negligibleVolumeChange for reaction at final
```

The grammar accepts the shape; catalogue resolution later determines whether
the assumption kind exists, requires a target, permits a stage, and is
applicable.

`for` targets a local material, vessel, or other named experiment entity. `at`
targets `initial`, `final`, or a labelled procedure stage. No assumption payload
can contain an arbitrary proposition.

### Material syntax

A concise material expression has one of two grammatical shapes:

```chems
given
  calciumCarbonate := 5.00 g of CaCO3(s)
  silverNitrate := 50 mL of 0.100 mol/L AgNO3(aq)
```

The first quantity describes the finite sample or total solution. An optional
second quantity describes a concentration. Dimensional elaboration decides
which material constructor is valid; token position alone does not assign a
chemical meaning.

An explicit prepared composition is a block:

```chems
given
  preparedSample := prepared
    5.00 mmol of NaCl(aq)
    100 mmol of H2O(l)
```

Every component must later resolve to a valid species and exact inventory. The
grammar does not permit free-form component descriptions.

### Vessel syntax

Every experiment declares at least one logical vessel:

```chems
vessels
  reaction := open vessel 250 mL
  filtrate := open vessel 250 mL
  residue := open vessel 100 mL
```

The only normative vessel constructor is `open|closed vessel <capacity>`.
Rendered apparatus types such as beaker or flask are presentation choices.
Chemically relevant vessel extensions require a future language version or a
catalogue-resolved property with specified semantics.

### Procedure syntax

The grammar fixes this closed operation vocabulary:

```chems
place material in vessel
add material to vessel
combine left with right in vessel
transfer [quantity] from sourceVessel to targetVessel
stir vessel [for duration]
heat vessel to temperature
cool vessel to temperature
wait duration
seal vessel
open vessel
filter sourceVessel into filtrateVessel and residueVessel
decant sourceVessel into targetVessel
```

An operation may have a label:

```chems
procedure
  place silverNitrate in reaction
  mixed: add sodiumChloride to reaction
  stir reaction
```

The label names the stage after that operation completes. Unlabelled operations
still produce semantic stages but cannot be referenced by source name.

The grammar contains no operation for asserting that a reaction occurs.
Chemical transformations are kernel conclusions at operation boundaries.

### Expectation syntax

Each `expect` block targets one stage. Missing `at` is canonical sugar for
`at final`:

```chems
expect at mixed
  produces AgCl(s)

expect
  class := precipitation
  molecular := ?
```

Multiple expectation blocks are aggregated into one semantic expectation.
Repeating an incompatible claim at the same stage is a later static or proof
error rather than a parsing ambiguity.

Claims use a closed grammatical vocabulary:

```chems
class := precipitation
produces AgCl(s)
consumes Ag^+(aq)
remains NO3^-(aq)
spectator Na^+(aq)
amount AgCl(s) := 5.00 mmol
limiting := silverNitrate
```

Any permitted value position may use the anonymous hole `?` where the grammar
explicitly says so:

```chems
class := ?
produces ?
amount AgCl(s) := ?
limiting := ?
```

`?` is not a wildcard and cannot occur in inputs, conditions, procedures,
assumptions, or tactic arguments. The surrounding claim assigns its type and
creates one source-addressable proof goal.

Equation claims accept inline or block values:

```chems
netIonic := Ag^+(aq) + Cl^-(aq) -> AgCl(s)

molecular :=
  AgNO3(aq) + NaCl(aq)
  -> AgCl(s) + NaNO3(aq)
```

Canonical formatting uses a block equation whenever the formatted inline form
would exceed the configured canonical line width. Line width affects only
layout, never AST or HIR meaning.

Observation claims remain grouped:

```chems
observe
  precipitate AgCl(s)
  gas CO2(g)
  colour := white
  temperatureChange := none
```

Observation values come from closed syntax or catalogue-qualified names; no
quoted natural-language observation is part of the language.

### Formula, species, and equation syntax

Formula syntax supports element symbols, positive subscripts, parenthesized
groups, and dot-separated adduct segments:

```text
NaCl
Ca(OH)2
Al2(SO4)3
CuSO4.5H2O
```

The canonical source dot is ASCII `.`. The application may display it as a
centered dot without changing source.

A species always includes a phase and may include a charge:

```text
AgNO3(aq)
Ag^+(aq)
SO4^2-(aq)
AgCl(s)
H2O(l)
CO2(g)
```

Charge magnitude precedes its sign. Magnitude one is omitted canonically.
Neutral species omit charge syntax. Phase is mandatory in every species
position.

Equation coefficients are positive integers written before species. Coefficient
one is omitted canonically. The grammar does not include fractional
coefficients, reversible arrows, equilibrium arrows, isotope notation,
coordination brackets, electrons, radical notation, or structural formulae.

Those exclusions align the grammar with the initial trusted chemistry domain;
they are not silently approximated.

### Quantity and unit-expression syntax

A quantity is an exact decimal followed by a unit expression:

```text
50 mL
0.100 mol/L
-5 degC
1.25 g/mol
2.0 mol*L^-1
```

The grammar preserves the decimal lexeme, including trailing zeros. It has no
scientific notation, arithmetic expressions, constants, implicit units, or
unitless bare decimals in source.

Unit expressions support multiplication, division, and signed integer powers.
The quantitative and chemical type-system chapter below defines the accepted
symbols, dimensions, exact conversion factors, canonical units, positivity
constraints, and semantic equivalence.

A unit symbol has its own context-sensitive lexical class rather than entering
the experiment's identifier namespace. It may therefore use spellings such as
`g`, `L`, `mol`, or `s` even when that spelling is reserved or meaningful in
another grammar context.

Whitespace is required between the decimal and unit expression and forbidden
inside an individual unit symbol. Canonical formatting emits `*`, `/`, and `^`
without surrounding spaces inside a unit expression.

### Identifier classes

There are two identifier forms:

```text
valueIdentifier = lowercase ASCII letter, then ASCII letters/digits/underscore
TypeIdentifier  = uppercase ASCII letter, then ASCII letters/digits/underscore
```

Canonical style is lower camel case for values and upper camel case for the
experiment name and catalogue namespace segments. Underscores are syntactically
valid to support generated and migrated source, but the formatter does not
rename identifiers.

Qualified catalogue names join value or type segments with `.`:

```text
ChemSpec.Aqueous
solubilityRules
ChemSpec.Assumptions.idealSolution
```

Element symbols inside formula context are not identifiers and do not resolve
through the experiment's value namespace. Unit symbols likewise use the unit
namespace rather than the value or type identifier classes.

### Reserved words

The following case-sensitive spellings are reserved throughout the language and cannot
be used as value or type identifiers:

```text
add amount and aq aqueous assuming at atoms auto balance by cancel catalog
charge chems class close closed colour combine completeIonic conditions consumes
cool decant decrease derive dissociate expect experiment filter final for from g
gas gasFormation given heat in increase infer initial into l limiting medium
molecular netIonic neutralization noReaction none observations observe of open
place precipitate precipitation prepared pressure procedure produces products
prove remains s seal solve spectator spectators stir stoichiometry temperature
temperatureChange to transfer use using verify vessel vessels wait where with
```

The compiler maintains this set as language data and tests it against every
literal keyword in the normative grammar. Catalogue names matching a reserved
word remain addressable only through a catalogue alias mechanism if a later
chapter explicitly defines one; source does not escape keywords.

### Scopes and declarations

The source contains these namespaces:

| Namespace | Members |
| --- | --- |
| Experiment type | The single experiment name |
| Local value | Materials, vessels, and procedure stage labels |
| Catalogue | Catalogue entities, rule families, media, colours, and assumption kinds |
| Formula | Element symbols and formula groups parsed only in formula context |

All local declarations belong to the experiment scope. Nested layout blocks do
not introduce shadowing scopes.

Rules:

- every local material, vessel, and stage label is unique across the shared
  local-value namespace;
- `initial` and `final` are built-in stage references and cannot be declared;
- all local declarations are collected before reference resolution, permitting
  assumptions to refer to materials or labelled stages declared later in the
  file;
- procedure operands must resolve to the declaration kind required by that
  operation;
- catalogue-qualified names never resolve to local declarations;
- duplicate declarations are errors with both source spans;
- unused materials, vessels, assumptions, and stage labels are warnings unless
  a later semantic rule makes their use mandatory;
- there is no import, alias, export, module, function, parameter, or local
  declaration syntax.

### Canonical complete shape

This is the canonical source shape fixed by the grammar chapter:

```chems
chems 1
use catalog ChemSpec.Aqueous@1

experiment SilverChloridePrecipitation where
  conditions
    temperature := 25 degC
    pressure := 1 atm
    medium := aqueous

  given
    silverNitrate := 50 mL of 0.100 mol/L AgNO3(aq)
    sodiumChloride := 50 mL of 0.100 mol/L NaCl(aq)

  vessels
    reaction := open vessel 250 mL

  procedure
    place silverNitrate in reaction
    mixed: add sodiumChloride to reaction
    stir reaction

  expect at final
    class := precipitation
    produces AgCl(s)
    molecular :=
      AgNO3(aq) + NaCl(aq)
      -> AgCl(s) + NaNO3(aq)
    completeIonic := ?
    netIonic := ?
    amount AgCl(s) := ?

    observe
      precipitate AgCl(s)
      colour := white

  by
    dissociate aqueous
    infer products using solubilityRules
    balance molecular
    derive completeIonic
    cancel spectators
    solve stoichiometry
    verify atoms
    verify charge
    prove observations
    close
```

### Decisions fixed by the grammar chapter

This chapter fixes:

- UTF-8 files with ASCII semantic tokens and Unicode comments;
- LF canonical newlines, mandatory final newline, and no BOM or NUL;
- exact two-space layout with synthetic NEWLINE/INDENT/DEDENT tokens;
- `--` line comments and nested `/- ... -/` block comments;
- one explicit `chems 1` header and one catalogue bundle;
- the required outer-section order;
- required material, vessel, procedure, and proof blocks;
- optional assumptions and zero or more stage-targeted expectation blocks;
- the closed procedure, claim, observation, and tactic spellings;
- anonymous typed holes written `?` only in permitted claim values;
- mandatory phases, charge syntax, formula grouping, adduct dots, and equation
  coefficients;
- exact decimal and compound unit-expression syntax;
- ASCII value/type identifiers, one shared local-value namespace, no shadowing,
  and no imports or user declarations;
- the normative EBNF as the syntactic conformance authority.

## Quantitative and chemical type system

This chapter defines the typed meaning of numeric literals, units, dimensions,
formulae, species, and initial materials. These rules are normative for
elaboration and for every later kernel calculation.

The central rule is:

> Source spelling and educational precision are preserved, while mathematical
> reasoning uses exact normalized values and never binary floating point.

### Numeric representations

The implementation distinguishes three numeric concepts.

```text
SourceDecimal
  original lexeme
  signed integer coefficient
  decimal scale
  written decimal places
  written significant digits

ExactScalar
  arbitrary-precision signed rational numerator/denominator

DisplayedNumber
  exact scalar
  chosen unit
  explicit rounding and precision metadata
```

For a source decimal with digits `d` and `scale` digits after the decimal point:

```text
value = signed_integer(d) * 10^(-scale)
```

Examples:

| Source | Coefficient | Scale | Exact value |
| --- | ---: | ---: | ---: |
| `50` | `50` | `0` | `50` |
| `0.100` | `100` | `3` | `1/10` |
| `-5` | `-5` | `0` | `-5` |
| `273.15` | `27315` | `2` | `5463/20` |

The compiler never parses one of these values through `f32` or `f64`.

Arithmetic uses reduced arbitrary-precision rationals. Addition,
subtraction, multiplication, division, comparison, unit conversion, reaction
extent, and stoichiometry are therefore deterministic across platforms.
Division by zero and implementation resource exhaustion are explicit errors;
they cannot yield infinity, NaN, or a silently rounded result.

### Written precision

The source lexeme remains attached to each authored quantity. For a nonzero
decimal, significant digits run from the first nonzero digit through the final
written digit. Leading zeros are not significant; trailing written zeros are.

```text
50       -> 2 significant digits
50.0     -> 3 significant digits
0.100    -> 3 significant digits
0.01020  -> 4 significant digits
```

For exact zero, the compiler retains decimal places and written digits rather
than inventing a significant-figure count.

Written precision is educational and presentational metadata. It is not
an implicit uncertainty interval. Quantitative proof claims compare exact
normalized values, so `5 mmol` does not silently accept `5.4 mmol` merely
because a display could round them alike.

The validated artifact records both:

- the exact rational result used by the kernel;
- a recommended display precision derived from the authored inputs and the
  calculation performed.

The application may explain significant-figure propagation, but it may never
replace the kernel value with its rounded presentation. Explicit uncertainty,
tolerance, and interval syntax are outside the language and require a future
version.

Canonical formatting preserves authored trailing zeros and omits an explicit
leading `+`. Numerically equal literals with different precision metadata are
mathematically equal but not source-identical.

### Dimension algebra

Every multiplicative quantity has a dimension vector over five base
dimensions:

```text
Dimension
  mass          M
  length        L
  time          T
  amount        N
  temperature   Q
```

Each component is a signed integer exponent. Examples:

| Quantity kind | Dimension |
| --- | --- |
| Dimensionless | `1` |
| Mass | `M` |
| Length | `L` |
| Time | `T` |
| Chemical amount | `N` |
| Volume | `L^3` |
| Concentration | `N L^-3` |
| Pressure | `M L^-1 T^-2` |
| Molar mass | `M N^-1` |
| Density | `M L^-3` |

Multiplication adds exponent vectors, division subtracts them, and integer
powers multiply them. Addition, subtraction, ordering, and equality require
identical dimensions after unit resolution.

Chemical ionic charge is not represented by the physical-unit dimension
system. It is an exact integral property of `Species` and equations.

### Temperature points

Absolute temperature is an affine quantity rather than an ordinary
multiplicative scalar. The type system distinguishes:

```text
TemperaturePoint
TemperatureDifference
```

Conditions and `heat ... to` or `cool ... to` operands require a
`TemperaturePoint`. Source has no standalone temperature-difference
constructor.

`K` and `degC` are permitted only as standalone temperature-point units. They
cannot appear in products, quotients, or powers. Conversion is exact:

```text
kelvin = degrees_celsius + 273.15
```

A temperature below absolute zero is ill-typed. Negative Celsius values are
otherwise valid.

### Unit resolution

The grammar accepts a general unit-expression shape. Elaboration resolves each
unit symbol through the closed unit registry, expands aliases, combines
dimensions, and computes one exact conversion into canonical base units.

The unit registry is:

| Symbol | Kind | Exact definition |
| --- | --- | --- |
| `kg` | Mass | canonical mass unit |
| `g` | Mass | `1/1000 kg` |
| `mg` | Mass | `1/1000000 kg` |
| `m` | Length | canonical length unit |
| `cm` | Length | `1/100 m` |
| `mm` | Length | `1/1000 m` |
| `L` | Volume | `1/1000 m^3` |
| `mL` | Volume | `1/1000000 m^3` |
| `uL` | Volume | `1/1000000000 m^3` |
| `mol` | Amount | canonical amount unit |
| `mmol` | Amount | `1/1000 mol` |
| `umol` | Amount | `1/1000000 mol` |
| `s` | Time | canonical time unit |
| `min` | Time | `60 s` |
| `h` | Time | `3600 s` |
| `K` | Temperature point | canonical absolute temperature unit |
| `degC` | Temperature point | `K = degC + 273.15` |
| `Pa` | Pressure | `kg*m^-1*s^-2` |
| `kPa` | Pressure | `1000 Pa` |
| `atm` | Pressure | `101325 Pa` |
| `M` | Concentration | `mol/L` |
| `mM` | Concentration | `mmol/L` |
| `%` | Dimensionless ratio | `1/100` |

ASCII `u` is canonical for the micro prefix. The application may display `uL`
and `umol` with `µ` without changing source.

All registry factors are exact. `mol/L`, `mol*L^-1`, and `M` therefore
normalize to the same dimension and value.

Rules:

- an unknown unit symbol is `IllTyped`, not `Unsupported`;
- unit exponents are exact signed integers;
- resolving a unit expression cannot depend on the empirical catalogue;
- `%` and `degC` must appear alone;
- `K` must appear alone when used as a `TemperaturePoint`;
- a dimensionally valid but contextually wrong quantity is `IllTyped`, such as
  using `5 g` for pressure;
- conversion preserves the original unit expression and source span for
  diagnostics and display.

### Typed quantities

A resolved multiplicative quantity contains:

```text
Quantity
  exact rational value in canonical base units
  dimension vector
  original SourceDecimal
  original unit-expression syntax
  exact conversion derivation
```

A resolved temperature point contains its exact kelvin value plus the original
decimal and temperature unit.

Quantity equality compares dimension and exact canonical value. Source
equality additionally compares original spelling and precision metadata.

Context imposes value restrictions after dimensional typing:

| Context | Required value |
| --- | --- |
| Initial material amount, mass, volume, or concentration | Greater than zero |
| Vessel capacity | Greater than zero |
| Pressure | Greater than zero |
| Duration | Greater than or equal to zero |
| Absolute temperature | Greater than or equal to zero kelvin |
| Authored product or remaining amount claim | Greater than or equal to zero |

Negative syntax is retained because temperature points require it and because
one grammar should report a context-aware error rather than fail lexically.

### Formula syntax tree

Formula parsing produces a structural tree before catalogue resolution:

```text
FormulaSyntax
  segments[]
    coefficient
    parts[]
      ElementSymbol(symbol, count)
      Group(parts[], count)
```

Every omitted count is one. Written counts and segment coefficients must be
positive integers. Counts are arbitrary-precision during normalization, subject
only to documented compiler resource limits.

For example:

```text
Ca(OH)2
  Element Ca * 1
  Group(
    Element O * 1
    Element H * 1
  ) * 2
```

The dot in `CuSO4.5H2O` begins an adduct segment. Its coefficient multiplies the
entire following segment, so normalization yields:

```text
Cu: 1
S:  1
O:  9
H: 10
```

Formula grouping records composition only. It does not encode bond structure,
geometry, oxidation state, resonance, stereochemistry, or mechanism.

### Element resolution and normalized formulae

The selected catalogue bundle includes one versioned periodic-table registry.
Each syntactically valid element symbol must resolve to exactly one stable
`ElementId` and atomic number.

An unknown symbol such as a syntactically valid but nonexistent `Xx` is
`IllTyped`: no formula can be constructed from it. Atomic weights and other
empirical element properties are separate catalogue premises and are not
required merely to normalize composition.

Normalization recursively expands groups and adduct coefficients into an
ordered map:

```text
NormalizedFormula
  composition: ElementId -> positive integer count
  structural source tree
  source span map
```

The map is canonically ordered by atomic number. Formula equality compares the
normalized composition map. Source ordering and grouping are retained for
formatting and explanation but do not change formula equality.

Consequently, compositionally equivalent spellings can have equal formulae
without identifying the same empirical substance. Isomers and polymorphs are
distinguished by catalogue `SubstanceId`, not by formula equality.

### Molar mass

Molar mass is derived only when required. For formula `F`:

```text
molar_mass(F) = sum(element_count(E) * catalogue_atomic_mass(E))
```

Every atomic-mass value is a versioned catalogue premise with its own source
and written precision. The exact central decimal supplied by the catalogue is
converted to a rational and used by the kernel. The derivation records every
element contribution.

The classroom model neglects the electron-mass difference between neutral
formula units and ions. This is a declared kernel-model convention recorded in
the derivation metadata, not a hidden per-experiment assumption.

If formula composition resolves but a required atomic mass is absent, a
mass-based material calculation is `Unsupported`; formula parsing and
amount-based calculations may still be supported.

### Charge and phase

A source charge elaborates into an exact signed integer:

```text
^+   -> +1
^-   -> -1
^2+  -> +2
^3-  -> -3
```

Magnitude zero is impossible in source. Neutral species have charge zero and
omit charge syntax.

The phase set is closed:

| Source | Phase meaning |
| --- | --- |
| `(aq)` | Dissolved in the experiment's resolved medium |
| `(s)` | Solid phase |
| `(l)` | Liquid phase |
| `(g)` | Gas phase |

Phase is mandatory and is part of species identity. `H2O(l)` and `H2O(g)` are
different species even though they share formula and neutral charge.

`(aq)` does not mean “known to be soluble.” It is an authored phase claim or
initial-state declaration that must be supported by the catalogue under the
experiment's medium and conditions. Dissociation is likewise not implied by
the parser or formula type.

### Substance and species resolution

The semantic distinction is:

```text
Substance
  stable catalogue identity
  formula identity
  empirical properties and aliases

ResolvedSpecies
  SubstanceId
  NormalizedFormula
  signed integral charge
  Phase
  applicable condition domain
  catalogue origin
```

Resolution uses the tuple of normalized formula, charge, phase, medium, and
conditions. A conforming catalogue bundle must expose at most one supported
`SubstanceId` for any such tuple. Ambiguous tuples make the catalogue bundle
invalid before experiment evaluation; source has no identity-disambiguation
syntax in the language.

Resolution outcomes are classified as follows:

| Situation | Outcome |
| --- | --- |
| Formula syntax malformed | `Malformed` |
| Element symbol does not exist | `IllTyped` |
| Explicit qualified catalogue name does not exist | `IllTyped` |
| Formula and elements are valid but no supported substance entry exists | `Unsupported` |
| Catalogue explicitly disproves the authored phase or condition | `Invalid` |
| Exactly one applicable catalogue species exists | Resolved species |

Lack of catalogue evidence is never evidence that no substance or reaction
exists.

Species equality compares resolved `SubstanceId`, normalized formula, exact
charge, and phase. Conditions are checked for applicability but do not turn the
same species into a different identity.

### Analytical and actual species

A solution declaration such as `AgNO3(aq)` introduces an analytical component:
the amount of silver nitrate used to prepare the solution. It does not silently
replace that component with ions during material elaboration.

Catalogue-backed dissociation rules may later derive actual dissolved species:

```text
AgNO3(aq) -> Ag^+(aq) + NO3^-(aq)
```

The validated artifact preserves both views:

- analytical inventory for preparation, stoichiometry explanation, and source
  traceability;
- actual derived species inventory for ionic equations and simulation.

This prevents parser syntax from embedding an empirical dissociation claim.

### Material constructors

The grammar's concise material expression is elaborated by the dimensions and
phase of its operands.

For one quantity:

| Source shape | Required dimension/phase | Typed constructor |
| --- | --- | --- |
| `q of S` | Amount; any phase | `SampleByAmount` |
| `q of S` | Mass; any phase | `SampleByMass` |
| `q of S` | Volume; liquid phase | `LiquidSampleByVolume` |
| `q of S` | Volume; gas phase | `GasSampleByVolume` |

For two quantities:

| Source shape | Requirements | Typed constructor |
| --- | --- | --- |
| `q1 of q2 S` | `q1` is Volume, `q2` is Concentration, `S` is aqueous | `Solution` |

No other dimensional combination is legal. In particular:

- concentration without a total volume is ill-typed;
- volume of a solid is ill-typed because bulk packing is not a defined
  material model;
- a two-quantity non-aqueous material is ill-typed;
- percentages do not form a material constructor merely because `%` is a valid
  dimensionless unit;
- every source material quantity and concentration must be greater than zero.

The typed material variants are:

```text
Material
  id and source name
  analytical component inventory
  physical phase/context
  known exact amount, mass, and volume values
  derivations for converted or inferred values
  required catalogue premises
  source-origin map

SampleByAmount
  species
  exact amount

SampleByMass
  species
  exact mass
  derived amount using molar mass

LiquidSampleByVolume
  species
  exact volume
  derived mass/amount using applicable density

GasSampleByVolume
  species
  exact volume
  derived amount using an applicable gas model

Solution
  analytical aqueous species
  exact total volume
  exact analytical concentration
  exact analytical amount = volume * concentration
  resolved medium/solvent
```

Derived fields remain absent until their required premises are established.
The type is not populated with guessed values.

### Material premise requirements

Material construction may require empirical premises:

| Constructor | Required premises beyond species identity |
| --- | --- |
| `SampleByAmount` | None |
| `SampleByMass` | Formula atomic masses for derived amount |
| `LiquidSampleByVolume` | Applicable density and formula molar mass |
| `GasSampleByVolume` | Pressure, temperature, and an applicable gas model |
| `Solution` | Medium-to-solvent identity; no dissociation premise yet |

A missing required empirical premise produces `Unsupported` unless the
catalogue defines an explicit permitted assumption kind that the source admits.
Using such an assumption is traced and leads to `ValidatedWithAssumptions`.

For a gas sample, the ideal-gas equation is not a universal hidden default. It
may be applied only through a catalogue rule whose premises hold or through an
explicit permitted ideal-gas assumption.

For a solution, analytical amount is exactly:

```text
amount = concentration * total_volume
```

No assumption of volume additivity is needed to construct the input solution.
After multiple solutions are combined, calculating a new concentration or
total volume requires an applicable mixture-volume model or an explicit
permitted assumption. Amount-based stoichiometry can proceed without that
assumption.

### Prepared compositions

A `prepared` material contains one or more component entries already
co-located before the experiment procedure begins.

Each component quantity must elaborate as one of:

- amount of any supported species;
- mass of any supported species with molar-mass support;
- volume of a supported liquid or gas with the corresponding material model.

Concentration is not a prepared-component basis because it lacks a total
component volume in that syntax. Duplicate resolved components are summed
exactly in normalized inventory while retaining every source origin.

A one-component prepared material is legal but produces a simplification
warning.

Prepared composition does not assert that its components are chemically inert.
Before `Stage[0]` is accepted, the kernel evaluates the co-located composition
under initial conditions. A supported transformation becomes part of the
initial-preparation derivation; an undecidable transformation makes the
experiment unsupported. The material cannot bypass reaction validation by
calling a mixture “prepared.”

### Linear material identity

Every material declaration receives one stable `MaterialId` and owns a finite
inventory. The same source material cannot be placed or added twice unless a
typed operation has explicitly divided or transferred part of its inventory.

Material equality is not formula equality. It compares normalized component
inventories, exact quantities, phase/context, applicable conditions, and
derivation premises. Two separately declared equal materials retain different
linear identities.

Materials initially exist outside all declared vessels. Procedure operations
move them into vessel state. Declaring a material never places it, reacts it
with another declaration, or creates renderer particles.

### Type judgments

Later implementation and conformance tests express these elaboration relations:

```text
UnitRegistry |- decimal unitExpression => Quantity
Catalogue |- formulaSyntax => NormalizedFormula
Catalogue; Conditions |- formula charge phase => ResolvedSpecies
Catalogue; Conditions |- materialExpression => Material
```

Each successful judgment produces a typed value, exact conversion or
normalization derivation, and source-origin mapping. Each failed judgment
produces one primary diagnostic classification and may include related spans or
missing-premise information.

### Quantitative and chemical invariants

Before procedure elaboration begins:

- every decimal has an exact rational value;
- every quantity has one resolved dimension or temperature-point type;
- every unit conversion is exact and derivable;
- every formula contains only resolved elements and positive counts;
- every species has one resolved catalogue identity, charge, and phase;
- every material matches exactly one legal constructor;
- every finite material inventory is positive and dimensionally sound;
- every inferred amount, mass, or volume names its premises;
- analytical and actual species inventories remain distinct;
- no empirical fact has been introduced by source syntax;
- no floating-point value participates in validation.

### Decisions fixed by the quantitative and chemical chapter

This chapter fixes:

- exact rational kernel arithmetic and lossless source-decimal metadata;
- significant figures as presentation metadata, not implicit tolerance;
- five-dimensional multiplicative unit algebra plus affine temperature points;
- the closed unit registry and exact conversions;
- context-specific positivity and dimensional requirements;
- structural formula trees, group/adduct normalization, and formula equality;
- catalogue-backed element identities and atomic-mass premises;
- exact integral charge and the closed four-phase model;
- the boundary among malformed, ill-typed, unsupported, and contradicted
  species;
- analytical versus actual dissolved species;
- dimension-directed sample, solution, and prepared-material constructors;
- explicit premise requirements for molar mass, density, gas, solvent, and
  post-mixing volume calculations;
- linear material identity and exact normalized inventories.

## Experiment state and procedure semantics

This chapter defines conditions, logical vessels, procedure operations,
immutable stages, reaction opportunities, and transition invariants. Procedure
execution is deterministic and finite. It produces semantic chemistry states,
not renderer instructions.

### Initial conditions

Every experiment defines exactly one initial environment:

```text
Environment
  temperature: TemperaturePoint
  pressure: Pressure
  medium: MediumId
```

`temperature`, `pressure`, and `medium` each occur exactly once. Duplicate or
missing entries are `IllTyped`.

The selected medium resolves through the catalogue to a stable `MediumId` and
at least:

- its solvent identity or declared component model;
- the phases and species forms it can support;
- applicable dissociation and reaction-rule domains;
- the condition ranges over which those facts are reviewed.

The environment is an initial boundary condition, not a permanently enforced
global value. Each vessel copies it when constructed and can later change
temperature, pressure, or closure through typed operations.

The engine does not extrapolate empirical facts beyond their declared condition
domains. A well-typed experiment outside supported domains is `Unsupported`.

### Vessel identity and state

Every vessel declaration constructs one empty logical vessel:

```text
VesselState
  id: VesselId
  capacity: Volume
  closure: Open | Closed
  temperature: TemperaturePoint
  pressure: Pressure
  contents: MaterialInventory
  phase partitions
  mixing state
```

The initial temperature and pressure are copied from the environment. Vessel
capacity is positive and exact. Vessel identity is linear and stable through
every stage.

An open vessel is coupled to ambient pressure under the language model. A closed
vessel retains an internal pressure that may require a supported gas or volume
model to update. Opening a vessel whose internal pressure differs from ambient,
or whose contents may escape, is `Unsupported` unless the catalogue/kernel has
an applicable release model. The engine never silently discards material.

Vessel capacity is a hard invariant whenever contained volume is known. If an
operation needs a capacity judgment but volume cannot be derived, the operation
is `Unsupported`; it is not assumed to fit.

Apparatus shape and screen geometry are absent from vessel state.

### Material locations and inventory ledger

At every stage, each finite inventory portion has exactly one location:

```text
Unplaced(MaterialId)
InVessel(VesselId)
ConsumedBy(ReactionEventId)
ProducedIn(VesselId, ReactionEventId)
SeparatedInto(VesselId, OperationId)
```

The ledger records every movement, split, combination, chemical consumption,
production, and separation. A source material cannot be placed twice or exist
in two vessels simultaneously.

Chemical consumption does not delete elemental inventory. It closes one species
lot and opens stoichiometrically related product lots linked by the reaction
derivation.

### Stage model

`Stage[0]`, named `initial`, is the immutable state after:

1. conditions, materials, and vessels elaborate;
2. prepared compositions undergo their required initial validation;
3. no declared material has yet been placed into a vessel.

Every procedure operation produces exactly one subsequent immutable stage.

```text
Stage
  id and ordinal
  optional source label
  elapsed experiment time
  environment
  vessel states
  unplaced material inventories
  cumulative inventory ledger
  transition operation
  reaction events caused at this boundary
  observations established at this boundary
  source-origin map
```

`final` aliases the last procedure stage. It is not a separately constructed
stage. An experiment must contain at least one operation, so `final` never
aliases `initial`.

An operation label names its resulting stage. Labels do not name the operation
before it runs.

### Transition pipeline

For operation `op[n]`, the engine evaluates:

```text
Stage[n]
    -> check static operand kinds
    -> check state-dependent preconditions
    -> apply explicit movement/condition/separation transition
    -> construct reaction opportunities
    -> resolve supported reaction closure
    -> verify stage and ledger invariants
    -> establish observations and claim inputs
    -> Stage[n + 1]
```

If any required step is ill-typed, invalid, incomplete, or unsupported, no
successor stage is admitted into a validated timeline.

Tactics do not execute this transition imperatively. They construct and
discharge the proof goals required to justify its result.

### Reaction opportunities

A `ReactionOpportunity` is created when an explicit operation changes one or
more of:

- co-location of materials;
- homogeneous contact between phases;
- temperature;
- pressure or vessel closure;
- phase partition following a supported separation.

It contains the affected vessel, candidate actual species inventory, conditions,
operation origin, and applicable catalogue rule families.

`place`, `add`, `combine`, `stir`, `heat`, `cool`, `seal`, `open`, `filter`, and
`decant` may create opportunities. `transfer` creates one in the destination
and, when a partial heterogeneous state changes, may create one in the source.
`wait` does not create a kinetic transformation, but it creates a stage
at which persistent state and observations may be inspected.

The kernel resolves opportunities to a deterministic reaction closure. Source
operations never state products or force a reaction rule.

### Operation semantics

Every operation has closed operand and state requirements.

#### `place material in vessel`

- `material` resolves to an unplaced declared material.
- `vessel` resolves to an empty vessel.
- The entire material inventory moves into the vessel.
- Capacity must be established where volume is known or required.
- A reaction opportunity is created for the placed material under vessel
  conditions.

Using an already placed material or nonempty vessel makes the transition
`Invalid`.

#### `add material to vessel`

- `material` resolves to an unplaced declared material.
- The entire material inventory moves into the target vessel.
- Existing contents remain.
- Capacity must be established.
- Co-location creates a reaction opportunity.

#### `combine left with right in vessel`

- Both operands resolve to distinct unplaced declared materials.
- The target vessel is empty.
- Both complete inventories move atomically into the vessel.
- Capacity must be established.
- One co-location opportunity is created; source order does not give either
  material chemical priority.

`combine` is semantic sugar for an atomic two-material placement, not two
order-sensitive `add` operations.

#### `transfer [quantity] from source to target`

Without a quantity, all source contents move to an empty target vessel.

With a quantity:

- the quantity must have Volume dimension and be positive;
- the source must contain one homogeneous mobile phase with known volume;
- the requested volume cannot exceed the available mobile volume;
- every component of that homogeneous phase is divided in the same exact
  proportion;
- immobile solid residue remains in the source;
- the target must be empty.

A quantified transfer from a heterogeneous or volume-unknown source is
`Unsupported`, because the language has no sampling-position model. Capacity is checked
for the target. Resulting source and target states may each create reaction
opportunities.

#### `stir vessel [for duration]`

The vessel must contain material. Duration, when present, is non-negative.

Stirring marks compatible mobile phases as homogeneously contacted and creates
a reaction opportunity. It does not introduce kinetic rate laws, mechanical
energy, evaporation, or heating. Its optional duration is recorded for the
educational timeline only.

#### `heat vessel to temperature`

The target must be strictly above the vessel's current temperature. The
operation sets the exact temperature point and creates a reaction opportunity
under the new conditions.

It does not calculate heat capacity, required energy, heating rate, boiling,
evaporation, or thermal gradients. If the new temperature requires a phase or
pressure transition that the supported model cannot establish, the transition
is `Unsupported`.

#### `cool vessel to temperature`

The target must be strictly below the current temperature. It otherwise follows
the same model boundary as `heat`.

Using `heat` for a lower target or `cool` for a higher target is `Invalid` with
a safe replacement diagnostic.

#### `wait duration`

Duration is non-negative and advances the experiment's elapsed time. All vessel
chemistry state remains unchanged.

`wait` cannot make an unsupported reaction succeed, imply completion, model
settling time, or invoke kinetics. It exists to create an explicit observation
and presentation boundary.

#### `seal vessel`

The vessel must be open. Closure changes to `Closed`; contents do not move.
Internal pressure initially equals the pre-seal pressure. A reaction opportunity
is created because later gas-producing rules may depend on closure.

#### `open vessel`

The vessel must be closed. If pressure equalization and retention of contents
can be proven, closure changes to `Open` and pressure becomes ambient. Otherwise
the transition is `Unsupported`; material is never silently vented.

#### `filter source into filtrate and residue`

All three vessel operands are distinct. Destination vessels must be empty.

The ideal filtration model partitions:

- supported mobile liquid and dissolved species into the filtrate vessel;
- supported solid-phase material into the residue vessel.

The source becomes empty. Every amount is conserved exactly. If a component's
phase or filterability is undecidable, the operation is `Unsupported`.

Ideal complete partition is a named kernel operation model recorded in the
derivation. It is not presented as real equipment efficiency.

#### `decant source into target`

The target must be empty. The ideal decant model moves all supported mobile
liquid phase to the target and retains supported solid phase in the source.

If no solid/liquid partition exists, or phase mobility is undecidable, the
operation is `Invalid` or `Unsupported` respectively. Ideal complete decanting
is recorded as a kernel operation model.

### Static versus state-dependent failures

| Failure | Classification |
| --- | --- |
| Operand name missing or wrong declaration kind | `IllTyped` |
| Quantity has wrong dimension | `IllTyped` |
| Operation precondition contradicted by a known stage | `Invalid` |
| Amount or capacity is exceeded | `Invalid` |
| Required density, volume, phase, gas, or partition model is absent | `Unsupported` |
| A permitted explicit assumption supplies the missing premise | Continue as assumption-dependent |
| Inventory or conservation invariant fails internally | Internal kernel failure; no language result artifact |

An operation does not become unsupported merely because its authored
precondition is false. Known false inputs are invalid; missing trusted knowledge
is unsupported.

### Reaction closure and determinism

At each opportunity, the kernel enumerates applicable rules from the selected
supported domain. Rule resolution must produce one of:

- a unique supported reaction closure;
- a supported proof of no net reaction under a declared complete rule domain;
- a unique closure dependent on explicit permitted assumptions;
- `Unsupported` because knowledge is incomplete or outcomes are non-confluent;
- `Invalid` because an authored state or claim contradicts established facts.

The kernel does not select the first matching rule. If multiple applicable
rules do not prove the same normalized final inventory and reaction extent, the
outcome is `Unsupported`.

Repeated rule application terminates because each rule family supplies a
well-founded measure over finite inventory and the kernel admits no general
recursion. The kernel chapter defines the checked measures.

### Stage observations

Observations are derived properties of a stage or its cumulative reaction
events. Procedure operations do not author observations.

A stage may establish:

- presence of a supported precipitate;
- supported gas production or gas-phase presence;
- catalogue-backed colour contributions;
- qualitative net temperature direction relative to `initial`;
- visible phase separation supported by the state model.

The renderer may dramatize these observations but cannot introduce one that is
absent from the validated stage.

### Stage invariants

Every admitted stage satisfies:

- all quantities and conditions are typed and finite exact values;
- each inventory portion has one location and one lineage;
- no amount, mass, volume, or component count is negative;
- vessel capacity holds wherever volume is required and supported;
- elemental composition is conserved across chemical events;
- total integral charge is conserved across chemical events;
- movement and separation conserve every component amount;
- reaction consumption does not exceed available reactant inventory;
- product amounts equal stoichiometric extent;
- vessel closure, pressure, temperature, and contents are explicit;
- every empirical transition premise resolves to catalogue provenance;
- every state change traces to one source operation and derivation node.

These are kernel-required goals even if the source proof omits corresponding
named tactics.

### Procedure equivalence

Two procedures are semantically equivalent only when their typed operations
produce equal complete stage timelines. Equal final inventories alone do not
make procedures equivalent because intermediate stages and claims may differ.

Within an atomic `combine`, swapping left and right operands is equivalent.
Sequential `add` operations are not presumed commutative, even when a particular
kernel derivation later proves equal outcomes.

### Decisions fixed by the state and procedure chapter

This chapter fixes:

- exact initial environment and catalogue-bounded condition applicability;
- logical vessel state, capacity, closure, pressure, and contents;
- linear inventory locations and a complete movement/reaction ledger;
- `initial`, per-operation immutable stages, labels, and `final` aliasing;
- the transition pipeline and reaction-opportunity boundary;
- exact preconditions and effects for every procedure operation;
- ideal filtration and decanting as visible kernel operation models;
- no implicit kinetics, heat transfer, evaporation, venting, or volume
  additivity;
- deterministic reaction closure rather than first-match rule order;
- static, invalid, unsupported, and internal-failure distinctions;
- mandatory stage conservation, capacity, provenance, and trace invariants.

## Claims, holes, assumptions, and proof goals

This chapter defines what source expectations mean, how typed holes request
derivations, how assumptions enter the trusted context, and which proof goals
must be discharged before validation.

### Expectation aggregation

All `expect` blocks elaborate into one `ExpectationSet`. Each block supplies a
target stage; omitted `at` means `final`.

Claims are keyed by normalized claim kind, target stage, and subject. Exact
duplicates are warnings. Two explicit values for the same key that cannot both
hold are `Invalid` once the conflict is established. A hole and explicit value
for the same key are merged: the explicit value supplies the requested answer
and remains subject to proof.

Expectation source order is retained for explanation but does not change truth.

### Claim evaluation windows

Claims fall into two semantic groups.

**Snapshot claims** evaluate the complete state at the target stage:

- `remains`;
- `amount`;
- precipitate presence;
- gas-phase presence;
- colour;
- temperature direction.

**Cumulative outcome claims** evaluate reaction events from `initial` through
the target stage:

- `class`;
- `produces`;
- `consumes`;
- `spectator`;
- molecular, complete ionic, and net ionic equations;
- limiting material.

This makes `expect at final` describe the experiment's outcome even if the last
operation is `stir` or `wait` and the reaction occurred at an earlier stage.

### Claim propositions

#### Reaction class

`class := C` asserts that cumulative supported reaction events through the
target normalize to class `C`.

The reaction classes are precipitation, neutralization, gas formation, and no net
reaction. Multiple non-equivalent primary reaction classes before one target
make a singular class claim `Unsupported`; the author must split the procedure
and target an earlier labelled stage.

`noReaction` requires a positive proof of closure under a catalogue-declared
complete rule domain. Absence of a derived product is insufficient.

#### Identity predicates

For normalized species `S` between `initial` and target:

```text
produces S   iff cumulative chemical production(S) > 0
consumes S   iff cumulative chemical consumption(S) > 0
remains S    iff snapshot amount(S) > 0
spectator S  iff S is present on both sides of an ionic derivation with equal
             coefficient and zero net chemical consumption/production
```

Movement between vessels is not chemical production or consumption. A species
can remain without being a spectator, and a spectator can move or be separated
later.

#### Equation claims

An equation claim asserts equality with the normalized cumulative reaction
equation through the target stage.

The kernel first normalizes coefficients to the smallest positive integral
ratio, canonical species ordering, exact charge, and phase. Multiplying every
coefficient by a common factor does not change equation meaning.

If cumulative events cannot be represented by one unambiguous normalized
equation in the supported domain, an equation claim is `Unsupported` and the
procedure should expose a labelled stage per reaction.

An equation claim at `initial` is `IllTyped` because no reaction history exists.

#### Amount claims

`amount S := q` asserts the total amount of species `S` across all locations in
the target snapshot. `q` must have Amount dimension and be non-negative.

Amount syntax does not select a vessel. A location-specific quantitative
claim requires a future grammar extension.

Equality is exact after unit normalization. Written significant figures affect
display, not claim tolerance.

#### Limiting claim

`limiting := materialName` asserts that the named initial material uniquely
limits the cumulative primary reaction through the target.

`limiting := none` asserts exact stoichiometric equivalence or a supported
no-reaction result. Multiple limiting constraints or coupled reaction networks
outside the solver produce `Unsupported`.

#### Observation claims

`precipitate S` asserts solid species `S` is present at the target snapshot and
was established through a supported precipitation or phase rule.

`gas S` asserts gas species `S` is present at the target or was produced in a
traced cumulative gas event. The derivation records which interpretation
discharged the claim.

`colour := name` asserts the catalogue-backed combined visible colour model for
the target snapshot. If multiple colour contributions lack a supported mixture
model, the claim is `Unsupported`.

`temperatureChange` compares the target vessel state relevant to the reaction
against its initial temperature. When multiple affected vessels have different
directions, the singular claim is `Unsupported`.

Observations are propositions, not free text or renderer commands.

### Explicit values, holes, and omissions

Every permitted claim value has one of three meanings.

#### Explicit value

An explicit value creates a verification goal:

```text
derive actual value
prove actual value = authored value
```

A proven mismatch is `Invalid` and includes the authored value, derived value,
and derivation boundary.

#### Typed hole

`?` creates a synthesis goal whose type is determined entirely by its claim
position. Each hole receives a stable `HoleId` derived from source identity and
span.

```text
class := ?             -> ReactionClass
molecular := ?         -> MolecularEquation
produces ?             -> Species
amount AgCl(s) := ?    -> Quantity<Amount>
limiting := ?          -> MaterialId | None
```

A solved hole is stored in the validated artifact and displayed beside source.
It does not silently rewrite the source file. Applying a generated source patch
is a separate visible editor action.

An unresolved hole yields `Incomplete` only when the goal is within the trusted
supported domain. If the engine cannot decide the chemistry required to fill
it, the result is `Unsupported`.

#### Omitted claim

An omitted claim creates no author-requested goal. It does not assert absence or
permission to omit data from the validated artifact.

Kernel-required artifact and conservation goals are generated independently of
source claims. The simulation still receives a complete supported result.

### Assumption declarations

Each catalogue assumption kind has a schema:

```text
AssumptionKind
  stable id and version
  proposition template
  required target kind
  permitted stage scope
  applicability conditions
  goals it may discharge
  educational explanation
  safety classification
  provenance
```

Source assumption entries elaborate by resolving their kind, target, stage,
and instantiated proposition. Wrong target kinds or missing required targets
are `IllTyped`. Known violated applicability conditions are `Invalid`. Missing
applicability knowledge is `Unsupported`.

Assumptions may supply bounded model premises such as ideal gas behaviour or
negligible volume change. They may not assume:

- atom or charge conservation;
- the truth of an authored outcome claim;
- arbitrary products, equations, or observations;
- the existence or identity of an element or substance;
- permission to bypass safety, catalogue resolution, or the kernel;
- an unrestricted proposition supplied as text.

The proof engine cannot invent an undeclared assumption. When a permitted
assumption could unblock an unsupported goal, diagnostics may suggest the exact
typed source entry, but the user or agent must add it visibly.

Declared but unused assumptions are warnings. Only assumptions that occur in
the final derivation change the result to `ValidatedWithAssumptions`.

### Proof context

The initial proof context contains only:

- language axioms and checked arithmetic;
- typed experiment inputs and operation facts;
- selected catalogue facts applicable under conditions;
- declared assumption propositions whose applicability is established;
- conclusions of already checked derivation nodes.

Agent research, tactic logs, renderer state, source comments, unsupported facts,
and failed candidate rules never enter the proof context.

### Goal model

Every goal has:

```text
Goal
  stable GoalId
  judgment kind
  typed proposition
  target stage or transition
  local proof context
  dependency GoalIds
  source origins
  status
```

Goal statuses are `Open`, `Solved`, `Disproved`, or `Unsupported`. A goal is
immutable; tactic evaluation produces a new proof state with new or solved
goals.

The engine generates these goal families:

| Goal family | Purpose |
| --- | --- |
| Resolve premise | Establish an applicable catalogue fact or assumption |
| Expand analytical species | Derive supported actual species inventory |
| Infer reaction | Establish products or no-reaction closure |
| Normalize/balance equation | Produce canonical stoichiometric coefficients |
| Conserve atoms | Check elemental inventory |
| Conserve charge | Check exact integral charge |
| Solve extent | Compute limiting material and maximum reaction extent |
| Apply operation | Prove a procedure state transition |
| Establish observation | Connect stage state to an observation fact |
| Verify claim | Compare derived and explicit authored values |
| Synthesize hole | Produce a typed requested value |
| Complete artifact | Establish all mandatory validated fields and invariants |

Name resolution and basic typing failures are diagnostics before proof-goal
construction; they are not goals the tactic language can repair.

### Proof script execution

The `by` block is evaluated once, top to bottom, against an immutable proof
state. Each tactic may solve goals, add more specific subgoals, or fail without
changing the prior state.

Tactic selection never changes source experiment state. It changes only which
derivations are attempted and exposed.

A source proof succeeds only when:

- every explicit claim is proved;
- every typed hole is synthesized;
- every operation and stage transition is proved;
- all mandatory conservation and artifact goals are solved;
- no disproved or unsupported goal remains.

Open supported goals after the last tactic yield `Incomplete`. A false goal
yields `Invalid`. A goal outside the catalogue/kernel domain yields
`Unsupported`.

### Proof visibility

The application presents at least:

- the current open goals during validation;
- which tactic is executing;
- newly created and discharged goals;
- catalogue premises and assumptions used;
- explicit versus synthesized claim values;
- the final checked derivation rather than only tactic success messages.

The source proof remains human-readable orchestration. The derivation is the
machine-checkable evidence.

### Decisions fixed by the claims and proof-goal chapter

This chapter fixes:

- aggregation and stage targeting of all expectation blocks;
- snapshot versus cumulative claim windows;
- formal meaning of every claim and observation;
- normalized equation and exact amount-claim equality;
- explicit value, anonymous typed hole, and omission semantics;
- stable holes without silent source rewriting;
- typed catalogue-defined assumptions and their forbidden uses;
- proof-context membership and exclusion rules;
- immutable goal structure, families, statuses, and dependencies;
- ordered tactic execution and exact completion requirements;
- visible proof-state and derivation presentation.

## Trusted kernel, tactics, and derivations

The trusted kernel is the only component that can turn typed experiment data,
catalogue premises, and a completed proof into `ValidatedExperiment`.

### Trusted computing base

The soundness-critical computing base consists of:

- exact integer and rational arithmetic;
- dimension and unit normalization;
- formula, charge, species, and inventory normalization;
- catalogue bundle validation and digest binding;
- the closed set of kernel rules and operation models;
- derivation-node construction and replay checking;
- the private validated-artifact constructor.

The parser, formatter, tactic search strategy, agent, application, renderer, and
simulation are not trusted to assert conclusions. They may propose typed values
or derivations that the kernel rechecks.

Unsafe Rust is forbidden in the kernel crates. Kernel behavior is deterministic
for equal typed input and catalogue digest.

### Core judgments

The specification uses these judgment families:

```text
C |- fact
E; C |- species resolves S
E; C |- material elaborates M
C; Stage[n] |- operation op => ExplicitTransition
C; ExplicitTransition |- reactions => ReactionClosure
C |- equation balances
C |- equation conserves atoms
C |- equation conserves charge
C; Inventory |- reaction extent = x
C; StageHistory |- claim P
C; Goals |- derivation D closes Goals
```

`C` is the validated catalogue and assumption context. `E` is the typed
environment. Every successful judgment produces one derivation conclusion.

### Mandatory kernel goals

Regardless of source tactic spelling, the kernel requires proof of:

- catalogue and language-version binding;
- well-typed inputs, quantities, species, and operation operands;
- every procedure transition;
- elemental and charge conservation for every reaction event;
- non-negative and non-duplicated inventory;
- capacity and condition invariants where applicable;
- stoichiometric consumption and production;
- every authored claim and synthesized hole;
- all fields required by the validated artifact schema.

Named tactics help construct these proofs. Omitting a tactic never disables an
invariant; it leaves the corresponding goal open.

### Reaction-family interface

Each kernel reaction family is a versioned, closed rule implementation with:

```text
ReactionFamily
  stable RuleFamilyId
  applicability domain
  candidate enumeration
  premise schemas
  product construction
  well-founded termination measure
  completeness declaration scope
  derivation rule identifiers
```

The initial families are precipitation, strong acid/base neutralization, and a
curated gas-formation family. No-reaction is a closure judgment, not a family
that matches by default.

### Candidate resolution

For each reaction opportunity, the kernel:

1. expands supported analytical species when requested and justified;
2. enumerates candidates from every applicable selected family;
3. resolves required catalogue premises under exact conditions;
4. rejects inapplicable candidates with recorded reasons;
5. constructs balanced candidate transformations;
6. proves atom and charge conservation;
7. solves maximum exact extent against available inventory;
8. compares normalized outcomes for confluence;
9. applies the unique closure or returns a non-success result.

Outcomes:

| Applicable candidates | Coverage/confluence | Result |
| --- | --- | --- |
| One | Conserved and supported | Apply it |
| Several | Same normalized closure | Apply one canonical closure with all supporting derivations |
| Several | Different closures or competing extents | `Unsupported` |
| None | Rule-domain completeness proves exhaustive coverage | Prove no net reaction |
| None | Coverage is incomplete | `Unsupported` |

Candidate iteration order cannot select chemistry.

### Initial reaction-family rules

#### Precipitation

The family requires:

- supported aqueous actual species;
- applicable dissociation premises where analytical solutes are expanded;
- a catalogue solubility or insolubility fact under current conditions;
- a unique supported solid product identity;
- a balanced conserved net ionic transformation;
- finite available ionic amounts.

The product extent is the minimum available stoichiometric ratio. Spectator
species remain unchanged in actual inventory.

#### Strong acid/base neutralization

The family requires catalogue classification of the participating acid and
base as supported strong electrolytes under conditions, actual `H^+(aq)` and
`OH^-(aq)` availability, and the supported formation of `H2O(l)`.

Weak acids, weak bases, buffers, pH equilibria, and incomplete dissociation are
outside the initial family and produce `Unsupported` rather than approximation.

#### Curated gas formation

The family requires one explicit catalogue reaction pattern whose reactant and
condition premises match, a supported gas product, balanced conservation, and
an applicable phase/closure model.

The family does not infer arbitrary decomposition or redox products. Each
curated pattern has a stable rule identifier and empirical provenance.

#### No net reaction

The kernel proves no net reaction only when the selected catalogue declares the
applicable family set complete for every resolved candidate pair and condition
domain present in the opportunity, and every family proves no applicable
candidate.

Unknown substances, incomplete rule coverage, ambiguous products, or missing
condition facts yield `Unsupported`.

### Equation normalization and conservation

An equation is normalized by:

1. resolving every species;
2. combining duplicate species on each side;
3. moving identical terms across sides and cancelling only where permitted by
   the equation kind;
4. solving positive integral stoichiometric coefficients;
5. dividing coefficients by their greatest common divisor;
6. ordering terms by stable catalogue species key;
7. checking elemental and charge vectors exactly.

Molecular equations preserve supported analytical formula units. Complete
ionic equations expand supported aqueous dissociation. Net ionic equations
remove exact unchanged spectators from the complete ionic equation.

A balanced atom vector with unbalanced charge is invalid, and vice versa.
Neither check is optional.

### Reaction extent

For each reactant `i` with available amount `n_i` and normalized positive
coefficient `nu_i`:

```text
extent_i = n_i / nu_i
maximum_extent = min(extent_i)
```

All arithmetic is exact rational arithmetic. Every reactant attaining the
minimum is limiting. The singular source `limiting` claim is supported only
when one material is uniquely limiting or all relevant minima represent exact
stoichiometric equivalence for `none`.

Consumed and produced amounts are coefficient times maximum extent. Negative
residue or consumption beyond supply is impossible in a checked derivation.

### Derivation DAG

The kernel emits a directed acyclic graph rather than trusting a tactic trace:

```text
DerivationNode
  DerivationNodeId
  rule id and rule version
  typed judgment conclusion
  premise node ids
  catalogue FactIds
  used AssumptionIds
  exact numeric working
  source origins
```

Node identifiers are content hashes of canonical node content. The graph root
is the artifact-completeness judgment. Shared premises appear once and may have
multiple dependants.

Every node can be replayed by a small derivation checker that knows only the
closed kernel rules, exact data types, catalogue digest, and referenced facts.
Provider events and human-readable explanations are not replay inputs.

### Derivation validity

A derivation is valid only when:

- all referenced premise nodes exist and precede the node topologically;
- every referenced catalogue fact belongs to the bound bundle digest;
- every assumption was explicitly declared and applicable;
- the named rule exists at the declared kernel version;
- recomputing the rule yields the exact serialized conclusion;
- the root conclusion matches the complete validated artifact payload.

Modifying source, catalogue digest, rule version, assumption set, or artifact
payload invalidates the derivation digest.

### Tactic semantics

Tactics are untrusted deterministic proof constructors. Each consumes a proof
state and proposes new checked derivation nodes.

#### `dissociate aqueous`

Finds open analytical-to-actual-species goals for aqueous materials and applies
applicable catalogue dissociation facts. It leaves weak, missing, ambiguous, or
condition-inapplicable cases unsupported.

#### `infer products using family`

Restricts candidate search to the named catalogue/kernel rule family, constructs
candidate product and no-reaction subgoals, and submits resulting premises to
the kernel. Naming a family does not assert that it applies.

#### `balance kind`

Balances an existing equation goal of the requested kind using exact integer
linear algebra, normalizes coefficients, and creates mandatory atom/charge
conservation subgoals. It does not infer unknown products.

#### `derive kind`

Derives the requested equation representation from already established
reaction products and premises. Complete ionic derivation requires supported
dissociation; net ionic derivation requires a complete ionic equation and exact
spectator analysis.

#### `cancel spectators`

Cancels species with exactly equal normalized identity and coefficients on both
sides of a complete ionic equation. It cannot cancel merely similar formulae,
different phases, or unequal quantities.

#### `solve stoichiometry`

Computes maximum reaction extent, limiting materials, consumed, produced, and
remaining inventories using an already balanced supported transformation.

#### `verify atoms`

Constructs and compares exact element-count vectors for every open equation or
reaction-event conservation goal.

#### `verify charge`

Constructs and compares coefficient-weighted exact integral charge totals.

#### `prove observations`

Attempts open observation goals using established stage state and applicable
catalogue observation facts. It does not infer a reaction solely from an
expected observation.

#### `close`

Closes goals whose conclusions are already directly available through exact
equality, a checked premise, or a completed dependent derivation. It performs
no reaction-family search and introduces no assumptions.

#### `auto`

Runs the published bounded strategy:

```text
dissociate aqueous
infer products using every applicable initial family
derive molecular
balance molecular
derive completeIonic
cancel spectators
derive netIonic
solve stoichiometry
verify atoms
verify charge
prove observations
close
```

Inapplicable steps are skipped only when no goal of that kind exists. `auto`
cannot broaden catalogue scope, admit assumptions, choose among non-confluent
outcomes, or hide derivation nodes. Its strategy version is recorded.

### Tactic diagnostics

A tactic can report:

- no matching open goal;
- missing prerequisite goal or derivation;
- inapplicable catalogue premise;
- disproved candidate;
- unsupported candidate space;
- ambiguous/non-confluent outcomes;
- goals created, solved, or remaining.

An inapplicable tactic is an error when it prevents completion. A redundant
tactic whose judgment is already solved is a warning and leaves the proof state
unchanged.

### Soundness and tactic independence

Validation depends on the checked derivation, not on a particular tactic script.
Different scripts producing the same canonical derivation root are semantically
equivalent proofs.

No tactic has an “accept,” “trust,” “skip,” “force,” or arbitrary fact-injection
operation. There is no reflection or execution of source-authored code.

### Result precedence

The compiler can emit multiple diagnostics, but chooses one primary result by
this precedence:

1. `Malformed` when a complete source AST cannot be built.
2. `IllTyped` when syntax is meaningful but names, units, dimensions, operands,
   or explicit catalogue references do not elaborate.
3. `Invalid` when a required invariant or authored proposition is proven false.
4. `Unsupported` when a required well-typed judgment cannot be decided inside
   the bound catalogue/kernel domain.
5. `Incomplete` when all encountered judgments are supported but the source
   proof leaves holes or goals open.
6. `ValidatedWithAssumptions` when every goal closes and at least one declared
   assumption occurs in the root derivation.
7. `Validated` when every goal closes without assumptions.

A known contradiction takes precedence over unrelated missing support. The
compiler does not claim `Invalid` for a proposition it could not decide.

Internal kernel faults, corrupt catalogues, failed derivation replay, and
resource-limit exhaustion are explicit system failures rather than chemistry
result states.

### Decisions fixed by the kernel chapter

This chapter fixes:

- the trusted computing base and deterministic kernel boundary;
- mandatory judgments independent of tactic spelling;
- versioned reaction-family and candidate-resolution interfaces;
- initial precipitation, strong neutralization, curated gas, and no-reaction
  proof boundaries;
- confluence requirements and no first-match chemistry;
- equation normalization, conservation, and exact reaction extent;
- content-addressed replayable derivation DAGs;
- exact semantics for every tactic, `close`, and bounded `auto`;
- tactic diagnostics, redundancy, and proof-script equivalence;
- primary result precedence and separation from internal system failures.

## Catalogue, provenance, diagnostics, and source tooling

This chapter defines the empirical trust store, the evidence chain exposed to
users, stable diagnostics, canonical formatting, and the editor/agent protocol.

### Catalogue bundle

A catalogue bundle is an immutable, versioned, self-consistent set of reviewed
empirical facts and kernel rule metadata.

```text
CatalogueBundle
  catalogue schema version
  public name and semantic version
  canonical content digest
  creation metadata
  element registry
  substances and supported species
  media and solvents
  empirical facts
  assumption-kind schemas
  reaction-family coverage declarations
  evidence-source records
```

The bundle is loaded and validated before source evaluation. A source selects
one public name/version; the resolved exact digest is bound into every HIR,
goal, derivation, cache key, and validated artifact.

The application cannot merge runtime model research into the trusted bundle.
Catalogue updates are reviewed build artifacts with new versions/digests.

### Required catalogue records

The catalogue schema supports at least:

- element identities, symbols, atomic numbers, and selected atomic masses;
- stable substance identities, formulae, aliases, and supported phases;
- aqueous species identities and charges;
- medium and solvent identities;
- condition applicability domains;
- dissociation facts;
- solubility/insolubility facts;
- density facts;
- supported gas-model premises;
- observable colour and phase facts;
- curated gas-formation reaction patterns;
- rule-family completeness declarations;
- permitted assumption kinds;
- evidence and review metadata.

Every record has a stable identifier within the bundle schema. Display aliases
never replace stable identity.

### Fact model

An empirical fact is:

```text
CatalogueFact
  FactId
  typed proposition
  condition domain
  evidence source ids
  review status
  reviewer metadata
  schema/rule version
```

Typed propositions are closed schema variants, not text. Examples include
`Dissociates`, `Soluble`, `Insoluble`, `HasDensity`, `HasColour`,
`HasAtomicMass`, and `SupportsGasPattern`.

Overlapping applicable facts with contradictory propositions make the bundle
invalid. The experiment compiler never resolves such a conflict by ordering,
recency, or model preference.

### Condition domains

Facts declare the temperature, pressure, medium, phase, and other supported
preconditions required by their schema. A fact applies only when the kernel can
prove the experiment context lies inside its domain.

Boundary inclusion is explicit. Unknown or partially overlapping ranges do not
apply optimistically. Interpolation or extrapolation requires a separately
specified kernel model and premise.

### Coverage declarations

No-reaction proof requires more than individual facts. A catalogue may declare
that a named reaction-family set is complete for a finite domain:

```text
CoverageDeclaration
  supported substance/species set
  condition domain
  reaction families covered
  exclusions
  evidence/review origin
```

The kernel may prove no net reaction only inside this exact declared domain.
Coverage declarations cannot be inferred from catalogue size.

### Evidence sources

Evidence records contain:

```text
EvidenceSource
  SourceId
  title
  publisher or institution
  stable URL or publication identifier
  publication/revision date where known
  retrieval date
  relevant locator or section
  licence/usage metadata
  review notes
```

The catalogue stores concise typed facts and evidence pointers, not copied
articles. Evidence may support several facts; a fact may cite several sources.

### Review states

Facts have one of:

- `Reviewed`: eligible for validated derivations;
- `Provisional`: visible to catalogue tooling but ineligible for validation;
- `Rejected`: retained only for audit and excluded from published bundles.

Only reviewed facts and coverage declarations enter a production bundle.

### Research provenance boundary

Provider research produces a separate `ResearchResult` containing claims,
citations, retrieval events, and generated source. It can help a human decide
what source to write or which catalogue work is needed.

Research citations do not become kernel premises merely because they are recent
or model-selected. Validation provenance consists only of bound catalogue facts,
kernel rules, and explicit assumptions.

The application presents both chains distinctly:

```text
Agent research: why this source was proposed
Kernel provenance: why this result was trusted
```

### End-to-end provenance

Every displayed empirical conclusion traces:

```text
Validated field
  -> derivation node
  -> kernel rule
  -> Catalogue FactId
  -> Evidence SourceId
```

Arithmetic and conservation nodes trace to language/kernel rules rather than
external empirical evidence. Assumption-dependent nodes trace to both the
source assumption span and its catalogue assumption-kind record.

The application can therefore explain whether a conclusion is mathematical,
catalogue-backed, or assumed.

### Catalogue validation

Bundle loading checks:

- schema version support;
- canonical digest;
- unique stable identifiers;
- element/formula/charge/phase consistency;
- exact unit and dimension validity;
- alias uniqueness within declared namespaces;
- condition-domain validity;
- evidence presence for every reviewed empirical fact;
- absence of contradictory overlapping facts;
- reaction pattern conservation;
- assumption-schema restrictions;
- coverage-declaration consistency;
- deterministic canonical serialization.

A corrupt or inconsistent catalogue is a system failure, not `Unsupported`.

### Diagnostic model

All stages emit one common serializable structure:

```text
Diagnostic
  stable code
  severity
  pipeline stage
  concise summary
  primary source label
  related source labels[]
  chemistry/type explanation
  optional safe fix edits[]
  optional help
  optional GoalIds[]
  optional FactIds[]
  optional unsupported-boundary record
```

Severities are `Error`, `Warning`, and `Information`. Only errors prevent the
current stage from succeeding.

### Diagnostic namespaces

The language reserves:

| Prefix | Producer |
| --- | --- |
| `CHEMS-L` | Encoding, lexing, comments, and layout |
| `CHEMS-P` | Parsing and source-AST construction |
| `CHEMS-T` | Names, quantities, types, formulae, and operation elaboration |
| `CHEMS-C` | Catalogue resolution and applicability |
| `CHEMS-K` | Goals, tactics, kernel judgments, claims, and derivations |
| `CHEMS-F` | Canonical formatting |
| `CHEMS-I` | Internal invariant failures; never ordinary user chemistry results |

Codes are never reused for a different condition within one language major.
Wording may improve without changing a code when machine meaning remains the
same.

### Diagnostic ordering and recovery

Diagnostics are sorted deterministically by primary byte start, severity,
pipeline stage, and code.

The lexer and parser recover sufficiently to produce multiple local diagnostics
and a partial source tree. Elaboration may inspect complete unaffected
declarations for additional diagnostics, but no proof or validated artifact is
constructed from a tree containing error nodes.

One primary cause should not emit unbounded cascades. Related consequences are
labels or notes where possible.

### Fix edits

A fix is a list of non-overlapping UTF-8 byte-range replacements plus the source
digest against which they were computed. Applying a fix to a different digest
is rejected.

Fixes may correct syntax, names, units, indentation, or an explicitly known
claim mismatch. They may not silently add assumptions, change experimental
inputs, broaden catalogue scope, or replace unsupported chemistry with a
fabricated answer.

Agent repairs use the same edit representation.

### Canonical formatter

The formatter is deterministic, idempotent, and comment-preserving:

```text
format(format(source)) = format(source)
parse(format(source)) is semantically equivalent to parse(source)
```

For a well-formed file it:

- emits LF and one final newline;
- emits two spaces per indentation level;
- removes trailing whitespace;
- applies canonical operator spacing;
- emits condition entries as temperature, pressure, medium;
- preserves authored declaration, procedure, expectation, and tactic order;
- preserves identifier spellings and authored decimal trailing zeros;
- normalizes charge magnitude one and equation coefficient one by omission;
- uses canonical ASCII formula, phase, arrow, unit, and micro-prefix forms;
- formats equations inline when they fit and as operator-continuation blocks
  when they exceed 100 columns;
- preserves every comment with a stable syntax-node attachment.

The formatter does not resolve names, alter claims, fill holes, add tactics,
apply migrations, or rewrite empirical meaning.

### Comment attachment

Comments attach during source-AST construction:

- a same-line comment attaches as trailing trivia to the preceding node;
- a full-line comment followed by a declaration without a blank line attaches
  as leading trivia to that declaration;
- comments separated by a blank line attach to the enclosing block at their
  source position;
- block comments between tokens attach to the smallest containing syntax node.

Reordering the three condition entries moves their attached leading/trailing
comments with them. All other source-order preservation keeps comments local.

Formatting malformed source is best-effort editor behavior and has no canonical
guarantee. CLI `format --write` requires a complete parse.

### Source spans and positions

Normative spans are half-open UTF-8 byte ranges `[start, end)`. Line and Unicode
scalar columns are derived views. Editor adapters may additionally expose UTF-16
positions but must round-trip to the byte range.

Synthetic tokens and inserted recovery nodes use zero-width spans anchored at
the nearest source boundary and are marked synthetic.

### Editor protocol

The language service exposes:

- incremental source replacement by version/digest;
- lossless syntax tree and typed semantic tokens;
- parse, type, catalogue, and proof diagnostics;
- goal state and derivation progress;
- completion candidates constrained by grammar and catalogue;
- hover information with type, normalized identity, and provenance;
- safe fix edits;
- canonical formatting;
- source-to-HIR, source-to-goal, and source-to-validated-field mappings.

Every asynchronous response carries the source version and catalogue digest.
The application discards stale responses.

### Agent editing protocol

An agent receives:

- current source and source digest;
- stable diagnostics with spans and fixes;
- open typed goals;
- supported grammar/tactic/catalogue summaries;
- a bounded revision count.

It returns source edits with a precondition digest, not a trusted AST or
validated value. The edited source re-enters the entire pipeline.

Automatic repair stops after three revisions by default. Failure remains
visible as malformed, ill-typed, incomplete, invalid, or unsupported.

### Decisions fixed by the catalogue and tooling chapter

This chapter fixes:

- immutable validated catalogue bundles bound by exact digest;
- required empirical, coverage, assumption, evidence, and review records;
- contradiction rejection and condition-domain applicability;
- strict separation of agent research and kernel provenance;
- end-to-end FactId and SourceId traceability;
- common diagnostics, code namespaces, deterministic ordering, and fixes;
- canonical idempotent formatting and comment attachment;
- byte-based source spans and stale-response rejection;
- one safe edit protocol shared by humans, editor actions, and agents.

## Intermediate representations and validated artifact

The implementation uses distinct representations with explicit schema
boundaries. No representation is a type alias for the next.

### Lossless CST

The concrete syntax tree contains every token, whitespace range, comment,
layout token, error node, and source span. It is optimized for editing and is
not a stable interchange format.

### Source AST

The source AST removes irrelevant punctuation structure while retaining:

```text
SourceDocument
  language version syntax
  catalogue selection syntax
  SourceExperiment
    names and declarations
    source quantity/formula/species syntax
    procedure syntax
    claims and holes
    assumptions
    tactic syntax
  comments/trivia attachments
  source-origin map
```

It may contain unresolved names, malformed recovery nodes, and holes. It is
serializable for golden parser fixtures but is versioned as a compiler schema,
not a public validated contract.

### Typed HIR

The `TypedExperiment` HIR contains no unresolved language construct:

```text
TypedExperiment
  hir schema version
  language major version
  source digest
  catalogue identity and digest
  ExperimentId
  typed Environment
  declared Assumptions
  MaterialIds and typed materials
  VesselIds and typed initial vessels
  typed finite operations and StageIds
  stage-targeted typed claims
  HoleIds and expected types
  typed tactic program
  complete source-origin map
```

It can still be incomplete, invalid, or unsupported because proof has not yet
established its chemistry.

### Proof state and derivation

Proof state is an internal persistent value containing goals and candidate
checked nodes. It is serializable only for debugging/evaluation fixtures and is
not accepted as a trusted result.

The derivation DAG is a stable component of the validated artifact and can be
replayed independently.

### Validated artifact

The public artifact schema is:

```text
ValidatedExperiment
  artifact schema version
  language version
  compiler/kernel version
  source digest
  catalogue name, version, and digest
  ExperimentId
  ValidationKind
  exact normalized initial environment
  exact normalized initial material inventories
  resolved logical vessels
  typed procedure
  complete immutable Stage timeline
  ReactionEvents and operation events
  normalized equations
  exact reaction extents
  consumed/produced/remaining/spectator inventories
  evaluated explicit claims
  synthesized hole values
  used assumptions
  observation results
  derivation DAG and root id
  source-origin map
  catalogue-provenance map
  artifact content digest
```

`ValidationKind` is `Validated` or `ValidatedWithAssumptions`. Other result
states have diagnostic/result envelopes but never a `ValidatedExperiment`.

### Private construction

In Rust, artifact fields are not publicly constructible. Deserialization into a
trusted artifact always performs:

- schema/version checks;
- canonical data validation;
- catalogue digest binding;
- derivation replay;
- root/payload equality;
- artifact digest verification.

Unchecked fixture or network JSON cannot become a simulation input merely by
matching field names.

## Stable identities

The schemas use typed identifiers:

| Identifier | Construction |
| --- | --- |
| `ExperimentId` | Source digest plus declared experiment name |
| `MaterialId` | ExperimentId plus material declaration identity |
| `VesselId` | ExperimentId plus vessel declaration identity |
| `OperationId` | ExperimentId plus procedure ordinal/source origin |
| `StageId` | ExperimentId plus stage ordinal and optional label |
| `HoleId` | ExperimentId plus hole source origin and expected type |
| `GoalId` | Content hash of normalized goal context/proposition |
| `ReactionEventId` | Content hash of stage, opportunity, and closure |
| `DerivationNodeId` | Content hash of canonical derivation node |
| `FactId` | Stable identifier declared by a catalogue bundle |
| `SubstanceId` | Stable identity declared by a catalogue bundle |

Identifiers are opaque in public APIs. Display names are separate and may not
be used as substitute identities.

Source-origin-dependent identifiers are stable only for one source digest.
Edits intentionally produce new identities and stale prior results.

## Canonical serialization

Golden fixtures, cache keys, derivation hashes, and artifacts use canonical
UTF-8 JSON with:

- lexicographically sorted object keys;
- schema-defined array ordering;
- no insignificant whitespace;
- tagged enum objects with a `kind` field;
- arbitrary integers and rational numerator/denominator encoded as decimal
  strings;
- no JSON floating-point numbers for chemistry values;
- absent optional fields omitted rather than serialized as ambiguous defaults;
- normalized LF inside retained source excerpts.

Content digests use SHA-256 over canonical bytes and are encoded lowercase
hexadecimal. The digest algorithm identifier is stored with external artifacts
so a future schema can add another algorithm without ambiguity.

Human-readable pretty JSON is a presentation of the same data but is never the
hashed byte sequence.

## Schema versions

These versions evolve independently:

```text
language major version       selected by `chems 1`
catalogue schema/version     selected and digest-bound by `use catalog`
source AST schema version    compiler/testing representation
HIR schema version           compiler/kernel boundary
artifact schema version      kernel/simulation boundary
kernel rule version          derivation replay semantics
diagnostic catalogue version machine-facing diagnostics
```

Every serialized boundary carries the relevant versions. Consumers reject an
unknown incompatible version with an explicit diagnostic rather than attempting
best-effort interpretation.

## Language compatibility policy

Within `chems 1`:

- accepted source syntax cannot be reassigned a different meaning;
- reserved words cannot be removed or repurposed;
- a previously valid program cannot silently select a broader empirical domain;
- canonical formatting and diagnostic wording may improve while preserving AST
  meaning and stable codes where applicable;
- compiler bug fixes that change a result record a new kernel/compiler version
  and conformance note;
- adding catalogue facts creates a new catalogue version/digest, not a language
  change;
- adding syntax, operations, claim forms, phases, or proof powers requires a new
  language major unless explicitly reserved as compatible by this specification.

Breaking evolution uses `chems 2`. A compiler may support several majors but
parses each under its selected grammar and semantics.

## Conformance contract

A `.chems` implementation is conforming only when it passes the normative
conformance suite for its claimed components.

The suite categories are:

| Category | Required evidence |
| --- | --- |
| Encoding/layout | Accepted/rejected source and exact lexical diagnostics |
| Parsing | Golden lossless/source AST and recovery diagnostics |
| Formatting | Golden canonical source, idempotence, comment preservation |
| Quantities/types | Exact rational values, dimensions, conversions, failures |
| Formula/species | Normalized composition, identity, charge, phase, catalogue outcomes |
| Materials | Constructor selection, premises, exact inventories |
| Procedures | Stage timelines, ledgers, operation preconditions and failures |
| Claims/holes | Goal generation, explicit comparisons, synthesized values |
| Kernel/tactics | Golden derivation nodes, tactic effects, unsupported/invalid boundaries |
| Catalogue | Bundle validation, digest binding, FactId provenance |
| Artifacts | Canonical JSON, digest, replay, private-construction rejection |
| End to end | Complete result envelope for reviewed experiments |

Every case declares:

```text
case id
specification requirement ids
source and catalogue fixture
expected primary result
expected diagnostic codes, severities, and exact primary/related spans
optional expected canonical formatted source
optional expected AST/HIR
optional expected derivation/artifact
```

Expected chemistry results are independently reviewed and are not generated by
the engine under test.

### Mandatory conformance experiments

The first complete suite includes:

- silver nitrate plus sodium chloride precipitation;
- hydrochloric acid plus sodium hydroxide neutralization;
- hydrochloric acid plus sodium bicarbonate gas formation;
- potassium nitrate plus sodium chloride supported no net reaction;
- unsupported valid species/domain;
- disproved authored equation;
- incomplete hole/proof;
- assumption-dependent gas or volume calculation;
- multi-stage placement, stirring, filtration, and decanting;
- invalid capacity, duplicate inventory use, and wrong operation operands.

### Metamorphic requirements

Implementations also prove through tests:

- compatible unit conversions preserve semantic quantities;
- formula grouping/adduct alternatives normalize identically where specified;
- swapping operands of atomic `combine` preserves its result;
- multiplying every equation coefficient by a common positive integer preserves
  normalized equation meaning;
- formatting is idempotent and semantics-preserving;
- comments never alter semantics;
- source or catalogue digest changes stale prior artifacts;
- reactant inventory cannot be consumed beyond supply;
- invalid, unsupported, and incomplete never produce artifacts;
- derivation replay rejects any changed premise, node, or artifact field.

### Property and fuzz requirements

The implementation plan includes:

- lexer/parser fuzzing over arbitrary UTF-8;
- parse/format/parse property tests;
- exact unit conversion round trips;
- formula normalization properties;
- arbitrary balanced-equation conservation checks;
- inventory/state-machine transition generation;
- derivation DAG mutation rejection;
- no-panic/no-unsafe checks across untrusted source and catalogue files.

## Specification requirement identifiers

Every normative section has a stable requirement identifier in
[`conformance/requirements.json`](../conformance/requirements.json), for example
`LEX-001`, `TYP-014`, `STA-008`, and `KER-021`. Every rule below a mapped heading
inherits that identifier until the next mapped heading. Identifiers remain
stable within `chems 1` and are referenced by tests and design changes.

The prose in this document remains normative; identifiers make coverage
auditable rather than replacing the prose.

## Specification completion criteria

The `.chems` design is complete when:

- every chapter in the status table is locked;
- the normative EBNF has no unresolved production;
- representation and trust boundaries are explicit;
- every source construct has typing and semantic rules;
- every tactic and result state has deterministic meaning;
- empirical conclusions have a catalogue/provenance path;
- canonical IR and artifact schemas are defined;
- versioning behavior is fixed;
- the conformance contract covers every normative requirement;
- implementation slices have dependencies and acceptance gates.

This completes the language design. Slice 0 makes its structure executable; the
compiler and kernel are implemented by later slices.

### Decisions fixed by the IR, compatibility, and conformance chapter

This chapter fixes:

- distinct CST, source AST, typed HIR, proof, derivation, and artifact boundaries;
- complete validated-artifact contents and private checked construction;
- typed identity and content-addressing rules;
- canonical no-float JSON and SHA-256 digests;
- independent schema/kernel/catalogue/language versioning;
- strict `chems 1` compatibility;
- component and end-to-end conformance categories;
- mandatory chemistry, metamorphic, property, and fuzz coverage;
- stable normative requirement identifiers and specification completion gates.

## Implementation handoff

The language-design chapters are complete and coherent. Implementation proceeds
through the dependency-ordered slices in the
[implementation plan](chems-implementation-plan.md), beginning with the
conformance and requirement-ID scaffold.
