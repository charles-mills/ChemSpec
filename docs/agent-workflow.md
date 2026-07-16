# Agent workflow and providers

## Role of the agent

The agent is a reaction researcher, working-catalogue author, concise `.chems
1` author, repair assistant, and explainer—not a chemistry authority.

The application first checks the host-pinned catalogue. On a miss, the agent:

1. researches the representative reaction and typed qualitative observations
   with claim-level evidence;
2. authors a self-contained `working` catalogue document containing the exact
   premises, structures, applicability, mapping, and operation template;
3. authors concise source bound to that working catalogue and evidence packet;
4. receives catalogue, parsing, binding, expansion, and structural diagnostics;
5. proposes a bounded visible patch when repair is possible; and
6. supplies a concise evidence-linked overview after playback.

The agent may author atom-map and operation templates only inside an untrusted
working catalogue. Those records have no effect unless the deterministic
catalogue and chemistry validators accept the complete artifact. The agent
never supplies a real-world laboratory method.

## Visible workflow

```text
✓ No catalogue entry; Codex is building this reaction
✓ Researched reaction and qualitative observations
✓ Generated working catalogue and LithiumAndWater.chems
✓ Expanded 4 reactant and 3 product instances
✗ Validation: observation claim R2 has the wrong subject
↻ Correcting the .chems source
✓ Structural validation passed
```

These are checkable product events, not hidden chain-of-thought.

## Provider selection

Startup exposes two provider cards.

### Codex subscription

Preflight locates `codex`/`codex.exe`, checks `codex --version`, reads
`codex login status`, and capability-probes `codex exec --help`. ChemSpec never
reads credential files directly.

Required capabilities are non-interactive execution, JSONL events, output
schema, ephemeral runs, ignored user configuration/rules, read-only sandbox,
live search, working-directory selection, and a last-message artifact.
Capability detection is authoritative; no arbitrary minimum version is assumed.

Conceptual invocation:

```text
codex --search exec
  --json
  --output-schema <result-schema.json>
  --output-last-message <result.json>
  --sandbox read-only
  --ephemeral
  --ignore-user-config
  --ignore-rules
  --skip-git-repo-check
  -C <isolated-temporary-directory>
  -
```

The provider chooses a currently available Codex model independently of the
direct API provider. It must not assume the same model identifier is valid on
both surfaces. The prompt is supplied through stdin and is self-contained: its
editable Markdown template, normative `.chems` specification and grammar,
catalogue/evidence schemas, and complete reference artifact are compiled into
the binary. An installed app never assumes a repository checkout or asks Codex
to read project files. Codex runs in an isolated temporary directory. It emits
JSONL, but the first vertical discards those events, captures bounded stderr on
failure, and reads the last-message artifact only after the child succeeds.
Iced generation IDs reject stale completions. Parsing events into visible
workflow entries and direct child termination remain follow-up work.

Never use sandbox-bypass flags.

### OpenAI API key

API-key mode is the planned second provider and has no Codex binary dependency.
The startup selector and in-memory field exist, but dynamic Responses API
construction is not connected yet. It will not mutate shared Codex
authentication.

The key stays in memory by default and may be persisted only through the
operating-system credential manager after explicit choice. It appears only in
the authorization header and never in prompts, logs, provenance, `.chems`, or
child environments.

The provider uses a separately configured supported Responses API model, hosted
web search, moderation, and strict structured output. Model identifiers remain
provider-specific and capability/configuration driven.

## Provider-neutral interface

```text
AgentProvider
  preflight()
  observations(resolved_rule, event_sink)
  author_source(resolved_rule, observations, event_sink)
  repair(previous_result, diagnostics, event_sink)
  overview(validated_reaction, event_sink)
  cancel(run_id)
```

Both providers use the same normalized result envelope. Catalogue validation,
parsing, expansion, kernel validation, and simulation are provider independent.
The initial implemented live path is Codex subscription.

## Evidence packet

Research is claim-oriented:

```text
Evidence.LithiumAndWater@1
  R1
    subject: hydrogen product
    predicate: gas evolves
    sources: [S1, S2]
  R2
    subject: lithium reactant
    predicate: disappears
    sources: [S1]
```

Sources preserve title, URL, publisher, retrieval time, and supported claim
IDs. Search snippets are not evidence. Conflicts remain visible rather than
being silently averaged.

The evidence packet is immutable and digest-bearing. Source references claim
IDs; editing evidence invalidates downstream validation through its digest.

## Structured output

Provider output contains source text, the evidence packet, a working catalogue
document, and provenance. The Codex outer schema carries the three nested
artifacts as strings so each repository-owned strict decoder remains
authoritative. Strict parsing rejects unknown fields, missing claims, invalid
IDs, source/evidence/catalogue disagreement, or provider prose outside the
declared schema.

The editable runtime template lives at
`crates/agent/prompts/dynamic-reaction.md`. Compile-time inclusions deliberately
reuse the normative source files instead of copying their contents into Rust or
maintaining a second grammar/schema.

## Repair loop

A repair request contains the original request, current working catalogue,
evidence packet, source, and stable diagnostics. It instructs the agent to
patch only the rejected candidate artifact; validator configuration and the
host-pinned catalogue are unavailable for modification.

Allow at most three repair attempts. Every patch is shown as a diff and remains
undoable. After the limit, stop with a transparent invalid or unsupported
result.

## Failure behavior

Provider unavailable, authentication required, cancellation, timeout, refusal,
malformed structured result, incomplete evidence, and repair-limit exhaustion
are explicit workflow states. None is reinterpreted as a chemistry result.
