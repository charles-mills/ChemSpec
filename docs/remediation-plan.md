# Remediation plan

A full fix plan from the July 2026 architecture review of the catalogue,
authoring pipeline, and application. Phases are ordered by leverage: deletion
first, then the seams that block LLM integration, then data-format and
code-structure debt. Each phase is independently shippable; later phases
assume earlier ones only where stated.

Verification baseline for every phase: `just ci` (fmt, clippy `-D warnings`,
`cargo test --workspace`, conformance validate) plus a release build of
`chemspec-app` that loads the trusted catalogue and plays one experience
end to end.

---

## Phase 0 — Delete dead weight

Zero-risk removals. Nothing in this phase changes behaviour; every item was
verified to have no callers or references.

### 0.1 Committed build artifacts (~21 MB)

Remove from the repo root:

- `reaction-rules-check-1/`
- `reaction-rules-check-3/`
- `reaction-rules-check-final/`

These are `chems catalogue check --out` outputs (75 tracked files). They are
referenced by nothing, and their digests do not match the pinned trusted
digest — stale snapshots of review iterations of the same catalogue.

Add to `.gitignore`:

```gitignore
/reaction-rules-check*/
/reaction-rules-promoted*/
```

### 0.2 Superseded trusted bundle (~4.3 MB)

Remove `catalogue/trusted/periodic-table-and-alkali-water/`. Its 103 rule IDs
are a strict subset of `core-chemistry`'s 138, and nothing in crates, docs, or
tools references it. `core-chemistry` is the sole production bundle.

### 0.3 Dead chem-domain modules (~2,500 lines)

Remove from `crates/chem-domain/src` (each has zero users outside chem-domain
and its own tests; `docs/product-spec.md` explicitly excludes what they
model — quantities, apparatus, vessels, timed steps):

- `unit.rs` (1,180 lines — exact dimensional analysis, temperature scales)
- `material.rs`
- `state.rs` (vessels, stages, mixing, `ReactionRuleFamily`)
- `scalar.rs` (`ExactScalar`, `SourceDecimal`)
- `formula.rs`: `NormalizedFormula` / `FormulaSyntax` only (keep the rest)
- The 14 orphaned ID kinds: `VesselId`, `StageId`, `HoleId`, `GoalId`,
  `ExperimentId`, `MediumId`, `FactId`, `CoverageId`, `SpeciesId`,
  `SubstanceId`, `MaterialId`, `InventoryPortionId`, `ReactionOpportunityId`,
  `ReactionEventId`

Also remove the matching orphaned conformance fixture directories that are
absent from `conformance/manifest.json`: `materials/`, `quantities-types/`,
`procedures/`, `claims-holes/`, `kernel-tactics/`.

If any of this is wanted later, it is in git history.

### 0.4 Unused dependencies and stray files

- `crates/chemspec-app/Cargo.toml`: drop `chems-lang`, `serde`, `serde_json`
  from `[dependencies]` (keep `serde_json` in `[build-dependencies]` — build.rs
  uses it). The app reaches the parser only through `chem_kernel`.
- Remove the empty committed directory `crates/chems-lang/src/bin/`.
- Move `crates/chem-kernel/src/dative_tests.rs` (799 lines, `#[cfg(test)]`
  only) to `crates/chem-kernel/tests/`.

**Exit criteria:** `just ci` green; repo tree shrinks by ~25 MB; no source
references to any removed path.

---

## Phase 1 — One request-matching mechanism

Three generations of "does the catalogue support this request?" coexist. This
is the seam LLM integration will land on, so it must be resolved first.

Current state:

1. **Legacy:** `ReactionRequest::ALL`, a hand-written 36-entry const array in
   `crates/chemspec-app/src/chemistry.rs` (~line 442), with per-family enum
   variants, `legacy_participants()`, `request_for_participants`, and the
   hand-coded `UnsupportedRequest::from_participants` pair table
   (chemistry.rs ~246–313) that duplicates catalogue unsupported-case
   knowledge in Rust.
2. **Registry:** 289 experiences codegen'd by `crates/chemspec-app/build.rs`
   from `catalogue/experience-registry.json`. Draft resolution
   (`requests_for_drafts`, chemistry.rs ~1000–1026) linearly scans both 1 and 2.
3. **Screening:** `reaction_family_candidates` in
   `crates/chem-catalogue/src/reaction_screening.rs` — the trait-based,
   compound-ID-independent API the README advertises as the future. It has
   zero callers and zero tests.

Plan:

### 1.1 Wire screening in front

Make `reaction_family_candidates` the first stage of draft resolution in the
app: drafts → structural-trait screening → family candidates → experience
lookup. Add unit tests for the screening API itself (it currently has none)
and keep the existing 36-case routing fixture in `main.rs` as the regression
harness — it must pass unchanged throughout this phase.

