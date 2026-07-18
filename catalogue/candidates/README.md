# Catalogue candidate queue

Each child directory is an untrusted catalogue shard containing exactly:

- `candidate.json` — typed catalogue records only;
- `example.chems` — one ordinary authored invocation; and
- `evidence.json` — an explicitly untrusted observation packet for that
  invocation.

The promoted review currently covers these source surfaces:

| Package | Content | Review state |
|---|---|---|
| `periodic-table-and-alkali-water` | 118 element identity records and `Rules.AlkaliMetalWithWater` for Li, Na, and K | `host-selected-ai-reviewed` and pinned in aggregate |
| `precipitation-silver-halide` | `Categories.Halide` and `Rules.SilverHalidePrecipitation` with member-specific AgCl/AgBr/AgI colours | `host-selected-ai-reviewed` and pinned in aggregate |
| `acid-base-neutralization` | `Rules.MonoproticAcidHydroxideNeutralization` (HX + MOH -> MX + H2O) | `host-selected-ai-reviewed` and pinned in aggregate |
| `acid-carbonate-gas-evolution` | Carbonate and bicarbonate rules: 2 HX + M2CO3 or HX + MHCO3 -> MX + H2O + CO2 | `host-selected-ai-reviewed` and pinned in aggregate |
| `single-displacement-halogen` | `Rules.HalogenDisplacement` for the bounded aqueous order Cl > Br > I | `host-selected-ai-reviewed` and pinned in aggregate |
| `oxygen-reactions` | 68 representative elemental-oxygen outcomes and 81 fixed-charge main-group ion-pair experiences | `host-selected-ai-reviewed` and pinned in aggregate |
| `covalent-combinations` | 20 explicit hydrogen-compound and interhalogen outcomes | `host-selected-ai-reviewed` and pinned in aggregate |

See the current [`trusted catalogue coverage`](../../docs/catalogue-coverage.md)
for the finite-domain summary and machine-readable authorities. Completed
execution and review records are retained under
[`docs/archive/catalogue/`](../../docs/archive/catalogue/).

The reviewed surfaces are promoted together as the exact generated aggregate under
`catalogue/trusted/core-chemistry/`, with a separate AI attestation and both
semantic digests pinned in `chem-catalogue`. The 118 element records provide
identity metadata only; runnable reaction coverage is explicitly limited to
the reviewed families and members named by the current attestation.

Generate a review bundle covering every package from the repository root:

```sh
cargo run -p chems-cli -- catalogue check --out /tmp/chems-review-$(date +%s) \
  catalogue/candidates/periodic-table-and-alkali-water \
  catalogue/candidates/precipitation-silver-halide \
  catalogue/candidates/acid-base-neutralization \
  catalogue/candidates/acid-carbonate-gas-evolution \
  catalogue/candidates/single-displacement-halogen
```

The command above reproduces the original generalized-family review bundle;
the larger oxygen, ion-pair, and covalent surfaces have repository-owned
generators documented in the coverage summary. The compiler rejects extra
package files and unknown candidate fields. It does
not read generated artifacts back as input. Candidate premises must be
`provisional` with no reviewers; only a separate host-selected AI review may supply review
metadata through the separate attestation boundary.
