# Luna catalogue candidate handoff

## Purpose

Luna may populate reviewed-design content after Codex has defined the wire
records, validation rules, and family semantics. Luna does not design chemistry
semantics, change code, or attest its own output. The authoring compiler turns
one or more three-file shards into a deterministic, executable review bundle
bound to one semantic catalogue digest.

## Exact Luna prompt

```text
Objective
Populate the assigned ChemSpec catalogue candidate content exactly as designed.

Authoritative design sections
docs/generalized-chemistry-rules.md
docs/generalized-rules-implementation-plan.md, Slice G6 only
docs/chems-specification.md, generalized catalogue and lowering sections

Allowed files and crates
Create or edit only one directory under catalogue/candidates/<assigned-id>/.
That directory must contain exactly candidate.json, example.chems, and evidence.json.
Do not edit any Rust crate, Cargo file, schema, grammar, conformance oracle,
trust root, generated output, documentation file, or another candidate package.

Required deliverables
candidate.json: only the assigned typed elements, categories, traits, templates,
applications, patterns, generalized rules, and their required evidence,
premises, valence records, or concrete supporting structures.
example.chems: one canonically formatted ordinary invocation of the assigned family.
evidence.json: one untrusted packet supporting only that example's observations.

Required positive fixtures
Use the assigned supported members and family cases exactly. Bind every record
to explicit source and premise identifiers. Do not infer missing structures,
states, sites, mappings, operations, cases, or applicability.

Required negative fixtures
Do not add negative files to the package. Report unsupported members, absent
domain features, ambiguous sites, or a design contradiction in your response;
the compiler and repository tests own negative fixtures.

Stable diagnostics or error classes
Treat CHEMS-A002/A004/A005 as package/schema/duplicate content errors,
CHEMS-Cxxx as catalogue integrity errors, CHEMS-Xxxx UnsupportedChemistry as a
real unsupported boundary, CHEMS-Kxxx as kernel rejection, and CHEMS-Fxxx as
frame rejection. Never work around one by weakening or deleting validation.

Explicit exclusions
No invented reaction family, runtime inference, best-effort structure, web-derived
unreviewed mutation, concrete rule duplication, automatic chemical review,
LLM attestation, production publication, trust-root edit, or generated artifact edit.

Focused verification commands
cargo run -p chems-cli -- catalogue check --out <new-empty-output-directory> \
  <all-assigned-candidate-package-directories>
cargo test -p chems-cli --test authoring

Repository-wide verification commands
cargo fmt --all -- --check
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
RUSTDOCFLAGS='-D warnings' cargo doc --workspace --no-deps
cargo test --workspace --doc
cargo run -p chems-conformance -- validate
git diff --check

Stop condition
Stop when the candidate command succeeds and emits one exact catalogue digest
with review-request status pending-chemist-review. Report the digest and every
unsupported or unresolved item. Do not claim approval or promotion.
```

## Closed candidate surface

`candidate.json` is a schema-versioned, unknown-field-denying shard. It may
carry only catalogue records: evidence, premises, valence premises, concrete
supporting structures/rules, element identities and categories, structural
traits, templates and applications, graph patterns, and generalized rules.
Publication identity, creation metadata, trust settings, validation limits,
attestations, paths, and output configuration are compiler-owned.
Every candidate premise must remain `provisional` with an empty reviewer list;
the compiler rejects self-asserted reviewed or rejected metadata.

The package filename set is closed. Symlinks, subdirectories, missing files,
and extra files fail before catalogue assembly. IDs used as generated output
components are restricted to safe lowercase kebab case.

## What the compiler proves

One `chems catalogue check` invocation:

1. loads every exact package and rejects duplicate package IDs;
2. rejects duplicate evidence, premises, element symbols/names/atomic numbers,
   structures, aliases, categories, traits, templates, applications, patterns,
   and concrete or generalized rules before merge;
3. sorts all unordered record collections and computes the semantic envelope
   digest;
4. runs the existing catalogue validator, including reference resolution,
   derived categories, template construction, traits, cases, and patterns;
5. expands each `example.chems`, preserving Unsupported and Ambiguous rather
   than guessing;
6. runs the existing concrete kernel and candidate-only frame projection; and
7. emits the merged envelope, digest, candidate inspection artifacts, and a
   digest-bound pending human review request.

Generated certificates, derivations, and frames are implementation-produced
inspection views, not independent conformance oracles.

## Human promotion boundary

The generated request lists every exact catalogue premise and evidence source,
the catalogue digest, and the digests of every inspection artifact. It is not a
`chem-catalogue-review-1` attestation and has no reviewer identity.

The resident chemist reviews the exact generated digest externally and, if
accepted, supplies a complete attestation bound to every premise. A maintainer
then deliberately updates the compiled catalogue and review trust roots. Only
the existing `TrustedCatalogue::from_canonical_json` API can accept those exact
host-pinned values. Luna, the candidate package, the authoring command, and its
generated outputs cannot perform that step.
