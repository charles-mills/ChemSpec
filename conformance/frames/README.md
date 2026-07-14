# Frame fixtures

Slice 6 adds renderer-independent structural and observation frame oracles to
this component. Chemistry expectations are checked against the independently
authored Slice 5 state and operation oracle; they are not generated from the
frame projection under test. Exact trace and state digests are fixed integrity
regressions over those independently checked values, not a substitute source
of chemistry authority. Production promotion uses the separate exact,
host-pinned AI review attestation.
