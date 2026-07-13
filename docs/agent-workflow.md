# Agent workflow and providers

## Role of the agent

The agent is a researcher and source author, not a chemistry authority. It:

1. interprets a natural-language request;
2. identifies substances, quantities, conditions, and ambiguity;
3. researches evidence;
4. proposes a reaction hypothesis;
5. writes `.chems`;
6. receives validator diagnostics;
7. patches the source when a bounded repair is possible.

The deterministic validator remains authoritative.

## Visible workflow

The application displays concise, checkable action summaries:

```text
✓ Identified silver nitrate and sodium chloride
✓ Researched aqueous behaviour from 3 sources
✓ Predicted silver chloride precipitation
✓ Generated SilverChloridePrecipitation.chems
✗ Validation: nitrate charge written incorrectly
↻ Correcting the .chems program
✓ Validation passed
```

These entries represent product actions, tool events, evidence, and diagnostics.
They are not raw chain-of-thought.

## Provider selection

The startup selector always exposes two provider cards.

### Use Codex subscription

Requirements:

- a compatible `codex` executable on `PATH`;
- an active ChatGPT sign-in reported by `codex login status`;
- support for non-interactive execution, JSONL events, and output schemas.

Preflight is non-billing and does not inspect credential files:

1. locate `codex` or `codex.exe`;
2. run `codex --version` with a short timeout;
3. run `codex login status`;
4. capability-probe `codex exec --help` for required flags.

Use capability detection rather than an arbitrary minimum version. Possible UI
states:

- ready with ChatGPT subscription;
- installed but signed in with an API key;
- installed but signed out;
- installed but incompatible;
- not installed.

ChemSpec never reads `~/.codex/auth.json`.

Target non-interactive invocation:

```text
codex exec
  --model gpt-5.6
  --json
  --output-schema <result-schema.json>
  --sandbox read-only
  --ephemeral
  --ignore-user-config
  --skip-git-repo-check
  -C <empty-run-directory>
  -
```

The prompt is supplied through stdin. The process runs in a fresh empty
directory without filesystem write permission. ChemSpec parses JSONL events,
captures stderr separately, supports cancellation by terminating the child, and
writes returned source itself only after parsing the structured result.

Never use sandbox-bypass flags.

### Use OpenAI API key

This is a direct Responses API provider and has no Codex binary dependency. It
must not implement API-key mode by calling `codex login --with-api-key`, because
that would retain the external dependency and mutate the user's shared Codex
authentication.

The API key:

- stays in memory by default;
- may be saved only through the operating-system credential manager after an
  explicit choice;
- appears only in the HTTPS authorization header;
- never enters prompts, logs, provenance, `.chems`, or child-process
  environments.

The provider uses GPT-5.6, hosted web search, strict structured output, and
input/output moderation appropriate to the current OpenAI API.

## Provider-neutral interface

Conceptual interface:

```text
AgentProvider
  preflight()
  start(request, event_sink)
  repair(previous_result, diagnostics, event_sink)
  cancel(run_id)
```

Both providers emit the same `AgentEvent` values and produce the same
`ResearchResult`. All parsing, validation, provenance, and simulation behaviour
after source generation is provider independent.

## Evidence packet

Research output is structured around claims, not a general bibliography:

```text
Claim R1
  subject: AgNO3
  property: aqueous dissociation
  value: Ag+ + NO3-
  sources: [S1, S2]

Claim R2
  subject: AgCl
  property: aqueous solubility
  value: insoluble under stated conditions
  sources: [S2, S3]

Claim R3
  subject: AgCl
  property: appearance
  value: white precipitate
  sources: [S1]
```

Each source record preserves title, URL, publisher, retrieval time, and the
claims it supports. Search snippets alone are not evidence; the source must
actually support the claim.

Preferred source order:

1. scientific databases and standards bodies;
2. government and university references;
3. peer-reviewed publications;
4. established chemistry textbooks and educational resources;
5. other sources only as supplementary evidence.

Conflicting sources remain visible. The agent must not silently average or
erase a condition-dependent disagreement.

## Research versus catalogue authority

```text
Web research: AgCl is reported as insoluble.
Catalogue:     AgCl has the accepted insoluble-aqueous property.
Validator:     applicable precipitation rule found.
Result:        validated.
```

If only the research statement exists, the result remains unsupported. Live
research never mutates the active catalogue.

## Repair loop

Validator errors are returned in a stable machine-readable form:

```text
CHEM-E023 charge-mismatch
  at: expect.completeIonic.product[2]
  expected total charge: 0
  found total charge: -1
```

A repair input contains:

- the original request;
- the existing evidence packet;
- the current `.chems` source;
- validator diagnostics;
- an instruction to patch only invalid or unsupported claims.

The agent should not repeat web research unless a diagnostic identifies missing,
ambiguous, or conflicting evidence. Allow at most three repair attempts. After
that, stop with a transparent invalid or unsupported result.

Every patch is recorded and shown as a diff. Repair never changes the trusted
catalogue or validator configuration.

## Failure behaviour

Provider failures map into explicit product states:

- provider unavailable;
- authentication required;
- request cancelled;
- timed out;
- provider refused;
- malformed structured result;
- research incomplete;
- repair limit reached.

No provider failure is reinterpreted as a chemistry result.

## OpenAI references

- [Codex authentication](https://developers.openai.com/codex/auth)
- [Codex developer commands](https://developers.openai.com/codex/cli/reference)
- [Responses API web search](https://developers.openai.com/api/docs/guides/tools-web-search)
- [Responses API migration and structured output](https://developers.openai.com/api/docs/guides/migrate-to-responses)
- [OpenAI moderation](https://developers.openai.com/api/docs/guides/moderation)