### 1.2 Registry becomes the only experience table

Fold the 36 `ReactionRequest::ALL` entries into `experience-registry.json`
entries (most already exist there; audit for the delta). Delete
`ReactionRequest::ALL`, `legacy_participants()`, `request_for_participants`,
and the per-family enum variants they require.

### 1.3 Unsupported knowledge moves to the catalogue

Replace `UnsupportedRequest::from_participants` (the hand-coded HF /
silver-fluoride / halogen special cases) with catalogue-authored unsupported
cases, which the generalized-rule schema already supports
(`GeneralizedReactionCaseRecord::Unsupported`). The app then renders
unsupported outcomes from catalogue data instead of a Rust table.

### 1.4 Collapse the duplicated types

- Delete the app-side `UnsupportedCase` (chemistry.rs ~240), a field-for-field
  copy of the catalogue record — consume the catalogue type.
- Reconcile the reaction-family enums: app `ReactionFamily`
  (chemistry.rs ~366) vs catalogue `ReactionFamilyCandidate`
  (reaction_screening.rs). One enum, owned by the catalogue.
- `composition_catalogue.rs`: remove the hand-maintained `CompositionId` enum
  + static atom-count table where `TrustedCatalogue` lookups in the same file
  already answer the question; keep only presentation metadata that genuinely
  is not in the catalogue.

**Exit criteria:** one code path from draft to
supported/unsupported/ambiguous; the 36-case routing fixture passes; the
words "legacy" and the README boundary note about request enumeration can be
deleted.

**Fallback:** if 1.2 uncovers experiences the registry cannot express, stop,
extend the registry schema, and do not carry both tables forward.

---

## Phase 2 — Authoring pipeline

The highest-leverage change while the catalogue is hand-curated: cut "one new
reaction family ≈ a day" to "≈ an hour".

### 2.1 Shared authoring library

Create `tools/catalogue_lib.py` (~100–150 lines) extracting what every script
currently reimplements:

- repo-root resolution
- `experience-registry.json` load / filter-by-prefix / append / save
- evidence-packet construction (`schema_version`, `id`, `claims`, `sources`)
- `.chems 1` source templating
- operation-dict helpers (`op(kind, **values)`)
- canonical `json.dumps(..., indent=2) + "\n"` serialization
- JSON-schema validation of output against `schemas/*.schema.json`
  at write time (today authoring errors surface only when Rust consumes the
  files)

Port the 9 `author_*.py` scripts and `generate-covalent-catalogue.py` onto it.
Rewrite `generate-oxygen-catalogue.ps1` (644 lines of PowerShell in a
Rust+Python repo on macOS) as Python on the same library.

### 2.2 Fix or delete the broken last mile

`tools/author_reaction_review.py` reads hardcoded
`reaction-rules-check-expanded-11/` and `reaction-rules-promoted-expanded-11/`
paths that do not exist in the repo, so it silently no-ops. Point it at real,
parameterised paths — or delete it if 2.3's recipes supersede it. Remove the
silent `if not path.exists(): return`; missing input is an error.

### 2.3 justfile recipes

Add recipes so the pipeline is runnable and documented in one place:

```text
just catalogue-generate     # run all author scripts (deterministic output)
just catalogue-check        # chems catalogue check over all candidate packages
just catalogue-promote ATT  # promote with an attestation into trusted/
just catalogue-verify       # regenerate + diff against committed candidates
```

The candidate package list must live in one file (e.g.
`catalogue/candidates/packages.txt` or derived from the directory listing) —
never typed out as 12 paths on a command line again.

### 2.4 CI coverage

Extend `.github/workflows/ci.yml`:

- `just catalogue-verify` — committed candidates are reproducible from the
  scripts (catches machine-local drift, the current failure mode).
- Validate every candidate and trusted JSON against the JSON schemas (today
  only tiny fixtures are schema-validated; the real bundles are only
  type-checked through serde).
- Assert `catalogue.digest`, `promotion.json`, and the pinned Rust digest
  constants agree (see 2.5).
- Lint the Python (`ruff` is sufficient).

### 2.5 Stop hand-editing digest pins

`PINNED_CANONICAL_CATALOGUE_DIGEST` / `PINNED_CANONICAL_REVIEW_DIGEST`
(`crates/chem-catalogue/src/lib.rs` ~41–50) were rewritten in 8 commits over
3 days — every content change edits trust-bearing source. Keep compile-time
pinning (the trust property is good) but generate it: emit the constants from
`catalogue/trusted/core-chemistry/catalogue.digest` via a build script or
`include_str!` + const parsing, so promotion updates one data file and the
Rust source never changes. The CI check in 2.4 makes disagreement impossible
to merge either way.

