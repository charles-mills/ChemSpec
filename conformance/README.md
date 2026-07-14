# `.chems` conformance contract

This directory is the executable evidence registry for the definitive
structural `.chems 1` language.

```text
conformance/
  requirements.json
  requirements.schema.json
  manifest.json
  manifest.schema.json
  reserved-words.txt
  specification/
  encoding-layout/
  parsing/
  formatting/
  structural-domain/
  catalogue/
  expansion/
  validation-kernel/
  observations/
  diagnostics-tooling/
  artifacts/
  frames/
  end-to-end/
```

The registry deliberately reports incomplete coverage while implementation
slices remain outstanding. A listed case may have expected state `incomplete`
to record an independently authored future oracle without claiming that the
current implementation supports it.

For component-level cases, expected state `validated` means that component's
fixture constructed and passed its owned invariants. It is not the public
reaction result `Validated`, which remains unreachable for initial-language
reactions because their required model assumptions produce
`ValidatedWithAssumptions`.

## Authority

- `requirements.json` maps every normative specification section to exactly one
  component.
- `manifest.json` maps cases to requirements and owned fixture paths.
- `grammar/chems.ebnf` is the only normative grammar.
- `reserved-words.txt` must exactly equal the specification's reserved-word
  block and contain every grammar keyword.
- Expected chemistry is independently authored and must be reviewed before
  promotion.

Quantitative, vessel, material, and physical-procedure fixtures remaining in
the repository are unreferenced implementation archaeology until their code is
replaced. They do not define compatibility or conformance.

## Canonical structural evidence

Slice 0 establishes:

- `parsing/canonical-source-001.chems` — definitive authored source;
- `structural-domain/electron-model-001.*` — formal-charge and electron ledger;
- `catalogue/lithium-rule-001.*` — provisional structure/rule shape prepared
  for review;
- `expansion/canonical-expansion-001.*` — complete atom map and operation
  certificate prepared for chemistry review;
- `observations/lithium-observations-001.input.json` — typed evidence packet;
  and
- `end-to-end/lithium-outcome-001.*` — honest incomplete vertical artifact.

The catalogue, evidence, and expanded certificate explicitly remain
`pending-chemist-review` until the resident chemist accepts their exact states.
No implementation may convert that status into production trust.

## Slice progression

- Slice 1 turns structural-domain oracles into executable domain tests and
  promotes them only after those tests pass.
- Slice 2 replaces parser/formatter cases and their independent CST/AST oracles.
- Slice 3 validates reviewed catalogue and rule fixtures.
- Slice 4 validates expansion and certificate oracles.
- Slice 5 adds operation, conservation, derivation, and negative kernel cases.
- Slice 6 adds frames, staleness, artifact, and complete end-to-end evidence.

Each case is promoted from `incomplete` only when its producing implementation
and independent review are complete.

## Commands

```sh
cargo run -p chems-conformance -- validate
cargo run -p chems-conformance -- report
cargo test -p chems-conformance
```

`validate` checks internal consistency and succeeds for an honestly partial
registry. `report` exits non-zero until no case remains `incomplete` and every
component has promoted cases covering all owned requirements.
