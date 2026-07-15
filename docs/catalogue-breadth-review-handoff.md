# Catalogue breadth review handoff

Status: implementation complete for all four families in the fixed queue
(`docs/implementation-plan.md`, "Next catalogue breadth"). This document is
the exact digest-bound review request for Codex's independent AI review,
attestation, and promotion. It was produced by the catalogue implementer
role, not the reviewer; nothing in this document or the underlying commits
constitutes review, attestation, or promotion.

## Commits produced

On branch `expand-catalogue`, each family and the execution plan committed
separately, in this order:

| Commit | Subject |
| --- | --- |
| `2060980` | Add catalogue breadth execution plan for the four-family queue |
| `183afd7` | Add precipitation catalogue family: silver halide from silver nitrate and sodium halide |
| `1be771d` | Add acid-base neutralization catalogue family: monoprotic acid with alkali-metal hydroxide |
| `1848338` | Add acid-bicarbonate gas-evolution catalogue family: monoprotic acid with alkali-metal bicarbonate |
| `e6dad19` | Add single-displacement catalogue family: alkali-metal activity-series displacement |

No commit modifies Rust source, `Cargo.toml`, `grammar/chems.ebnf`, an
existing schema, an existing conformance fixture, `catalogue/trusted/`, or
any other candidate package's files. Every change is new candidate content
under `catalogue/candidates/<family-id>/`, one new test file section in
`crates/chems-cli/tests/authoring.rs`, `docs/catalogue-breadth-execution-plan.md`,
`catalogue/candidates/README.md`, and this handoff.

## The four implemented families

### 1. Precipitation — `catalogue/candidates/precipitation-silver-halide/`

`Rules.SilverHalidePrecipitation`. Finite domain: one parameter
`X : Categories.Halide`.

- **Supported**: `X ∈ {Cl, Br, I}` — `AgNO3(aq) + NaX(aq) -> AgX(s) + NaNO3(aq)`.
  Pure ionic re-association (`dissociate_ionic` x2, `associate_ionic` x2, no
  covalent or electron change).
- **Explicit unsupported case**: `X = F` — silver fluoride is soluble, not a
  precipitate (`Features.SolubleHalideException`).
- Worked example: `AgNO3 + NaCl -> AgCl + NaNO3`.

### 2. Acid-base neutralization — `catalogue/candidates/acid-base-neutralization/`

`Rules.MonoproticAcidHydroxideNeutralization`. Finite domain: parameters
`member : Categories.AlkaliMetal`, `halide : Categories.Halide`.

- **Supported**: `member ∈ {Li, Na, K}`, `halide ∈ {Cl, Br, I}` (9 combinations,
  one uniform case) — `HX(aq) + MOH(aq) -> MX(aq) + H2O(l)`. Mechanism:
  `cleave_covalent` (H-X heterolytic to X) then `transfer_electron` +
  `form_covalent` (hydroxide oxygen to the freed proton, producing an
  ordinary shared bond so the product structurally equals the existing
  trusted `Water`) then `associate_ionic`.
- **Explicit unsupported case**: `halide = F` — hydrofluoric acid is a weak
  acid (partial-dissociation equilibrium), a different reaction model
  (`Features.WeakAcidEquilibrium`).
- Worked example: `HCl + NaOH -> NaCl + H2O`.

### 3. Acid-carbonate gas evolution — `catalogue/candidates/acid-bicarbonate-gas-evolution/`

`Rules.MonoproticAcidBicarbonateGasEvolution`. Finite domain: parameters
`member : Categories.AlkaliMetal`, `halide : Categories.Halide`.

- **Supported**: `member ∈ {Li, Na, K}`, `halide ∈ {Cl, Br, I}` (9
  combinations, one uniform case) — `HX(aq) + MHCO3(aq) -> MX(aq) + H2O(l) + CO2(g)`.
  Mechanism (8 rewrite operations, dative-free — see "Design findings"
  below): acid cleavage, protonation of the bicarbonate's charged oxygen
  (`transfer_electron` + `form_covalent`), heterolytic cleavage of the C-OH
  bond that will leave as water, a second cleavage that frees the newly
  added proton again, a `change_covalent` single-to-double bond-order
  increase that completes the second C=O of carbon dioxide, a second
  `transfer_electron` + `form_covalent` that completes water, then
  `associate_ionic` for the salt.
- **Explicit unsupported case**: `halide = F`, same weak-acid reason.
- **Explicit unsupported boundary (documented, not a rule parameter)**: fully
  deprotonated carbonate salts (Na2CO3, K2CO3) are out of scope for this
  rule; they need one additional protonation cycle, a straightforward but
  deliberately deferred extension.
