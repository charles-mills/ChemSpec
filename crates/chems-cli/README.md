# `chems-cli`

The outer `chems` command composes the source frontend with catalogue-backed
provisional expansion while keeping chemistry authority in `chem-kernel`.

```sh
cargo run -p chems-cli -- parse reaction.chems
cargo run -p chems-cli -- format --check reaction.chems
cargo run -p chems-cli -- inspect source reaction.chems
cargo run -p chems-cli -- inspect expanded reaction.chems \
  --catalogue catalogue.json --evidence evidence.json
cargo run -p chems-cli -- catalogue check --out review-output \
  catalogue/candidates/periodic-table-and-alkali-water
cargo run -p chems-cli -- catalogue promote --out trusted-output \
  --attestation catalogue/reviews/core-chemistry.review.json \
  catalogue/candidates/periodic-table-and-alkali-water \
  catalogue/candidates/precipitation-silver-halide \
  catalogue/candidates/acid-base-neutralization \
  catalogue/candidates/acid-carbonate-gas-evolution \
  catalogue/candidates/single-displacement-halogen
```

Expanded inspection defaults to the human-readable unexecuted certificate.
`--json` prints canonical semantic HIR and `--provenance` prints exact source
origins. For a generalized rule these views include inferred parameters, the
selected case, equivalent-match count, instantiated concrete applications,
matched sites, and parameter/role premise provenance. Inspection never
promotes a provisional catalogue or constructs validated chemistry.

`catalogue check` is the catalogue-authoring compiler. Every input directory
must contain exactly `candidate.json`, `example.chems`, and `evidence.json`.
It merges shards in semantic order, rejects duplicate identities before merge,
validates the resulting catalogue and every example through expansion, kernel
execution, and frame projection, then writes:

- `catalogue.json` and `catalogue.digest`;
- `review-request.json` with status `pending-ai-review`; and
- one candidate-only expanded certificate, derivation, and frame sequence per
  shard under `inspections/`.

The output directory must not already exist or be inside a candidate package.
Candidate JSON has no fields for publication metadata, trust roots, validation
options, output paths, review attestations, or generated artifacts. Generated
inspection artifacts are labelled `candidate-inspection-only` and
`promotable: false`; they cannot satisfy the host-selected AI-review boundary.
Premises in candidate shards must be provisional and carry no reviewers.

`catalogue promote` rebuilds the same provisional digest, validates a separate
review artifact against every exact premise and evidence source, and writes the
catalogue, review, their semantic digests, and a promotion manifest. Compiling
both digests into `ReferenceCatalogue` establishes reproducible reviewed
provenance only. It does not authorize chemistry or grant a renderer
capability; the kernel validation path does that for both provisional and
reviewed-reference inputs.
