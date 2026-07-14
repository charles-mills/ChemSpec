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
- Expected chemistry is independently authored and must receive an explicit,
  digest-pinned host trust decision before production promotion.

`catalogue/alkali-metal-water-001.catalogue.json` is the canonical generalized
family catalogue. `catalogue/lithium-rule-001.catalogue.json` is retained only
as the non-generalized concrete compatibility fixture.

Quantitative, vessel, material, and physical-procedure fixtures remaining in
the repository are unreferenced implementation archaeology until their code is
replaced. They do not define compatibility or conformance.

## Canonical structural evidence

Slice 0 establishes:

- `parsing/canonical-source-001.chems` — definitive authored source;
- `structural-domain/electron-model-001.*` — formal-charge and electron ledger;
- `catalogue/lithium-rule-001.*` — digest-bound structural catalogue, closed
  reaction rule, and exact AI review attestation;
- `expansion/canonical-expansion-001.*` — executable review-candidate source,
  complete independent HIR oracle, and exact unexecuted text certificate;
- `observations/lithium-observations-001.input.json` — typed evidence packet;
  and
- `end-to-end/lithium-outcome-001.*` — trusted vertical artifact with explicit
  model assumptions;
- `frames/canonical-frames-001.expected.json` — independently authored frame,
  change, and observation-synchronization review candidate.

The external evidence packet remains labelled untrusted runtime research and
cannot claim host review. Candidate expansion remains visibly untrusted. The
trusted runtime catalogue requires both the host-pinned catalogue digest and
the exact host-selected AI review-attestation digest; runtime agents and
candidate packages cannot manufacture or update either trust root.

## Slice progression

- Slice 1 turns structural-domain oracles into executable domain tests and
  promotes them only after those tests pass.
- Slice 2 replaces parser/formatter cases and their independent CST/AST oracles.
- Slice 3 validates reviewed catalogue and rule fixtures.
- Slice 4 validates expansion and certificate oracles.
- Slice 5 executes the complete independent state-chain oracle and adds exact
  operation-precondition, conservation, mapping, product, and staleness cases.
- Slice 6 implements frames, staleness, artifact, CLI inspection, and complete
  requirement coverage. The canonical trusted path is promoted through the
  pinned AI attestation; unrelated archaeology cases remain honestly incomplete.

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
