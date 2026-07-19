# Rust workspace

The language workspace implements the definitive structural `.chems 1`
contract through the fixed Slices 0–6.

| Crate | Definitive responsibility |
| --- | --- |
| [`chem-domain`](chem-domain/) | Pure structural identities, atom/electron state, shared and dative graphs, mappings, and operations |
| [`chems-lang`](chems-lang/) | Sole authored-source lexer, lossless CST, AST, formatter, spans, and syntax diagnostics |
| [`chem-catalogue`](chem-catalogue/) | Immutable reviewed structures, reaction rules, templates, evidence premises, provenance, and digests |
| [`chem-kernel`](chem-kernel/) | Resolution, deterministic expansion, graph transitions, conservation, derivations, and private validation |
| `chems-cli` | Parsing, formatting, authored-source inspection, and expanded-certificate inspection |

Dependencies point inward:

```text
chems-lang       -> no chemistry authority
chem-domain      -> no parsing or I/O
chem-catalogue   -> chem-domain
chem-kernel      -> chems-lang + chem-catalogue + chem-domain
chems-cli         -> chem-kernel + chems-lang + chem-catalogue + chem-domain
```

Shared fixtures under [`../conformance`](../conformance/) are consumed directly
by their owning crate tests. `cargo test --workspace` is the conformance gate.

Older quantitative domain modules remain only as unrelated internal
application/archaeology code. They are not consumed by the structural pipeline
and are not a supported language or compatibility surface.