**Exit criteria:** a new reaction family = one small script importing
`catalogue_lib` + `just catalogue-generate && just catalogue-check` + an
attestation + `just catalogue-promote` + commit. No Rust source edits, no
hand-typed package lists, and CI proves reproducibility.

---

## Phase 3 — Catalogue data format

Cuts the trusted bundle from ~6.9 MB to roughly 2–3 MB and removes the worst
authoring noise. Requires Phase 2 (regeneration must be one command) because
every change here regenerates everything.

### 3.1 Case-level `premise_ids` defaults

55% of all rule bytes (~1.6 MB in core-chemistry) are repeated `premise_ids`
arrays on per-atom correspondence entries — the decane rule alone carries 126
identical 8-element lists. Add an optional case-level (or rule-level)
`default_premise_ids`; per-entry lists become overrides. Requires:

- schema bump consideration — prefer keeping `schema_version: 1` by making
  the field optional-with-default so old data stays valid; only bump if the
  loader must distinguish
- loader change in `chem-catalogue` (resolve defaults at parse time so the
  in-memory model is unchanged and the kernel is untouched)
- generator change in `tools/`
- regenerate candidates + re-promote

### 3.2 Parameterize the alkane family

Combustion is ten per-compound rules (`Rules.ButaneCompleteCombustion` …
`Rules.DecaneCompleteCombustion`); the family machinery being paid for
already supports one rule parameterized over fuel (the generator scripts
even have the parametric form internally). Collapse to one rule with a
`member` parameter over the C1–C10 alkane category, mirroring how
group-1 + water is one rule over five metals. Keep the existing
`generalized-instance` cap (128) — decane's 75 instances stay bounded.

### 3.3 Remove placeholder parameters

Twenty rules carry a 1-value enum parameter (e.g. Decane's
`"outcome": {"enum": ["complete"]}`) invented only because
`validate_generalized_rule_shape` (`generalized.rs` ~190–202) rejects empty
parameter lists. Allow zero-parameter generalized rules in the validator and
drop the placeholders. (Partially mooted for combustion by 3.2; applies to
the others.)

### 3.4 Decide the concrete-rules path

Both shipped bundles have `rules: []`, yet ~1,100 lines of concrete-rule
validation (`lib.rs` ~2835–3576 + helpers) are maintained for fixtures and
tests only. Decide once:

- **Keep** (recommended if generalized elaboration continues to compile into
  concrete `ReactionRuleRecord`s the kernel executes — it does today), and
  document that the JSON-level `rules` array is fixture-only; or
- **Remove** the JSON ingestion of concrete rules and keep only the in-memory
  record the elaborator emits.

Either way, record the decision in `docs/system-architecture.md`.

**Exit criteria:** trusted bundle ≤ ~3 MB; one combustion rule; no
placeholder parameters; digest re-pinned via the Phase 2 mechanism.

---

## Phase 4 — App-boundary and API quality

### 4.1 Preserve error classes across the app boundary

The app collapses every kernel error to `String`
(`Result<TrustedRun, String>`, chemistry.rs ~867; 10×
`map_err(|e| e.to_string())`), discarding the stable error codes
(CHEMS-C001..C023) and `KernelFailureClass` the kernel maintains — so the UI
cannot distinguish the product states the spec defines
(unsupported / no reaction / ambiguous / invalid). Introduce one app-level
error enum that wraps the typed catalogue/kernel errors and carries the code
and failure class through to the UI; render the class, not the string.
Also fix `CatalogueError::is_system_error()` (`lib.rs` ~135) which
unconditionally returns `true`.

### 4.2 Narrow the catalogue's public surface

The crate's API is its wire format: `pub use module::*` glob re-exports
(~120 types) with all-public serde fields, consumed by four crates. Do the
cheap version, not a facade rewrite:

- Replace the glob re-exports with explicit `pub use` lists of what
  downstream crates actually name (grep-driven).
- Demote to `pub(crate)` the pattern/automorphism internals that are public
  only for the crate's own tests (`raw_pattern_matches`,
  `structure_automorphisms`, `pattern_matches_are_automorphism_related`).
- Leave field visibility alone for now — sealing the records is a large
  change with little payoff until there is a second schema version.

### 4.3 Single source for element data

Element symbol/name/atomic-number is maintained three times: app
`elements.rs`, the catalogue JSON element registry, and the Python tools.
Make the catalogue registry canonical; generate the app's presentation table
from it in `build.rs` (which already does codegen), and have `catalogue_lib.py`
read the same JSON.

**Exit criteria:** UI shows distinct product states driven by failure class;
`cargo doc` on chem-catalogue shows an intentional surface; element facts
edited in exactly one file.

---

## Phase 5 — Code structure (opportunistic)

