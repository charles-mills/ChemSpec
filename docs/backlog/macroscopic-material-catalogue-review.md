# Macroscopic material catalogue review

## Status

Partially completed for the exact phase-synthesis structures explicitly
qualified by the July 2026 implementation brief. The promoted catalogue digest
is `f5667dab4f4f2380f5e0c7ff48429b7720b14c7f65fa79e7749931fcfcd44fdd`.
This includes the hydrogen-halide rule-role bindings needed to distinguish
reacting red-brown bromine vapour from colourless gaseous hydrogen bromide.
Other proposed material records remain deferred until their own exact
structure bindings and evidence pass review.

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
self-attestation and is forbidden by the catalogue trust contract.

## Required completion path for the remaining proposal

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
