# Macroscopic material catalogue review

## Status

Deferred at the July 2026 `origin/main` integration because the proposed
catalogue mutation had no separate review attestation. It must not be copied
into `catalogue/reference/` or added to the application trust pins until the
ordinary candidate, review, and promotion workflow is complete.

## Proposed content

The upstream change produced catalogue digest
`21554e27814f572ceee6aba9f5b1a22c42ac02a2fee3901d66e19aa783d19334` and
added standard-phase material records for:

- elemental carbon as a solid;
- carbon dioxide as a gas;
- molecular hydrogen as a gas;
- molecular oxygen as a gas; and
- water as a liquid.

The proposal cited five NIST Chemistry WebBook records and introduced the
corresponding `premise.material.*.standard-phase` premises. The existing review
artifact remains bound to catalogue digest
`9622e4605ca0a5762e601e5876526612cac6eda708bfe4c37cb3d4517add9cf2` and does
not contain those evidence or premise IDs. Re-pinning that artifact would be a
self-attestation and is forbidden by the catalogue provenance contract.

## Required completion path

1. Preserve the proposed records as an untrusted authoring candidate and run
   the catalogue structural checks.
2. Obtain a separate host-selected review of the exact generated digest,
   evidence set, premise set, scope, and material-to-structure bindings.
3. Promote the candidate with that attestation and update both application
   digest pins in the same change.
4. Restore live catalogue assertions for the five reviewed phases and the
   static oxygen-family surface-oxidation profiles.

Renderer and runtime oxide-colour tests use explicit typed material fixtures or
already validated dynamic outcomes in the meantime. Unknown catalogue phases
remain unknown and cannot silently select a macroscopic process.
