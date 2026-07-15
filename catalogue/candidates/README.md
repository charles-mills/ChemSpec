# Catalogue candidate queue

Each child directory is an untrusted catalogue shard containing exactly:

- `candidate.json` — typed catalogue records only;
- `example.chems` — one ordinary authored invocation; and
- `evidence.json` — an explicitly untrusted observation packet for that
  invocation.

The chemistry queue contains these source packages:

| Package | Content | Review state |
|---|---|---|
| `periodic-table-and-alkali-water` | 118 element identity records and `Rules.AlkaliMetalWithWater` for Li, Na, and K | `host-selected-ai-reviewed` and pinned |
| `precipitation-silver-halide` | `Categories.Halide` and `Rules.SilverHalidePrecipitation` (AgNO3 + NaX -> AgX + NaNO3, X in {Cl, Br, I}) | `pending-ai-review` |
| `acid-base-neutralization` | `Rules.MonoproticAcidHydroxideNeutralization` (HX + MOH -> MX + H2O) | `pending-ai-review` |
| `acid-bicarbonate-gas-evolution` | `Rules.MonoproticAcidBicarbonateGasEvolution` (HX + MHCO3 -> MX + H2O + CO2) | `pending-ai-review` |
| `single-displacement-alkali-metal` | `Rules.AlkaliMetalActivitySeriesDisplacement` (K/Na displacing a less reactive alkali metal from its halide salt) | `pending-ai-review` |

See [`docs/catalogue-breadth-execution-plan.md`](../../docs/catalogue-breadth-execution-plan.md)
for the exact finite domain, structures, and evidence backing each of the
four newer packages, and
[`docs/catalogue-breadth-review-handoff.md`](../../docs/catalogue-breadth-review-handoff.md)
for their review status.

Only `periodic-table-and-alkali-water` remains promoted. Its exact generated
derivative is stored under `catalogue/trusted/` with a separate AI
attestation and both semantic digests pinned in `chem-catalogue`. The 118
records provide element identity metadata only; runnable reaction coverage
across all packages is explicitly limited to the reviewed families and
elements named above.

Generate a review bundle covering every package from the repository root:

```sh
cargo run -p chems-cli -- catalogue check --out /tmp/chems-review \
  catalogue/candidates/periodic-table-and-alkali-water \
  catalogue/candidates/precipitation-silver-halide \
  catalogue/candidates/acid-base-neutralization \
  catalogue/candidates/acid-bicarbonate-gas-evolution \
  catalogue/candidates/single-displacement-alkali-metal
```

The compiler rejects extra package files and unknown candidate fields. It does
not read generated artifacts back as input. Candidate premises must be
`provisional` with no reviewers; only a separate host-selected AI review may supply review
metadata through the separate attestation boundary.
