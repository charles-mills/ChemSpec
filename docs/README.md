# ChemSpec documentation

The root of `docs/` contains durable product, chemistry, architecture, safety,
presentation, and verification contracts. Documents that describe temporary
work are kept one level deeper so completed plans and review records are not
mistaken for current authority.

## Durable sources

| Area | Source of truth |
| --- | --- |
| Product | [`product-spec.md`](product-spec.md) |
| System boundaries | [`system-architecture.md`](system-architecture.md) |
| Chemistry engine and trust | [`chemistry-engine.md`](chemistry-engine.md) |
| `.chems` overview | [`chems-language.md`](chems-language.md) |
| Normative `.chems` semantics | [`chems-specification.md`](chems-specification.md) and [`../grammar/chems.ebnf`](../grammar/chems.ebnf) |
| Generalized chemistry | [`generalized-chemistry-rules.md`](generalized-chemistry-rules.md) |
| Catalogue coverage | [`catalogue-coverage.md`](catalogue-coverage.md) |
| Agent and provider boundary | [`agent-workflow.md`](agent-workflow.md) |
| Safety | [`safety.md`](safety.md) |
| Structural presentation | [`automatic-animation-system.md`](automatic-animation-system.md) |
| Macroscopic presentation | [`macroscopic-visual-system.md`](macroscopic-visual-system.md) |
| Interface design | [`ui-design-system.md`](ui-design-system.md) |
| Verification and release gates | [`verification.md`](verification.md) |

`structural-chems-architecture.md` remains a focused explanation of the
structural language model. Use `system-architecture.md` for workspace ownership
and dependency direction.

## Current work

- [`plans/`](plans/) contains active Build Week delivery and dynamic-reaction
  execution documents.
- [`backlog/`](backlog/) contains explicitly deferred, unapproved proposals.

## Historical records

[`archive/`](archive/) contains completed implementation plans, handoffs,
reviews, change notes, superseded contracts, and visual-QA evidence. Archived
documents are retained for provenance and should not be used as current
implementation authority.