Do these when already working in the files; none are urgent.

### 5.1 Split `chem-catalogue/src/lib.rs` (4,548 lines)

Along its existing seams, matching the pattern already set by
`pattern.rs` / `generalized.rs`:

- `validate_rules` + operation validation (~2835–3576) → `rules.rs`
- template instantiation (~1636–2506) → `templates.rs`
- structure/graph validation (~2506–2835) → `structures.rs`
- canonicalization/normalization (~4057–4400) → `canonical.rs`
- `lib.rs` keeps the trust boundary, error types, and accessors

Pure moves; no signature changes.

### 5.2 Consolidate the operation-template interpreters

`OperationTemplateRecord` (13 variants) is exhaustively matched in ~6 places
across 3 files (`validate_operations`, `instantiate_operation`,
`transform_operation_references`, `set_operation_premises`,
`normalize_operation`, plus model accessors). Full visitor machinery is not
warranted; instead add small shared accessors on the record type
(`premise_ids()`, `referenced_ids()`, `map_references(f)`) so the mechanical
matches collapse and adding a 14th variant touches the enum + the genuinely
semantic matches only.

### 5.3 Split `chemspec-app/src/main.rs` (2,130 lines)

Five screens' view + update logic in one file, while `reactant_composer.rs`
already shows the per-screen module pattern. Extract the remaining screens
the same way.

### 5.4 Test the presentation crate

`chem-presentation` has 2 inline tests for 1,355 lines of timing and
scene-planning logic — the weakest coverage in the workspace. Add
golden-plan tests: for 3–4 validated reactions, snapshot the guided-2D and
macroscopic-3D plans and assert stability. Also add the missing `description`
to its Cargo.toml.

### 5.5 Minor consistency

`structure_premise` (`lib.rs` ~543) linearly scans `document.structures`
despite an ID index existing — use the index. Skip the allocation-churn items
from the review (`canonical_document` clones, per-call `BTreeSet` rebuilds,
`format!` match keys): all measured fast at current scale; revisit only if
the catalogue grows an order of magnitude.

---

## Phase 6 — Docs honesty and the LLM seam

### 6.1 Make the architecture doc match the code

`docs/system-architecture.md` describes an `agent` crate (provider preflight,
observation research, `.chems` authoring, post-playback overview) that does
not exist; there are zero network calls in the workspace, and the
provider-setup screen collects an API key that is never used. Until the agent
is built:

- Mark the `agent` box and runtime-flow steps as **planned**, not present.
- Fix the boundaries diagram: remove the unused app→chems-lang edge, add the
  real app→catalogue/kernel edges.
- Either remove the provider-setup gate from the app flow or label it
  explicitly as staging for the future agent — an app must not gate on a
  credential that does nothing.

### 6.2 Define the LLM integration contract now

The review's central conclusion: the trust architecture is already correct
for LLM integration — the agent proposes source and observations; only the
kernel mints trusted chemistry. Write the contract down before building:

1. Request arrives → Phase 1's screening API answers
   catalogued / unsupported / unknown.
2. Only **unknown** goes to the LLM. The LLM returns a `.chems 1` source
   proposal + evidence packet (`schemas/chem-evidence-packet-1.schema.json`
   already exists for this).
3. The proposal runs the identical parse → elaborate → validate path as
   trusted sources; failures surface as honest Unsupported with the kernel's
   failure class (Phase 4.1), never a guessed outcome.
4. LLM-derived results render visibly distinct from reviewed-catalogue
   results, and are never persisted into `trusted/` without the existing
   check → attestation → promote flow.

This is a doc deliverable (extend `docs/system-architecture.md` or a new
`docs/llm-integration-contract.md`), not code — but Phases 1, 2, and 4.1 are
its prerequisites, which is why they precede it.

---

## Sequencing summary

| Phase | Effort | Risk | Unblocks |
|-------|--------|------|----------|
| 0 Delete dead weight | hours | none | everything (smaller tree) |
| 1 One matching mechanism | days | medium — guarded by routing fixture | LLM seam, Phase 6 |
| 2 Authoring pipeline | 1–2 days | low | Phase 3, catalogue growth |
| 3 Data format | 1–2 days | medium — full regenerate + re-promote | catalogue scale |
| 4 Boundary/API quality | ~1 day | low | product states, Phase 6 |
| 5 Code structure | opportunistic | low | maintainability |
| 6 Docs + LLM contract | ~½ day writing | none | LLM integration |

Rules that hold across all phases:

- The kernel remains the only constructor of trusted chemistry; no phase
  weakens that.
- The 36-case routing fixture and the conformance suite pass at every step.
- Every regeneration of catalogue data goes through the Phase 2 recipes, and
  CI proves committed data is reproducible before anything else lands on it.
