# Rust workspace

The language workspace is being rebaselined onto the definitive structural
`.chems 1` contract through the fixed Slices 0–6.

| Crate | Definitive responsibility |
| --- | --- |
| [`chem-domain`](chem-domain/) | Pure structural identities, atom/electron state, graphs, mappings, operations, artifacts, and frames |
| [`chems-lang`](chems-lang/) | Sole authored-source lexer, lossless CST, AST, formatter, spans, and syntax diagnostics |
| [`chem-catalogue`](chem-catalogue/) | Immutable reviewed structures, reaction rules, templates, evidence premises, provenance, and digests |
| [`chem-kernel`](chem-kernel/) | Resolution, deterministic expansion, graph transitions, conservation, derivations, and private validation |
| [`chems-conformance`](chems-conformance/) | Specification, grammar, reserved-word, fixture, schema, and coverage validation |

Dependencies point inward:

```text
chems-lang       -> no chemistry authority
chem-domain      -> no parsing or I/O
chem-catalogue   -> chem-domain
chem-kernel      -> chems-lang + chem-catalogue + chem-domain
chems-conformance -> repository contracts and canonical serialization helpers
```

Code encoding the discarded quantitative source remains temporary internal
implementation until its producing replacement slice lands. It is not a
supported compatibility surface.
