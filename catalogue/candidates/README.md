# Catalogue candidate queue

Each child directory is an untrusted catalogue shard containing exactly:

- `candidate.json` — typed catalogue records only;
- `example.chems` — one ordinary authored invocation; and
- `evidence.json` — an explicitly untrusted observation packet for that
  invocation.

The initial chemistry queue contains one source package:

| Package | Content | Review state |
|---|---|---|
| `periodic-table-and-alkali-water` | 118 element identity records and `Rules.AlkaliMetalWithWater` for Li, Na, and K | `host-selected-ai-reviewed` and pinned |

The package remains untrusted authoring input. Its exact generated derivative
is stored under `catalogue/trusted/` with a separate AI attestation and both
semantic digests pinned in `chem-catalogue`. The 118 records provide element
identity metadata only; runnable reaction coverage is explicitly limited to
Li, Na, and K in the one reviewed family.

Generate a review bundle from the repository root:

```sh
cargo run -p chems-cli -- catalogue check --out /tmp/chems-review \
  catalogue/candidates/periodic-table-and-alkali-water
```

The compiler rejects extra package files and unknown candidate fields. It does
not read generated artifacts back as input. Candidate premises must be
`provisional` with no reviewers; only a separate host-selected AI review may supply review
metadata through the separate attestation boundary.