- Worked example: `HCl + NaHCO3 -> NaCl + H2O + CO2`.

### 4. Single displacement — `catalogue/candidates/single-displacement-alkali-metal/`

`Rules.AlkaliMetalActivitySeriesDisplacement`. Finite domain: parameters
`member : Categories.AlkaliMetal` (displacing), `displaced : Categories.AlkaliMetal`,
`halide : Categories.Halide`.

- **Supported**: the 3 reactivity-ordered pairs `(K,Na)`, `(K,Li)`, `(Na,Li)`
  (following K > Na > Li) times all 4 halides `{Cl, Br, I, F}` = 12
  combinations, one case — `member(s) + displaced-X(aq) -> member-X(aq) + displaced(s)`.
  Mechanism: `dissociate_ionic`, `release_metallic` (displacing metal, mirrors
  the existing `Rules.AlkaliMetalWithWater` fixture exactly), `transfer_electron`,
  `join_metallic` (displaced metal forms its own fresh single-site domain),
  `associate_ionic`.
- **Uncovered (implicitly Unsupported, no case needed)**: same-element pairs
  and every reversed (less-reactive-displaces-more-reactive) pair.
- **Scope boundary, documented as a design finding, not silently
  narrowed** — see below.
- Worked example: `K + NaCl -> KCl + Na`.

## Design finding: divalent metallic redox is not expressible with the current operation set

While designing family 4, the plan (`docs/catalogue-breadth-execution-plan.md`,
family 4 section) originally targeted the conventional Zn/Fe/Cu/Mg activity
series. Direct inspection of `chem-kernel`'s `release_metallic`/`join_metallic`
semantics (`crates/chem-kernel/src/validate.rs`) established that:

- `ReleaseMetallic` always removes the named site from its domain in the same
  call it credits exactly one electron to the departing atom; a site cannot
  be released twice.
- `JoinMetallic` always adds the named site to its domain in the same call it
  consumes exactly one local unpaired electron from the joining atom; a site
  cannot join twice.
- Consequently no single metallic site can end a reaction with a domain
  membership change of more than one electron. A monovalent metal (one
  delocalized electron per site — every alkali metal) is fully expressible.
  A divalent metal (two delocalized electrons per site — Zn, Fe(II), Cu, Mg,
  the metals conventionally used to teach the activity series) is not: there
  is no operation sequence that extracts or deposits a second electron for
  the same site without either losing an electron from the kernel's own
  per-operation conservation check (`validate_conservation`) or inventing a
  non-standard, unreviewable electron distribution.
- This was cross-checked against the existing (declared but never exercised)
  `CalciumMetal` divalent fixture already in `periodic-table-and-alkali-water`:
  it carries `metallic_domain_states` for a divalent site but no rule ever
  uses it, consistent with this being a real, previously-uncrossed boundary
  of the kernel rather than a gap specific to this queue.

Per the goal's guidance to design conservatively and to document rather than
invent semantics, family 4 was scoped to alkali-metal-on-alkali-metal-halide
displacement instead — genuinely bounded, reuses only already-reviewed
monovalent metallic machinery, and is honestly framed in its own premise as a
theoretical simulation of the activity-series principle (not a literal
recommended aqueous procedure; alkali metals also react with water, a
confound the premise text explicitly flags rather than silently ignoring).
Divalent-metal single displacement remains available as a future extension
once a multi-electron (or per-electron-incremental) metallic-transfer
operation is designed and reviewed — that is new kernel design work, out of
this queue's "candidate content only" scope.

## Design finding: `form_dative` cannot produce a bond that survives to a plain covalent product

Families 2 and 3 originally planned to use `form_dative` for proton-transfer
steps (matching the wording of `docs/generalized-chemistry-rules.md`'s worked
dative example). Iterating against `cargo run -p chems-cli -- catalogue
check` surfaced `CHEMS-K053: final covalent graph disagrees with declared
products`: a dative bond's directed `electron_origin` annotation is part of
its structural identity (`chem_domain::CovalentBond`'s `Eq` includes
`electron_origin`), so a dative-formed O-H bond never equals the plain shared
O-H bonds declared on the existing trusted `Water` structure. Both families
were redesigned to use `transfer_electron` + `form_covalent` for any new bond
that must survive to a final product (exactly the pattern
`Rules.AlkaliMetalWithWater` already uses for its H-H bond), reserving
`form_dative`/`cleave_dative` for bonds that form and are cleaved again
within the same rewrite (none of the four families ended up needing that).
`docs/catalogue-breadth-execution-plan.md` was updated to match the
implemented mechanism.

