# Agent workflow and providers

## Role of the agent

The agent is an identity assistant, observation researcher, concise `.chems 1`
author, repair assistant, and explainer—not a chemistry authority.

The deterministic engine resolves catalogue identities and selects a unique
reviewed reaction rule before observation research. The agent then:

1. researches typed qualitative observations with claim-level evidence;
2. authors concise source using resolved identities and the selected rule;
3. receives parsing, binding, expansion, and structural diagnostics;
4. proposes a bounded visible patch when repair is possible; and
5. supplies a concise evidence-linked overview after playback.

The agent never authors the atom-map or operation templates already owned by
the rule and never supplies a real-world laboratory method.

## Visible workflow

```text
✓ Identified lithium and water
✓ Selected reviewed AlkaliMetalWithWater rule
✓ Researched qualitative observations
✓ Generated LithiumAndWater.chems
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
schema, ephemeral runs, ignored user configuration, read-only sandbox, live
search, working-directory selection, and cancellation. Capability detection is
authoritative; no arbitrary minimum version is assumed.

Conceptual invocation:

```text
codex exec
  --json
  --output-schema <result-schema.json>
  --sandbox read-only
  --search
  --ephemeral
  --ignore-user-config
  --ignore-rules
  --skip-git-repo-check
  -C <empty-run-directory>
  -
```

The provider chooses a currently available Codex model independently of the
direct API provider. It must not assume the same model identifier is valid on
both surfaces. The prompt is supplied through stdin. ChemSpec parses stdout
JSONL, captures stderr separately, terminates the child on cancellation, and
writes returned source only after validating the structured envelope.

Never use sandbox-bypass flags.

### OpenAI API key

API-key mode calls the Responses API directly and has no Codex binary
dependency. It does not mutate shared Codex authentication.

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

Both providers emit the same normalized events and result envelopes. Parsing,
catalogue resolution, expansion, validation, and simulation are provider
independent.

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

Provider output contains resolved source text plus the evidence packet and
provenance envelope. Strict parsing rejects unknown fields, missing claims,
invalid IDs, source/evidence disagreement, or provider prose outside the
declared schema.

## Repair loop

A repair request contains the original request, selected reviewed rule, current
evidence packet, current source, and stable diagnostics. It instructs the agent
to patch only invalid authored fields; catalogue facts, rule templates, and
validator configuration are unavailable for modification.

Allow at most three repair attempts. Every patch is shown as a diff and remains
undoable. After the limit, stop with a transparent invalid or unsupported
result.

## Failure behavior

Provider unavailable, authentication required, cancellation, timeout, refusal,
malformed structured result, incomplete evidence, and repair-limit exhaustion
are explicit workflow states. None is reinterpreted as a chemistry result.
