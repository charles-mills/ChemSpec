# Catalogue breadth review handoff

Status: corrected candidate queue is complete, reproducible, separately
attested, promoted, and compiled into the host trust root.

## Review corrections incorporated

The independent review rejected the first overnight draft. This revision
incorporates every finding:

1. Silver-halide precipitation now has separate Cl, Br, and I cases with
   exact white, cream, and yellow observations respectively.
2. The chemically misleading alkali-metal/aqueous-alkali-salt displacement
   package was removed. It is replaced by conventional aqueous halogen
   displacement over the bounded order `Cl > Br > I`.
3. Acid-carbonate gas evolution now includes both carbonate and bicarbonate
   salts as first-class executable rules.
4. Tests execute every finite binding, rather than inspecting only declared
   JSON sets. This includes every supported and explicit unsupported case.
5. Evidence records and observation packets cite direct OpenStax and Royal
   Society of Chemistry pages rather than generic book landing pages.
6. Commands and documentation state that the compiler output path must not
   already exist.
7. External chemistry evidence is separated from ChemSpec's internal
   explanatory structural model. Exact graph representations and transient
   execution states no longer claim source support as physical mechanisms.

No candidate self-asserts review or promotion. No generated review artifact is
committed as source.

## Candidate packages

```text
catalogue/candidates/periodic-table-and-alkali-water/
catalogue/candidates/precipitation-silver-halide/
catalogue/candidates/acid-base-neutralization/
catalogue/candidates/acid-carbonate-gas-evolution/
catalogue/candidates/single-displacement-halogen/
```

The superseded `acid-bicarbonate-gas-evolution` and
`single-displacement-alkali-metal` packages have been removed.

## Finite executable coverage

| Family | Supported | Explicit unsupported |
|---|---:|---:|
| Silver-halide precipitation | 3 | 1 |
| Acid-base neutralization | 9 | 3 |
| Acid-carbonate and bicarbonate gas evolution | 18 | 6 |
| Halogen displacement | 3 | 13 |

All 56 bindings represented by this table are passed through ordinary
`.chems` parsing, generalized elaboration, structural validation, and frame
generation tests. An unsupported binding must produce `UnsupportedChemistry`
and must not create an output directory.

## Exact semantic digest

```text
b309c458e74b84b338afa1d172da7c48b8ef1f08bf0a82fd30f3aedbe7d11440
```

Forward and fully reversed package orders produce this same digest and byte-
identical `catalogue.digest` files.

## Reproduction

Choose an output path that does not already exist:

```sh
cargo run -p chems-cli -- catalogue check --out /tmp/chems-review-<unique> \
  catalogue/candidates/periodic-table-and-alkali-water \
  catalogue/candidates/precipitation-silver-halide \
  catalogue/candidates/acid-base-neutralization \
  catalogue/candidates/acid-carbonate-gas-evolution \
  catalogue/candidates/single-displacement-halogen

cargo test -p chems-cli --test authoring
```

Expected compiler status:

```text
status: candidate-inspection-only
promotable: false
review-request status: pending-ai-review
```

Repository verification completed successfully:

```sh
cargo fmt --all --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
cargo test --workspace --doc
cargo run -p chems-conformance -- validate
git diff --check
```

The conformance command exits successfully while continuing to report the
repository's four pre-existing incomplete conformance cases. Cargo also emits
the existing future-incompatibility advisory for `block v0.1.6`.

## Completed trust action

The host-selected AI attestation at
`catalogue/reviews/core-chemistry.review.json` binds exactly the digest above.
The promoted derivative is stored under `catalogue/trusted/core-chemistry/`,
and `chem-catalogue` pins both its catalogue and review semantic digests.
Promotion and runtime loading fail if either exact semantic digest changes.
