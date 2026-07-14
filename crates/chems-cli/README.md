# `chems-cli`

The outer `chems` command composes the source frontend with catalogue-backed
review-candidate expansion while keeping chemistry authority in `chem-kernel`.

```sh
cargo run -p chems-cli -- parse reaction.chems
cargo run -p chems-cli -- format --check reaction.chems
cargo run -p chems-cli -- inspect source reaction.chems
cargo run -p chems-cli -- inspect expanded reaction.chems \
  --catalogue catalogue.json --evidence evidence.json
```

Expanded inspection defaults to the human-readable unexecuted certificate.
`--json` prints canonical semantic HIR and `--provenance` prints exact source
origins. Inspection never promotes a review-candidate catalogue or constructs
trusted chemistry.
