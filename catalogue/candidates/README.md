# Catalogue candidate queue

Each child directory is an untrusted catalogue shard containing exactly:

- `candidate.json` — typed catalogue records only;
- `example.chems` — one ordinary authored invocation; and
- `evidence.json` — an explicitly untrusted observation packet for that
  invocation.

The initial chemist-selected queue contains one package:

| Package | Content | Review state |
|---|---|---|
| `periodic-table-and-alkali-water` | 118 element identity records and `Rules.AlkaliMetalWithWater` for Li, Na, and K | `pending-host-review` |

No queue entry is production chemistry. The identity records cite the IUPAC
periodic table and deliberately remain provisional. Conventional block choices
and optional group placement, particularly around the f block, are included in
the exact digest presented to the chemist rather than silently treated as
authority.

Generate a review bundle from the repository root:

```sh
cargo run -p chems-cli -- catalogue check --out /tmp/chems-review \
  catalogue/candidates/periodic-table-and-alkali-water
```

The compiler rejects extra package files and unknown candidate fields. It does
not read generated artifacts back as input. Candidate premises must be
`provisional` with no reviewers; only a separate host-selected AI review may supply review
metadata through the separate attestation boundary.
