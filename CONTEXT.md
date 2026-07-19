# ChemSpec Domain Language

ChemSpec turns reaction requests into validated structural meaning and then
into an explanatory experience. These terms distinguish knowledge provenance
from runtime authority.

## Language

**Reference catalogue**:
Bundled local chemistry records used for identity reuse, factual provenance,
and fast-path derivation. Presence or absence never grants or denies permission
to validate, animate, or display chemistry.
_Avoid_: Trusted catalogue, approved catalogue, allow-list

**Reviewed**:
A factual-provenance label stating that a reference record received the
declared review. It is not a runtime capability or validation result.
_Avoid_: Trusted, approved, authorized

**Provisional**:
Structurally represented chemistry whose factual content is not a reviewed
reference record. It may become renderer-readable after identical deterministic
validation while retaining provisional provenance.
_Avoid_: Untrusted chemistry, review candidate

**Validated**:
A private capability produced by deterministic balance, identity, structure,
mapping, electron, and staleness checks. Only validated meaning may reach the
simulation.
_Avoid_: Trusted

**Unsupported**:
A typed outcome stating that the current deterministic and bounded fallback
paths cannot produce validated meaning. It describes capability, not catalogue
membership.
_Avoid_: Not approved, not allow-listed
