# Catalogue breadth execution plan

Status: authoritative execution plan for the four-family catalogue-breadth
queue defined in `docs/implementation-plan.md` ("Next catalogue breadth") and
the goal handoff that commissioned this work. Governed by
`docs/generalized-chemistry-rules.md`, `docs/generalized-rules-implementation-plan.md`
(G6 boundary), and `docs/chems-specification.md`.

Every family below is one closed candidate package under
`catalogue/candidates/<family-id>/` containing exactly `candidate.json`,
`example.chems`, and `evidence.json`, checked together with the existing
`periodic-table-and-alkali-water` package and every previously completed
family package. No package duplicates an existing element record, category,
template, pattern, rule, or premise; each references the shared element
registry and, where chemically identical, an already-declared category,
template, pattern, or structure application from an earlier package in the
queue.

Sources used throughout (retrieved 2026-07-15):

- OpenStax, *Chemistry 2e*, <https://openstax.org/details/books/chemistry-2e>
  — precipitation/solubility rules, acid-base neutralization, carbonate-acid
  gas evolution, activity series and single displacement, formal charge and
  Lewis structures. Limited claim supported: qualitative reaction outcomes,
  solubility classifications, and standard secondary/undergraduate reaction
  equations for the species named below.
- IUPAC, *Compendium of Chemical Terminology (the Gold Book)*,
  <https://goldbook.iupac.org/> — terminology for ionic bond, formal charge,
  dissociation, precipitate, neutralization. Limited claim supported:
  terminology only, not specific reaction outcomes.
- IUPAC, *Periodic Table of the Elements*,
  <https://iupac.org/what-we-do/periodic-table-of-elements/> — element
  identity (reused from the existing 118-element registry; no new element
  records are added by this queue).

No laboratory procedure, quantity, optimization, or hazard-bypass guidance is
added anywhere in this queue, consistent with `docs/safety.md`.

## Shared building blocks across families

- `Categories.AlkaliMetal` (existing, `periodic-table-and-alkali-water`): Li,
  Na, K. Reused unchanged by families 2 and 4.
- `Categories.Halide` (new, defined by family 1): explicit members
  `{F, Cl, Br, I}`. At-and-below-francium-period halogens (At, Ts) are
  excluded from the category itself rather than left as an implicit
  Cartesian binding, because they have no meaningful aqueous/simple-salt
  chemistry at this educational level. F is a declared member but every
  family that uses the category carries an explicit `unsupported` case for
  it (halogen-specific exceptions below), so its exclusion from supported
  chemistry is visible in the catalogue, not just in prose.
- Every family's rewrite is expressed with the existing closed structural
  operation set only (`cleave_covalent`, `form_covalent`, `cleave_dative`,
  `form_dative`, `change_covalent`, `associate_ionic`, `dissociate_ionic`,
  `release_metallic`, `join_metallic`, `transfer_electron`,
  `assign_product`). No new operation kind is proposed.

## Family 1 — Precipitation (`precipitation-silver-halide`)

### Domain

One generalized rule, `Rules.SilverHalidePrecipitation`, over a single
parameter `X : Categories.Halide`.

- Supported: `X ∈ {Cl, Br, I}` — silver chloride, silver bromide, and silver
  iodide are all classically insoluble precipitates from mixing soluble
  silver nitrate with a soluble alkali-metal halide (OpenStax 2e,
  solubility-rules table).
- Unsupported (explicit case): `X = F` — silver fluoride is soluble, so the
  precipitation outcome does not hold; this is a genuine domain feature gap
  (`Features.SilverFluorideSolubility`), not a missing case.
- Uncovered (implicit Unsupported, no case needed): every non-halide binding
  is outside the parameter's category and cannot bind at all.

The halide source's counter-cation is fixed to sodium for the one worked
case (`example.chems`); the rule's `saltSource`/product templates are
parameterized only over `X`, not over the alkali metal, keeping the domain
small and exact rather than adding an unused second parameter dimension.

### Representative equation and structural outcome

```
AgNO3(aq) + NaX(aq) -> AgX(s) + NaNO3(aq)      X in {Cl, Br, I}
```

