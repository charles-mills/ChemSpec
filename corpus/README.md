# Dynamic reaction corpus

`dynamic-reactions-v1.json` is the versioned offline breadth contract for
`DYN-112`. It contains 266 unique request texts across every required chemistry
category. Each case names its expected product state explicitly; adversarial
cases do not inherit success merely from a related balanced scenario.

The manifest loader rejects duplicate requests, missing categories, incomplete
oracles, fewer than 250 cases, and live-smoke selections that do not span every
category. Corpus metrics keep identity, balance, evidence coverage, mapping,
presentation, trust tier, factual state, failure class, and latency separate.

## Offline report

Produce a report from a JSON array of `CorpusObservation` records with exactly
one record per case and per provider/model candidate:

```sh
cargo run -p agent --bin dynamic-corpus-report -- \
  corpus/dynamic-reactions-v1.json observations.json
```

The report records the corpus, provider, model, and provider version and emits
nearest-rank p50/p95 latency budgets for local hits, Fast cold static outcomes,
Researcher/evidence outcomes, and escalated mechanisms. It refuses to select a
release-default model while any corpus oracle lacks independent review.

`oracle_reviewed_by` is deliberately `null` in v1. A qualified reviewer must
review the identities, outcomes, balance vectors, evidence expectations, and
presentation expectations and record their identity before the results can be
used as a factual release-accuracy claim.

## Explicit live smoke

Normal tests are deterministic and offline. The ignored live smoke invokes the
signed-in Codex CLI in Researcher mode for 25 selected cases, performs bounded
HTTPS retrieval for every returned source, and prints provider/model/version
records:

```sh
cargo test -p agent representative_live_codex_and_evidence_smoke -- \
  --ignored --nocapture --test-threads=1
```

This command consumes subscription capacity and network access. It is a
capability smoke, not chemistry review and not a substitute for the full
dimension-separated observation report.
