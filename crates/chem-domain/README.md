# `chem-domain`

`chem-domain` is ChemSpec's pure deterministic value layer. The definitive
structural API contains no source parser, catalogue loading, rule application,
graph execution, networking, application state, or rendering.

## Structural core

`structural` provides:

- separate typed IDs for structures, atoms, groups, covalent bonds, ionic
  associations, metallic domains, expanded instances, rules, operations,
  mappings, evidence packets, claims, and premises;
- atom-local formal charge, non-bonding electrons, and unpaired electrons;
- the closed `single | double | triple` localized covalent order domain, with
  shared or directed dative electron origin on single bonds;
- canonical nonempty atom groups and many-body ionic associations;
- explicit metallic site and delocalized-electron-domain ownership;
- privately constructed immutable `StructuralGraph` values;
- normalized formula inventories checked against complete graphs;
- catalogue-level structure definitions and definition-derived, totally
  relabelled reaction-side instances;
- total bijective element-preserving atom mappings;
- the closed privately constructed structural-operation value set with exact,
  canonical endpoint states; and
- canonical JSON and SHA-256 semantic digests.

Graphs store identities in ordered maps and relationship memberships in ordered
sets. Declaration order therefore cannot change equality, canonical bytes, or
digests. Formula inventories must equal graph element inventories and never
replace graph identity; formula-equal structural isomers remain unequal.

Construction rejects empty or duplicate identities, self/duplicate/unknown
covalent edges, invalid dative endpoints or bond orders, empty or repeated
group membership, overlapping/non-neutral
ionic association components, multiply owned metallic sites, and simultaneous
site-local/domain-owned electrons. Validated graph fields are private and the
types intentionally do not deserialize directly; later trusted-boundary crates
deserialize versioned records and call these constructors.

## Electron accounting

Atom-local electron state enforces valid paired/unpaired counts. Given a
reviewed neutral-valence premise, it evaluates:

```text
formal_charge = neutral_valence - non_bonding - covalent_bond_order_sum
```

Graphs report atom formal-charge sum, covalent and local electron ownership,
delocalized-domain electron count, explicit valence-electron total, and:

```text
system_net_charge = atom_formal_charge_sum - domain_electron_count
```

## Migration boundary

The earlier quantity, material, vessel, and procedure-oriented modules remain
only for unrelated application work and repository archaeology. The catalogue,
kernel, CLI, and frame pipeline do not consume them. They are not part of the
definitive `.chems 1` contract or a compatibility language.

## Verification

`tests/structural.rs` covers constructor failure classes, all relationship
kinds, electron and charge accounting, formula-equal structural isomers,
reaction-side atom identity, mapping totality/bijection, shared and dative
operation endpoint states, definition-derived instance relabeling, canonical
serialization, digests, generated `proptest` properties, and the promoted
structural conformance fixtures under
`conformance/structural-domain`.