Structurally this is pure ionic re-association: `SilverNitrate`'s Ag+
component and the halide source's X- component swap partners with the
sodium/nitrate pair. No covalent bond, electron, or atom count changes.
Rewrite: `dissociate_ionic` (silver nitrate's association),
`dissociate_ionic` (sodium halide's association), `associate_ionic` (Ag+ with
X-, the precipitate), `associate_ionic` (Na+ with the conserved nitrate
group, the spectator salt), `assign_product` x2.

### Structures, templates, applications (new)

- `Templates.AlkaliMetalHalide<member, halide>` (ionic): `member+` cation
  component (`Categories.AlkaliMetal`), `halide-` monatomic anion component
  (`Categories.Halide`), one association. Reused as the halide-source
  reactant here and as a product template in families 2 and 3.
- `Templates.SilverHalide<halide>` (ionic): fixed `Ag+` cation component,
  `halide-` anion component, one association. Product template (the
  precipitate).
- `SilverNitrate` (concrete ionic structure): `Ag+` cation component plus a
  `NO3-` anion component (one canonical Lewis structure: N double-bonded to
  one O, single-bonded to two O-, matching formal charges +1/0/-1/-1
  summing to -1). Concrete because it never varies across the family.
- `Templates.AlkaliMetalNitrate<member>` (ionic): `member+` cation plus the
  same `NO3-` anion shape. Product template (the spectator salt).
- Worked application set for `example.chems` (M = Na, X = Cl):
  `SodiumChloride` (halide source reactant), `SilverChloride` (precipitate
  product), `SodiumNitrate` (spectator product). These three application IDs
  are declared once here and reused by later families instead of being
  redeclared.

### Observations and evidence

- `product precipitate forms claim R1` (predicate `forms`).
- `product precipitate colour claim R2` (predicate `colour`, silver chloride
  is white).
- Evidence packet cites OpenStax 2e solubility-rules coverage.

### Package layout

`catalogue/candidates/precipitation-silver-halide/{candidate.json,
example.chems, evidence.json}`.

### Focused validation

```sh
cargo run -p chems-cli -- catalogue check --out <tmp> \
  catalogue/candidates/periodic-table-and-alkali-water \
  catalogue/candidates/precipitation-silver-halide
cargo test -p chems-cli --test authoring
cargo test -p chem-catalogue
```

### Checklist

- [ ] `Categories.Halide` validates with exactly `{F, Cl, Br, I}`.
- [ ] Supported case covers exactly `{Cl, Br, I}`; `F` is an explicit
      unsupported case; catalogue check succeeds.
- [ ] `example.chems` (Na/Cl) expands, validates, executes, produces frames.
- [ ] Package order-independence: swapping package argument order produces
      an identical `catalogue.digest`.
- [ ] `cargo test -p chems-cli --test authoring` and
      `cargo test -p chem-catalogue` pass.

## Family 2 — Acid-base neutralization (`acid-base-neutralization`)

### Domain

One generalized rule, `Rules.MonoproticAcidHydroxideNeutralization`, over
two parameters: `M : Categories.AlkaliMetal` (base) and
`X : Categories.Halide` (acid halogen).

- Supported: `X ∈ {Cl, Br, I}`, all `M`, uniform single case (mechanism is
  identical regardless of member) — 9 combinations (HCl/HBr/HI with
  LiOH/NaOH/KOH).
- Unsupported (explicit case): `X = F` — hydrofluoric acid is weak (partial
  dissociation equilibrium), which is a different reaction model
  (`Features.WeakAcidEquilibrium`) from the strong, fully-dissociating acids
  this family represents; it is out of the current structural domain, not
  merely uncovered.

### Representative equation and structural outcome

```
HX(aq) + MOH(aq) -> MX(aq) + H2O(l)        X in {Cl, Br, I}, M in {Li, Na, K}
```

Mechanism (mirrors the proton-transfer/dative pattern already reviewed for
`Rules.AlkaliMetalWithWater`'s hydrogen chemistry):

1. `cleave_covalent` H-X heterolytic to X — X becomes a halide anion
   (charge -1, 4 lone pairs), H becomes a bare proton (charge +1, no
   electrons).
2. `form_dative` donor = hydroxide oxygen's lone pair, acceptor = the bare
   proton — the hydroxide's O-H group gains a second O-H bond and becomes
   neutral: this is exactly `Water`'s existing oxygen state
   (charge 0, 2 lone pairs, bond-order sum 2), so the product reuses the
   existing trusted `Water` structure unmodified.
3. `associate_ionic` M+ with X- — the new salt.
4. `assign_product` x2.

### Structures, templates, applications (new)

- `Templates.HydrogenHalide<halide>` (molecular): H-X single covalent bond,
  both neutral. Applications: `HydrogenChloride`, `HydrogenBromide`,
  `HydrogenIodide`.
- Product salt reuses `Templates.AlkaliMetalHalide<member, halide>` from
  family 1 (no redeclaration); the worked example's product application
  `SodiumChloride` is the exact one declared by family 1.
- Product water reuses the existing trusted `Water` structure by ID (no
  redeclaration).
- Reactant hydroxide reuses the existing trusted
  `Templates.AlkaliMetalHydroxide<member>` (and, for the worked example, the
  existing `SodiumHydroxide` application) from `periodic-table-and-alkali-water`.

### Observations and evidence

- `reactant acid disappears claim R1` (predicate `disappears`).
- `product water forms claim R2` (predicate `forms`).
- Evidence packet cites OpenStax 2e acid-base neutralization coverage.

### Focused validation

```sh
cargo run -p chems-cli -- catalogue check --out <tmp> \
  catalogue/candidates/periodic-table-and-alkali-water \
  catalogue/candidates/precipitation-silver-halide \
  catalogue/candidates/acid-base-neutralization
cargo test -p chems-cli --test authoring
cargo test -p chem-catalogue
```

### Checklist

- [ ] `Templates.HydrogenHalide` instantiates HCl/HBr/HI into distinct exact
      graphs.
- [ ] Supported case covers `{Cl, Br, I} x {Li, Na, K}`; `F` is explicit
      unsupported.
- [ ] `example.chems` (HCl + NaOH) expands to the existing `Water` structure
      exactly (no drift in O/H electron states) and to family 1's
      `SodiumChloride` application.
- [ ] `cargo test -p chems-cli --test authoring` and
      `cargo test -p chem-catalogue` pass; merged check with families 1+2
      succeeds and is order-independent.

## Family 3 — Acid-carbonate gas evolution (`acid-bicarbonate-gas-evolution`)

### Domain

One generalized rule, `Rules.MonoproticAcidBicarbonateGasEvolution`, over
`M : Categories.AlkaliMetal` and `X : Categories.Halide`, restricted the
same way as family 2 (`F` unsupported for the same weak-acid reason).
Scoped to **bicarbonate** (hydrogen carbonate), not the fully deprotonated
carbonate ion: bicarbonate + monoprotic acid is the standard secondary-level
"fizzing"/limewater gas test (OpenStax 2e), and it halves the number of
proton-transfer steps relative to carbonate without losing the target
chemistry. Full carbonate (e.g. Na2CO3) is an explicit unsupported case
below rather than a silently missing combination.

- Supported: `X ∈ {Cl, Br, I}`, all `M`.
- Unsupported (explicit case): `X = F` (same weak-acid reason as family 2).
- Explicit unsupported boundary (documented, not a parameter of this rule):
  fully deprotonated carbonate salts (e.g. Na2CO3, K2CO3) are out of scope
  for this rule; they would need a second protonation step (an additional
  `form_dative`/`cleave_covalent` pair) that is a straightforward but
  separate extension, deliberately deferred to keep this family's domain
  small and exact.

### Representative equation and structural outcome

```
HX(aq) + MHCO3(aq) -> MX(aq) + H2O(l) + CO2(g)     X in {Cl, Br, I}, M in {Li, Na, K}
```

Bicarbonate Lewis structure: C bonded to O_A (double, carbonyl, neutral, 2
lone pairs), O_B (single, -OH, neutral, carries H_B), O_C (single, charge
-1, 3 lone pairs, no H). This is a legal, charge-consistent Kekule structure
(one canonical resonance form, consistent with the domain's no-resonance
rule) with total charge -1, matching HCO3-.

Mechanism (six new rewrite operations plus the acid-cleave and salt
association already used by family 2):

1. `cleave_covalent` H-X heterolytic to X — as family 2.
2. `form_dative` donor = O_C lone pair, acceptor = the bare proton — O_C
   becomes a second neutral -OH group (now structurally equivalent to fully
   protonated carbonic acid, H2CO3, transient and never assigned to a
   product).
3. `cleave_covalent` C-O_B heterolytic to O_B — O_B fully detaches from
   carbon, keeping H_B; O_B becomes a free hydroxide-like fragment (charge
   -1, 3 lone pairs, single bond to H_B only). C drops to bond-order sum 3,
   becoming a transient cationic carbon (charge +1) — a standard organic
   carbocation-like intermediate, reviewed and bound to its own valence
   premise entry, not asserted as a stable species.
4. `cleave_covalent` O_C-H_new heterolytic to O_C — undoes step 2's bond,
   giving O_C back both electrons (charge -1, 3 lone pairs, single bond to C
   only) and leaving the just-added proton bare again.
5. `change_covalent` C-O_C single to double, with O_C's freed lone pair
   supplying the second bonding pair — O_C becomes the second carbonyl
   oxygen (charge 0, 2 lone pairs, bond-order sum 2) and C returns to
   neutral, bond-order sum 4: this is exactly `CO2`'s structure (O_A=C=O_C).
6. `form_dative` donor = O_B's remaining lone pair, acceptor = the bare
   proton freed in step 4 — O_B gains a second H (H_new), becoming neutral
   water (bond-order sum 2, 2 lone pairs): exactly the existing `Water`
   structure's oxygen state.
7. `associate_ionic` M+ with X- — the new salt.
8. `assign_product` x3 (CO2, water, salt).

Every intermediate electron state above is arithmetically self-consistent
under the formal-charge equation and is bound to a new valence-premise entry
authored by this package (the carbocation and free-hydroxide-fragment
states are standard, reviewable intermediates, not novel chemistry); no step
invents an operation outside the closed set, and total atom/electron/charge
conservation holds end to end (verified by the kernel's per-operation
conservation check, not merely asserted here).

### Structures, templates, applications (new)

- `Templates.AlkaliMetalBicarbonate<member>` (ionic): `member+` cation
  component plus a bicarbonate anion component (C, O_A, O_B+H_B, O_C as
  above). Reactant template.
- Products reuse `Templates.AlkaliMetalHalide` (family 1) and the existing
  trusted `Water` (unchanged) and a new concrete `CarbonDioxide` structure
  (O=C=O, both oxygens neutral carbonyl states, formula CO2).
- `Templates.HydrogenHalide` reused from family 2 (no redeclaration).
- Worked application for `example.chems` (M = Na, X = Cl):
  `SodiumBicarbonate` (reactant), reuses family 1's `SodiumChloride`
  (product) and the existing `Water`; `CarbonDioxide` is declared once here.

### Observations and evidence

- `gas carbonDioxide evolves claim R1` (predicate `evolves`, product side,
  molecular representation).
- `reactant acid disappears claim R2`.
- Evidence packet cites OpenStax 2e acid-carbonate gas-evolution coverage
  and the standard secondary-level CO2 gas test.

### Focused validation

```sh
cargo run -p chems-cli -- catalogue check --out <tmp> \
  catalogue/candidates/periodic-table-and-alkali-water \
  catalogue/candidates/precipitation-silver-halide \
  catalogue/candidates/acid-base-neutralization \
  catalogue/candidates/acid-bicarbonate-gas-evolution
cargo test -p chems-cli --test authoring
cargo test -p chem-catalogue
```

### Checklist

- [ ] Bicarbonate template validates with the declared charge/lone-pair
      accounting for O_A/O_B/O_C.
- [ ] Every transient intermediate state (carbocation, free hydroxide
      fragment) is bound to a reviewed valence-premise entry in this
      package, not borrowed from another package's premise.
- [ ] `example.chems` (HCl + NaHCO3) expands to `CarbonDioxide`, the
      existing `Water`, and family 1's `SodiumChloride` exactly.
- [ ] Full-carbonate salts remain outside this rule's parameter domain
      (documented, not silently absent).
- [ ] `cargo test -p chems-cli --test authoring` and
      `cargo test -p chem-catalogue` pass; merged check with families 1-3
      succeeds and is order-independent.

## Family 4 — Single displacement (`single-displacement-alkali-metal`)

### Domain and scope decision

One generalized rule, `Rules.AlkaliMetalActivitySeriesDisplacement`, over
three parameters: `member : Categories.AlkaliMetal` (displacing metal,
reuses the exact parameter name `member` so the existing trusted
`Templates.ElementalAlkaliMetal` / `Patterns.AlkaliMetal` are reused
unmodified), `displaced : Categories.AlkaliMetal` (metal displaced from its
halide salt), and `X : Categories.Halide`.

**Scope is deliberately restricted to alkali-metal-on-alkali-metal-halide
displacement**, not the more commonly illustrated Zn/Fe/Cu/Mg activity
series. This is a documented, evidence-backed domain boundary, not an
oversight:

- The existing closed structural operation set's `release_metallic` and
  `join_metallic` each move **exactly one** delocalized electron per call,
  and each permanently detaches or attaches the named site to its domain in
  that same call (verified directly against `chem-kernel`'s operation
  semantics: a released site cannot be re-released, a joined site cannot be
  re-joined, and an emptied single-site domain is forced to zero electrons
  in the same step). A monovalent metal (one delocalized electron per
  elemental site, exactly the alkali-metal case already proven by
  `Rules.AlkaliMetalWithWater`) is fully expressible this way. A divalent
  metal (two delocalized electrons per elemental site, e.g. Zn, Fe(II), Cu,
  Mg — the metals conventionally used to teach the activity series) is
  **not**: extracting or depositing a second electron for the same site has
  no expressible legal operation sequence that both (a) leaves every
  intermediate atom in a reviewable, charge-consistent state and (b)
  conserves total electrons, because the only two operations that change
  domain membership are single-electron, single-shot, and per-site.
- This was tested directly against the existing (unused) `CalciumMetal`
  valence-premise fixture in `periodic-table-and-alkali-water`, which
  declares divalent `metallic_domain_states` but is never exercised by any
  rule — consistent with this being a real, previously-uncrossed boundary
  of the current kernel, not a gap specific to this family.
- Reusing alkali metals keeps every operation in this family's rewrite
  identical in shape to the already-reviewed `Rules.AlkaliMetalWithWater`
  rewrite (`release_metallic` -> `transfer_electron` -> `join_metallic`),
  which is the smallest, most structurally exact way to deliver a genuine
  "explicitly bounded activity-series domain" family per the design
  conservatism directive, instead of leaving family 4 unbuilt.

This is recorded here as a design-boundary finding, consistent with the
"document the contradiction... leave completed families intact" guidance:
the family *is* implemented, but its domain is bounded to what the current
kernel operation set can express without inventing a new operation kind or
weakening validation. Divalent-metal single displacement is an explicit
unsupported extension for future kernel work (a new or generalized
multi-electron metallic-transfer operation), not something this package
approximates.

The relative reactivity itself is standard, reviewed chemistry (the group 1
reactivity trend K > Na > Li is IUPAC/OpenStax-sourced); the package frames
the outcome as a theoretical/virtual simulation of the activity-series
displacement principle (consistent with `docs/safety.md`'s "virtual
educational model" framing), not a literal recommended aqueous bench
procedure — alkali metals reacting with any water of hydration/solvation is
a known confound the prose and evidence explicitly flag rather than silently
ignore.

- Supported: `member, displaced ∈ {K, Na, Li}` with `member` strictly more
  reactive than `displaced` per K > Na > Li (`(K,Na)`, `(K,Li)`, `(Na,Li)`),
  `X ∈ {Cl, Br, I}` — 9 combinations.
- Unsupported (uncovered, no case needed): `member == displaced` and every
  reversed/equal-reactivity pair (e.g. `(Li,Na)`) are outside the case's
  `when` predicate and remain implicitly Unsupported.
- Unsupported (explicit case): `X = F`, same weak/soluble-halide reasoning
  extended for consistency (`Features.SilverFluorideSolubility` does not
  apply here directly, but AgF-style solubility exceptions for fluoride
  salts are common enough that F is excluded from every family in this
  queue for a consistent, documented reason: fluoride salts' solubility and
  bonding behavior diverge from Cl/Br/I often enough that this queue treats
  F as its own reviewed case throughout).

### Representative equation and structural outcome

```
member(s) + displaced-X(aq) -> member-X(aq) + displaced(s)
e.g. K(s) + NaCl -> KCl + Na(s)
```

Rewrite (identical shape to `Rules.AlkaliMetalWithWater`'s metallic
chemistry): `dissociate_ionic` (salt source), `release_metallic` (displacing
metal, `retain_electron` — becomes a neutral radical, mirrors the existing
lithium/water fixture exactly), `transfer_electron` (displacing radical to
the freed displaced-metal cation — displacing metal becomes M+, displaced
metal becomes a neutral radical), `join_metallic` (displaced metal's
radical electron donated into a fresh single-site domain — becomes the
elemental displaced metal, exactly the existing alkali-metal elemental
template shape), `associate_ionic` (displacing M+ with X-, the new salt),
`assign_product` x2.

### Structures, templates, applications (new)

- No new templates: reuses `Templates.ElementalAlkaliMetal` and
  `Patterns.AlkaliMetal` (existing, unmodified) for both the displacing
  metal reactant and the displaced metal product, and
  `Templates.AlkaliMetalHalide` (family 1) for the salt reactant/product.
- A new pattern, `Patterns.AlkaliMetalHalideSaltSource`, is required for the
  `saltSource` reactant role because its element parameter must bind to
  this rule's own `displaced` parameter name (patterns bind by parameter
  name, so family 1's identically-shaped pattern — bound to family 1's own
  parameter names — cannot be reused verbatim here).
- Worked application for `example.chems` (member = K, displaced = Na,
  X = Cl): reuses family 1's `SodiumChloride` (reactant) and the existing
  base package's `PotassiumMetal`/`SodiumMetal` (already declared); the new
  product application is `PotassiumChloride`.

### Observations and evidence

- `product displacedMetal forms claim R1` (predicate `forms`).
- `reactant saltSource disappears claim R2`.
- Evidence packet cites OpenStax 2e's group 1 reactivity-trend coverage,
  explicitly scoped to the relative-reactivity principle rather than a
  specific lab procedure.

### Focused validation

```sh
cargo run -p chems-cli -- catalogue check --out <tmp> \
  catalogue/candidates/periodic-table-and-alkali-water \
  catalogue/candidates/precipitation-silver-halide \
  catalogue/candidates/acid-base-neutralization \
  catalogue/candidates/acid-bicarbonate-gas-evolution \
  catalogue/candidates/single-displacement-alkali-metal
cargo test -p chems-cli --test authoring
cargo test -p chem-catalogue
```

### Checklist

- [ ] Case covers exactly the 9 valid `(member, displaced, X)` triples;
      reversed/equal-reactivity pairs remain Unsupported.
- [ ] `example.chems` (K + NaCl) expands, validates, executes, produces
      frames; reuses `SodiumChloride` and the existing elemental metal
      applications exactly.
- [ ] Divalent-metal exclusion is documented in this plan and in the final
      handoff, with the concrete kernel-operation evidence above, not left
      implicit.
- [ ] `cargo test -p chems-cli --test authoring` and
      `cargo test -p chem-catalogue` pass; merged check with all four
      families succeeds and is order-independent.

## Cross-family final check

After all four families are implemented and individually committed:

```sh
cargo run -p chems-cli -- catalogue check --out <new-empty-tmp> \
  catalogue/candidates/periodic-table-and-alkali-water \
  catalogue/candidates/precipitation-silver-halide \
  catalogue/candidates/acid-base-neutralization \
  catalogue/candidates/acid-bicarbonate-gas-evolution \
  catalogue/candidates/single-displacement-alkali-metal
```

run twice with the package arguments in reversed order, confirming identical
`catalogue.digest` both times, before proceeding to the repository-wide
gates listed in the goal handoff.

## Completion checklist (whole queue)

- [ ] Family 1 (precipitation) implemented, tested, committed.
- [ ] Family 2 (acid-base) implemented, tested, committed.
- [ ] Family 3 (acid-carbonate gas evolution) implemented, tested, committed.
- [ ] Family 4 (single displacement) implemented, tested, committed, with
      its divalent-metal scope boundary documented.
- [ ] Merged five-package catalogue check succeeds, order-independently.
- [ ] All repository-wide gates in the goal handoff pass.
- [ ] `docs/catalogue-breadth-review-handoff.md` written with exact digests
      and commit list.
