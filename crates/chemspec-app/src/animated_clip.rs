//! Colour roles for the procedural reaction assemblies.
//!
//! Historically the index into baked Blender clip materials; the clips are
//! gone, but the role names still key every assembly's reviewed-colour
//! bindings.

// Some roles are constructed only by specific scenes or their tests; the
// enum stays complete as the colour-role vocabulary.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipColour {
    Glass,
    Water,
    WaterHighlight,
    ReactiveMetal,
    FlameOuter,
    FlameInner,
    FlameCore,
    FizzBubble,
    Vapour,
    MixtureA,
    MixtureB,
    SaltResidue,
    Fuel,
    IgnitionSpark,
    ProductPlume,
    CombustionSmoke,
    Soot,
    SootDeposit,
    LiquidInitial,
    LiquidAdded,
    PrecipitateCloud,
    Precipitate,
    GasBubble,
    GasCloud,
    SolidReactant,
    SolutionInitial,
    SolutionFinal,
    OriginalMetal,
    DepositedMetal,
    MetalErosion,
    ReactantA,
    ReactantB,
    SynthesisProduct,
    ReactionFront,
    ReactionVessel,
    MixingTool,
}
