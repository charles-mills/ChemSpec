# `chem-kernel`

`chem-kernel` owns trusted typed elaboration, exact experiment state, goals,
rules, derivations, and validated artifacts.

Slice 4 implements typed elaboration and initial materials. The public
`elaborate` entry point accepts source plus one validated catalogue and returns
either complete `TypedExperiment` HIR or source/semantic diagnostics.

The implemented boundary includes:

- one shared experiment namespace with stable typed IDs;
- exact environment, quantity, unit, formula, species, medium, and operand
  resolution;
- every initial material constructor and prepared-component normalization;
- catalogue-fact and explicit-assumption premise tracing;
- warnings that do not suppress otherwise complete HIR;
- source origins for conditions, declarations, operations, and the experiment;
- a checked-in canonical HIR oracle and fixture-driven negative coverage.

It does not execute procedures, infer reactions, elaborate claims, generate
goals, or construct validated artifacts. Those boundaries begin in Slice 5 and
later slices.