## Newly participating elements

Before this queue, only Li, Na, and K had any executable reaction coverage
(via `Rules.AlkaliMetalWithWater`); H and O had coverage through that same
family. This queue adds executable coverage for:

- **Ag** (silver) — precipitation only.
- **N, O** (nitrate/carbonate contexts) — O already had water/hydroxide
  coverage; this queue adds its carbonyl/carbonate/bicarbonate states. N is
  new (nitrate).
- **C** (carbon) — new: bicarbonate and carbon dioxide.
- **Cl, Br, I** — new: precipitation, acid-base, gas evolution, and
  single-displacement anion coverage.
- **F** — new: appears in every family's finite parameter domain, reaching
  an executable *supported* case only in family 4 (single displacement,
  where it is chemically inert to the redox mechanism); every other family
  reaches it only through an explicit reviewed `unsupported` case.

No new element identity records were added (`docs/generalized-chemistry-rules.md`'s
118-element registry is unchanged) and none of the 118 identity records were
duplicated; every family references the shared registry and the shared
`Categories.AlkaliMetal` category by ID.

## Evidence sources and premise coverage

Four new evidence sources, one per family package, all OpenStax *Chemistry
2e* (retrieved 2026-07-15), each scoped to the specific sections that
support that family's claims (see each package's `candidate.json` `evidence`
array and `docs/catalogue-breadth-execution-plan.md`'s "Sources" section for
exact locators). Every premise in every new package is `provisional` with an
empty reviewer list, as required for candidate content — none self-asserts
review. The full set of premise IDs bound by the merged catalogue is listed
in the `premises` array of the generated `review-request.json` (reproduced
below).

## Candidate package paths

```
catalogue/candidates/periodic-table-and-alkali-water/   (existing, unchanged)
catalogue/candidates/precipitation-silver-halide/
catalogue/candidates/acid-base-neutralization/
catalogue/candidates/acid-bicarbonate-gas-evolution/
catalogue/candidates/single-displacement-alkali-metal/
```

Each new package contains exactly `candidate.json`, `example.chems`, and
`evidence.json`.

## Exact merged semantic digest

Generated with all five packages, order-independently (verified by running
the command below with packages in both forward and fully reversed order and
diffing `catalogue.digest`):

```
catalogue_digest: 304253cf4def649a9a27d153d5f7239780f03990bbabf3de820169cf6cc51c1f
```

Reproduce with a fresh empty output directory:

```sh
cargo run -p chems-cli -- catalogue check --out <new-empty-directory> \
  catalogue/candidates/periodic-table-and-alkali-water \
  catalogue/candidates/precipitation-silver-halide \
  catalogue/candidates/acid-base-neutralization \
  catalogue/candidates/acid-bicarbonate-gas-evolution \
  catalogue/candidates/single-displacement-alkali-metal
```

This writes `<out>/catalogue.json`, `<out>/catalogue.digest`,
`<out>/review-request.json` (status `pending-ai-review`, `promotable: false`),
and `<out>/inspections/<package-id>/{expanded-certificate,derivation,frames}.json`
per package. This command was **not** run to produce a committed artifact —
review-request and inspection artifacts are generated output, deliberately
not hand-edited or committed (`docs/luna-catalogue-handoff.md`), and are
reproducible exactly from the digest above.

## Generated review-request content (reproduced for convenience)

```json
{
  "schema_version": 1,
  "status": "pending-ai-review",
  "promotable": false,
  "catalogue_digest": "304253cf4def649a9a27d153d5f7239780f03990bbabf3de820169cf6cc51c1f",
  "required_external_artifact": "chem-catalogue-review-1 attestation",
  "promotion_boundary": "Only an exact host-selected AI attestation accepted by the host-pinned TrustedCatalogue API can promote this digest."
}
```

Evidence sources bound (7): `evidence.iupac.goldbook`,
`evidence.iupac.periodic-table`, `evidence.openstax.chemistry-2e`,
`evidence.openstax.chemistry-2e.acid-base`,
`evidence.openstax.chemistry-2e.activity-series`,
`evidence.openstax.chemistry-2e.gas-evolution`,
`evidence.openstax.chemistry-2e.solubility`.

Premises bound (36): every premise listed in each family section above plus
the shared `premise.elements.iupac-periodic-table`,
`premise.rule.alkali-metal-water.standard-outcome`, and
`premise.category.halide`. The full list is in the generated
`review-request.json`'s `premises` array (reproduce via the command above).

### Inspection-artifact digests

