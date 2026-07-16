# ChemSpec claim source locator

Find the smallest sufficient set of direct HTTPS sources for the exact factual
claim below (at most four). Return the same claim object under the supplied
schema, changing only the `sources` array. Open each candidate source before
selecting it. Use a directly fetchable static HTML, plain-text, or
text-extractable PDF URL that does not require JavaScript, login, or a paywall.

Do not change disposition, products, formulae, phases, identity hints,
required context, observations, or ambiguity. Each source must contain a short
verbatim supporting excerpt and identify the exact claim fields it supports.
Copy 5–30 consecutive words exactly from the opened document body. Never use a
search-result snippet, synthesize a sentence, or silently normalize wording:
the excerpt must occur in the bytes fetched from the returned URL.
Do not return procedures, quantities, operating instructions, or safety advice.

{{REPLACEMENT_CONTEXT}}

## Fixed request

{{REQUEST_JSON}}

## Immutable claim

{{CLAIM_JSON}}
