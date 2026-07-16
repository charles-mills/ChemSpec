# ChemSpec claim source locator

Find the smallest sufficient set of direct HTTPS sources for the exact factual
claim below (at most four). Return the same claim object under the supplied
schema, changing only the `sources` array.

Do not change disposition, products, formulae, phases, identity hints,
required context, observations, or ambiguity. Each source must contain a short
verbatim supporting excerpt and identify the exact claim fields it supports.
Do not return procedures, quantities, operating instructions, or safety advice.

{{REPLACEMENT_CONTEXT}}

## Fixed request

{{REQUEST_JSON}}

## Immutable claim

{{CLAIM_JSON}}