| Package | expanded_certificate | derivation | frames |
| --- | --- | --- | --- |
| `periodic-table-and-alkali-water` | `230ec42352092352ffe1d148454a40bb9c3c30a70b12697d395e11f3b90c6f47` | `d1b318550999f0be74087e33f0fb4b894c3d6d00f81610ae608a4b3113e9bce3` | `8fbb17715b7a310ac4ad608d049a0175f3ae96a7ed1f11c4d38b9249eda8b8ce` |
| `precipitation-silver-halide` | `2ecf0bd72463a1b015c6c139e9a95fcce1260c898c882910994b8be53d201347` | `d3b6febf78962651134c1c93fd5eba159b54d5cd8b29d63ebba27fd9ecba878d` | `4790be815e7c9feb56b0b28a9942fc959f95a86b73ba8213715e34988ed08a86` |
| `acid-base-neutralization` | `8c4712fc1cce9710ffd2b622ba7087afd8d87861bbb20b4f1edfef1f5766cb38` | `ddccbd1fb1b6ea70b6fce6c6ed4e0315f3e3af0747e3bf65aba2171c1029b9fa` | `120f3ac768b20d9432417e48a4786ccbff7fca326358c6aa8e854381a6ff63b2` |
| `acid-bicarbonate-gas-evolution` | `20169dfea488333941b3f4e9d299db7db5de241d599f5a3b62389c8272d3b7c2` | `c1f46e4be0bf08378370537a3dc522ab58d5c5c15c4fc420930e86062f742782` | `2f008f101ef64ae14b679f1a30099f4cbdf2b728b12f091113214faaf2fa8add` |
| `single-displacement-alkali-metal` | `322ade43111c1645961dccdc82384d74d30a83c28dc8b97ec769192fbcec3a77` | `29102bc26450c67564c550e166c8877790cdddf4bce3ad07ef8b564b70eed358` | `013168fbdb24a4943ebe9b32a387026a8345320252a16430121a12664bcf9fc7` |

All labelled `"status": "candidate-inspection-only"`, `"promotable": false` —
none is an independent conformance oracle and none can promote its own
producing implementation.

## Focused and repository-wide commands, with results

Every command below was run from the repository root on branch
`expand-catalogue` and passed.

### Focused (per family, cumulative)

```sh
cargo run -p chems-cli -- catalogue check --out <tmp> \
  catalogue/candidates/periodic-table-and-alkali-water \
  catalogue/candidates/precipitation-silver-halide                       # pass
cargo run -p chems-cli -- catalogue check --out <tmp> \
  catalogue/candidates/periodic-table-and-alkali-water \
  catalogue/candidates/precipitation-silver-halide \
  catalogue/candidates/acid-base-neutralization                          # pass
cargo run -p chems-cli -- catalogue check --out <tmp> \
  catalogue/candidates/periodic-table-and-alkali-water \
  catalogue/candidates/precipitation-silver-halide \
  catalogue/candidates/acid-base-neutralization \
  catalogue/candidates/acid-bicarbonate-gas-evolution                    # pass
cargo run -p chems-cli -- catalogue check --out <tmp> \
  catalogue/candidates/periodic-table-and-alkali-water \
  catalogue/candidates/precipitation-silver-halide \
  catalogue/candidates/acid-base-neutralization \
  catalogue/candidates/acid-bicarbonate-gas-evolution \
  catalogue/candidates/single-displacement-alkali-metal                  # pass
```

Every one of the above was also re-run with package arguments in fully
reversed order; `catalogue.digest` was identical each time.

```sh
cargo test -p chems-cli --test authoring     # 12 passed; 0 failed
cargo test -p chem-catalogue                 # 73 unit/integration tests across 6 binaries; 0 failed
```

`crates/chems-cli/tests/authoring.rs` gained 8 new tests: one positive
merged-check + digest-order-independence test and one negative
(`UnsupportedChemistry`) test per family — `precipitation_candidate_checks_with_the_base_package_and_covers_the_halide_domain`,
`silver_fluoride_remains_unsupported_rather_than_precipitating`,
`acid_base_candidate_checks_with_prior_packages_and_reuses_the_salt_template`,
`hydrofluoric_acid_remains_unsupported_as_a_weak_acid`,
`gas_evolution_candidate_checks_with_prior_packages_and_reuses_shared_salts`,
`hydrofluoric_acid_remains_unsupported_for_gas_evolution_too`,
`displacement_candidate_checks_with_all_prior_packages_and_reuses_shared_structures`,
`less_reactive_metal_cannot_displace_a_more_reactive_one`.

