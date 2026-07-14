# Archived Slice 3 review handoff

This document formerly described the lithium-only external review gate. That
trust root has been superseded by the generalized catalogue in
`catalogue/trusted/periodic-table-and-alkali-water/`.

The current source-controlled trust decision consists of:

- the deterministic candidate package under
  `catalogue/candidates/periodic-table-and-alkali-water/`;
- the separate host-selected AI attestation under `catalogue/reviews/`;
- the generated catalogue, review, and promotion manifest under
  `catalogue/trusted/`; and
- the catalogue and review semantic digests compiled into `TrustedCatalogue`.

The attestation explicitly states that AI review can be wrong and is not human
expert certification. It covers 118 element identity records but runnable
chemistry only for Li, Na, and K reacting with water through the one generalized
family. Candidate packages and runtime agents cannot extend the compiled trust
root.
