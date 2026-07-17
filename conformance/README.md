# Conformance fixtures

Shared test fixtures for the `.chems 1` language and the validation kernel.
The former manifest-driven conformance harness (`chems-conformance`) was
deleted in the programmatic pivot; these directories remain because crate
test suites read them directly:

```text
conformance/
  reserved-words.txt   # normative reserved words (chems-lang parser)
  specification/
  encoding-layout/
  parsing/             # chems-lang
  formatting/          # chems-lang
  structural-domain/
  catalogue/           # chem-kernel / chem-catalogue
  expansion/
  validation-kernel/
  observations/
  diagnostics-tooling/
  artifacts/
  frames/
  end-to-end/
```

A fixture is added or changed alongside the test that consumes it; there is
no separate coverage registry. `cargo test --workspace` is the gate.
