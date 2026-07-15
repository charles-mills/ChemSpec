# Compound structural resolution

The reactant builder does not maintain a formula-to-compound switch. Every
multi-atom draft is resolved against the host-pinned structural catalogue used
by generalized reaction elaboration.

Resolution has three outcomes:

- one unique reviewed graph: the reactant is structurally valid;
- no reviewed graph: the viewer displays the unknown `?` particle;
- more than one non-equivalent graph for the same atom inventory: resolution
  is ambiguous and remains unknown until the interface can select an isomer.

Reaction selection is a second boundary. A structurally valid reactant does not
imply that a reaction occurs. The selected pair must also match an installed
reviewed reaction experience before trusted simulation frames are available.

Ozone is included as `Ozone`/`O3`: a bent three-oxygen molecular graph with a
canonical `-1/+1/0` formal-charge contributor and one resonance-delocalisation
domain across both O-O edges. The reactant viewer renders both edges with an
effective bond order of `3/2`. Ozone reactions require their own reviewed atom
mappings; O2 frames are never silently reused for O3.

The installed catalogue now supplies those separate mappings for each of its
68 existing oxygen-product choices. Each O3 rule first cleaves both authored
ozone bonds, neutralises the canonical formal-charge contributor through an
explicit electron transfer, and then reuses the general product-forming
operation vocabulary. The resulting frames are generated and trusted through
the same kernel boundary as every other simulation.

This boundary applies equally to water, hydroxides, binary ionic compounds and
future covalent molecules. Adding a new catalogue graph makes the composition
resolvable; adding a reaction rule and reviewed experience makes a particular
pair runnable.