### Repository-wide (final gate)

```sh
cargo fmt --all -- --check                                    # clean
cargo test --workspace --all-targets                          # 0 failed (25 test binaries)
cargo clippy --workspace --all-targets -- -D warnings          # clean (0 warnings from this crate's code)
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps     # clean
cargo test --workspace --doc                                  # 0 failed
cargo run -p chems-conformance -- validate                     # exit 0; 24 cases, 4 incomplete (pre-existing, unrelated to this queue — see Warnings below)
git diff --check                                               # clean
```

## Warnings, unresolved chemistry, and schema limitations

- `cargo clippy --workspace --all-targets` emits one pre-existing, unrelated
  future-incompatibility notice for the `block v0.1.6` dependency (used by
  the `chemspec-app` GUI crate's platform bindings, not touched by this
  queue).
- `cargo run -p chems-conformance -- validate` reports "4 incomplete" out of
  24 manifest cases; this is the pre-existing conformance-registry state from
  before this queue (unrelated components — this queue adds no conformance
  manifest entries, since G6's "candidate content only" scope explicitly
  excludes conformance-registry changes until a maintainer promotes and
  registers the new families, see "Steps for Codex" below).
- Two explicit, reviewed domain gaps recorded as `unsupported` cases:
  hydrofluoric acid (families 2 and 3) and silver fluoride (family 1).
- One explicit, reviewed domain boundary recorded in prose rather than a
  rule parameter: full (non-bicarbonate) carbonate salts are out of scope
  for family 3.
- One explicit, reviewed scope boundary recorded as a design finding rather
  than a rule parameter: divalent-metal single displacement is out of scope
  for family 4, with the exact kernel-level evidence above. This is the one
  place this queue's implementation diverges from the originally
  more-ambitious plan; the four families as *actually built* are still
  genuinely generalized, structurally exact, and fully executable.
- No new Rust code, schema, or kernel operation was added or proposed; every
  family expands entirely within the existing closed structural operation
  set.

## Exact steps for Codex

1. Independently review the exact digest
   `304253cf4def649a9a27d153d5f7239780f03990bbabf3de820169cf6cc51c1f`,
   regenerated via the command in "Exact merged semantic digest" above, plus
   this handoff's chemistry, evidence, and design-finding sections.
2. If accepted, author a `CatalogueReviewAttestation` (schema in
   `crates/chem-catalogue/src/model.rs`'s `CatalogueReviewAttestation`,
   pattern in `catalogue/reviews/periodic-table-and-alkali-water.review.json`)
   bound to exactly this catalogue digest and covering every premise ID in
   the generated `review-request.json`'s `premises` array (36 premises) and
   every evidence source in its `evidence_sources` array (7 sources).
3. Run `cargo run -p chems-cli -- catalogue promote --out <dir> --attestation <review.json> <all five package directories>`
   to produce `catalogue.json`, `catalogue.digest`, `review.json`, and
   `promotion.json` (status `host-selected-ai-reviewed`).
4. Deliberately update the compiled trust roots
   (`PINNED_CANONICAL_CATALOGUE_DIGEST` / `PINNED_CANONICAL_REVIEW_DIGEST` in
   `crates/chem-catalogue/src/lib.rs`) and the committed
   `catalogue/trusted/` artifacts to the newly promoted digest pair, exactly
   as was previously done for `periodic-table-and-alkali-water`. This is the
   only step that can change what `TrustedCatalogue::from_canonical_json`
   accepts; this queue's implementation cannot and does not perform it.
5. Add trusted conformance fixtures for the four new families (independent
   positive fixtures per `docs/verification.md`'s "Chemistry-engine
   verification" table, e.g. the existing `AgNO3 + NaCl -> Precipitation` and
   `HCl + NaHCO3 -> Gas formation` rows already anticipate exactly this
   content) under `crates/chems-conformance`'s manifest, independently
   authored rather than generated from this queue's own inspection
   artifacts (`docs/chems-specification.md`'s conformance contract requires
   this independence).
6. Only after trust-root promotion and conformance registration, wire the
   four new families into `chemspec-app` (reactant/product recognition,
   guided animation, narration) the same way Li/Na/K's water reaction is
   wired today — this queue makes no application changes, consistent with
   "Do not... change unrelated UI."

## Goal completion condition

All four documented families are implemented as executable candidate
content; the merged five-package candidate check succeeds deterministically
and order-independently; every repository-wide gate above passes; the five
commits are cleanly separated by family (plus the execution plan); and this
exact digest-bound review handoff exists. No self-attestation, promotion, or
trust-root change was performed or claimed.
