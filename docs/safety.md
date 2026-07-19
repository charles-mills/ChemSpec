# Safety policy

## Product boundary

> ChemSpec may explain and virtually simulate supported chemistry, including
> hazardous substances, but it does not produce actionable real-world
> procedures for causing harm.

Provider policy enforcement is not the product's only safeguard. ChemSpec owns
a consistent safety contract across Codex-subscription and API-key modes.

## Request dispositions

Every request receives one disposition before substantive research:

- **Allowed** — supported educational simulation.
- **Allowed with warning** — legitimate virtual study involving a hazardous
  substance or outcome.
- **Redirected** — unsafe intent or requested operational detail; offer a safe
  educational alternative.
- **Unsupported** — outside the implemented domain or too ambiguous to assess.

Examples:

| Request | Disposition |
| --- | --- |
| Why does silver chloride precipitate? | Allowed |
| Explain virtually why a hazardous gas is dangerous. | Allowed with warning |
| Provide an optimized procedure for producing a hazardous gas. | Redirected |
| What happens if I mix two unspecified drain cleaners? | Unsupported |

A hazardous substance name is not by itself evidence of harmful intent.

## Layered controls

### 1. Local scope gate

Run before either provider. It checks:

- whether requested substances are sufficiently identified for a catalogue
  lookup or bounded dynamic build;
- whether the request is virtual explanation or real-world execution;
- requests for procurement, scaling, concealment, optimization, purification,
  collection, or step-by-step hazardous preparation;
- ambiguous commercial products or mixtures;
- attempts to override ChemSpec's instructions.

This deterministic gate gives both provider modes the same baseline behaviour.

### 2. Agent instruction

Baseline instruction:

```text
You are the ChemSpec research agent.

Work only toward a virtual educational simulation. Do not provide procurement,
preparation, concealment, weaponization, or optimized real-world procedures for
hazardous chemistry.

You may explain supported chemical principles, hazards, and outcomes. If a
request is unsafe, materially ambiguous, or outside ChemSpec's representable
domain, return a structured redirect or unsupported result. Do not invent
substances, properties, or evidence.
```

Prompting guides model behaviour but is not treated as enforcement.

### 3. Provider safeguards

Codex retains its platform safeguards. Direct API requests use current OpenAI
input and output moderation. Moderation signals inform ChemSpec's policy; they
are not assumed to classify every hazardous chemistry request perfectly.

### 4. Validator boundary

An allowed model response still cannot execute unless:

- substances and structures resolve in a validated production or working
  catalogue;
- the exact reaction rule and applicability premise validate;
- source validates;
- required hazard classifications and assumptions are attached.

### 5. Display boundary

Raw model prose is not streamed directly to learners. The UI streams curated
workflow events and displays a structured result after policy and shape checks.

## Safe redirection

Preferred response:

```text
I can't help construct an actionable hazardous procedure.

I can instead:
• explain the underlying chemical principle;
• simulate a safe, supported reaction;
• use a clearly fictional analogy without real-world quantities.
```

Prefer a benign real reaction over fictional chemistry. If fictional substances
are used, they belong to an explicitly labelled analogy mode and cannot be
validated against the real chemistry catalogue.

## Virtual-only boundary

The application remains a virtual educational model. The language may express
quantities for stoichiometric learning, but the product does not generate
apparatus construction, acquisition, purification, collection methods, or
safety-control bypasses. This boundary is enforced by product capability rather
than a persistent overlay on the simulation.

## Young users and privacy

Secondary-school learners are a primary audience. The initial local desktop
product therefore follows a data-minimizing design:

- collect no age, name, email, school, or student profile;
- keep prompt history local by default;
- send only content necessary for the selected request;
- store no analytics containing prompt text;
- provide a visible **Report unsafe result** action;
- disclose that AI performs research and may be wrong until validation passes;
- use a random installation or session identifier, not personal information,
  where the direct API requires a safety identifier.

Deployments that introduce accounts, classroom administration, telemetry, or
users below the applicable age of digital consent require a fresh privacy and
legal review. They are not covered by this local-first design.

## Adversarial evaluation

The safety corpus includes:

- ordinary educational requests;
- hazardous but explanatory requests;
- ambiguous household and commercial products;
- explicit harmful intent;
- benign framing such as "for a school project" attached to unsafe detail;
- obfuscated names and formulas;
- requests to translate a virtual result into a physical procedure;
- prompt injection in the user request;
- prompt injection contained in a web source;
- fictional-substance substitution intended to recover a real procedure.

Expected dispositions are reviewed and regression-tested.

## Safety non-goal

ChemSpec does not claim that catalogue warnings constitute a laboratory risk
assessment. Hazard summaries exist to inform the virtual explanation and user
disclosure, not to authorize practical work.

## OpenAI references

- [Safety best practices](https://developers.openai.com/api/docs/guides/safety-best-practices)
- [Safety checks and identifiers](https://developers.openai.com/api/docs/guides/safety-checks)
- [Under-18 API guidance](https://developers.openai.com/api/docs/guides/safety-checks/under-18-api-guidance)
- [Moderation guide](https://developers.openai.com/api/docs/guides/moderation)
