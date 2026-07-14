use std::{fs, path::PathBuf, str::FromStr};

use chem_domain::{
    AtomId, CanonicalJsonError, Charge, ChargeSign, ContentDigest, Count, Dimension, Element,
    ElementId, ElementSymbol, ExperimentId, FactId, FormulaError, FormulaPart, FormulaSegment,
    FormulaSyntax, Phase, Quantity, ResolvedUnit, SourceDecimal, StaticElementRegistry,
    SubstanceId, TemperaturePoint, TemperaturePointError, TemperatureScale, UnitError,
    UnitExpression, UnitPower, UnitProduct, UnitSymbol, canonical_json, resolve_unit_expression,
};
use num_bigint::{BigInt, BigUint};
use serde::Deserialize;
use serde_json::{Value, json};

fn decimal(source: &str) -> SourceDecimal {
    SourceDecimal::parse(source).expect("test decimal must be valid")
}

fn count(value: u32) -> Count {
    Count::new(BigUint::from(value)).expect("test count must be positive")
}

fn element(symbol: &str, atomic_number: u16) -> Element {
    Element {
        id: ElementId::new(atomic_number).expect("test atomic number must be nonzero"),
        symbol: ElementSymbol::new(symbol).expect("test symbol must be valid"),
    }
}

fn registry() -> StaticElementRegistry {
    StaticElementRegistry::new([
        element("H", 1),
        element("O", 8),
        element("S", 16),
        element("Ca", 20),
        element("Cu", 29),
    ])
    .expect("test registry must be valid")
}

fn atom(symbol: &str, amount: u32) -> FormulaPart {
    FormulaPart::Element {
        symbol: ElementSymbol::new(symbol).expect("test symbol must be valid"),
        count: count(amount),
    }
}

fn fixture(path: &str) -> Value {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    serde_json::from_slice(
        &fs::read(root.join(path)).unwrap_or_else(|error| panic!("could not read {path}: {error}")),
    )
    .unwrap_or_else(|error| panic!("could not parse {path}: {error}"))
}

fn assert_canonical_golden(actual: &Value, path: &str) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let expected_bytes =
        fs::read(root.join(path)).unwrap_or_else(|error| panic!("could not read {path}: {error}"));
    let expected: Value = serde_json::from_slice(&expected_bytes)
        .unwrap_or_else(|error| panic!("could not parse {path}: {error}"));
    assert_eq!(
        canonical_json(&expected).unwrap(),
        expected_bytes,
        "{path} is not canonical JSON bytes"
    );
    assert_eq!(
        canonical_json(actual).unwrap(),
        expected_bytes,
        "domain output did not match {path}"
    );
}

#[test]
fn source_decimals_preserve_precision_and_use_exact_rationals() {
    let cases = [
        ("50", "50", 0, 2, Some(2), "50"),
        ("50.0", "500", 1, 3, Some(3), "50"),
        ("0.100", "100", 3, 4, Some(3), "1/10"),
        ("0.01020", "1020", 5, 6, Some(4), "51/5000"),
        ("0.000", "0", 3, 4, None, "0"),
        ("+5.00", "500", 2, 3, Some(3), "5"),
        ("-273.15", "-27315", 2, 5, Some(5), "-5463/20"),
    ];

    for (source, coefficient, scale, written_digits, significant_digits, exact) in cases {
        let parsed = decimal(source);
        assert_eq!(parsed.coefficient().to_string(), coefficient);
        assert_eq!(parsed.scale(), scale);
        assert_eq!(parsed.precision().decimal_places, scale);
        assert_eq!(parsed.precision().written_digits, written_digits);
        assert_eq!(parsed.precision().significant_digits, significant_digits);
        assert_eq!(parsed.exact_value().to_string(), exact);
    }
    assert_eq!(decimal("+5.00").canonical_lexeme(), "5.00");
}

#[test]
fn every_closed_registry_unit_resolves_to_its_exact_definition() {
    let multiplicative = [
        ("kg", Dimension::MASS, "1"),
        ("g", Dimension::MASS, "1/1000"),
        ("mg", Dimension::MASS, "1/1000000"),
        ("m", Dimension::LENGTH, "1"),
        ("cm", Dimension::LENGTH, "1/100"),
        ("mm", Dimension::LENGTH, "1/1000"),
        ("L", Dimension::VOLUME, "1/1000"),
        ("mL", Dimension::VOLUME, "1/1000000"),
        ("uL", Dimension::VOLUME, "1/1000000000"),
        ("mol", Dimension::AMOUNT, "1"),
        ("mmol", Dimension::AMOUNT, "1/1000"),
        ("umol", Dimension::AMOUNT, "1/1000000"),
        ("s", Dimension::TIME, "1"),
        ("min", Dimension::TIME, "60"),
        ("h", Dimension::TIME, "3600"),
        ("Pa", Dimension::PRESSURE, "1"),
        ("kPa", Dimension::PRESSURE, "1000"),
        ("atm", Dimension::PRESSURE, "101325"),
        ("M", Dimension::CONCENTRATION, "1000"),
        ("mM", Dimension::CONCENTRATION, "1"),
        ("%", Dimension::DIMENSIONLESS, "1/100"),
    ];

    for (source, expected_dimension, expected_factor) in multiplicative {
        let symbol = UnitSymbol::from_str(source).expect("registered unit must parse");
        let resolved = resolve_unit_expression(&UnitExpression::single(symbol))
            .expect("registered unit must resolve");
        let ResolvedUnit::Multiplicative { dimension, factor } = resolved else {
            panic!("{source} must be multiplicative")
        };
        assert_eq!(
            dimension, expected_dimension,
            "wrong dimension for {source}"
        );
        assert_eq!(
            factor.to_string(),
            expected_factor,
            "wrong factor for {source}"
        );
    }

    assert_eq!(UnitSymbol::ALL.len(), 23);
    assert!(matches!(
        resolve_unit_expression(&UnitExpression::single(UnitSymbol::Kelvin)),
        Ok(ResolvedUnit::TemperaturePoint {
            scale: TemperatureScale::Kelvin
        })
    ));
    assert!(matches!(
        resolve_unit_expression(&UnitExpression::single(UnitSymbol::DegreesCelsius)),
        Ok(ResolvedUnit::TemperaturePoint {
            scale: TemperatureScale::DegreesCelsius
        })
    ));
}

#[test]
fn equivalent_unit_expressions_and_conversion_round_trips_are_exact() {
    let mol_per_litre = UnitExpression::quotient(
        UnitProduct::new(vec![UnitPower::parse("mol", 1).unwrap()]),
        vec![UnitProduct::new(vec![UnitPower::parse("L", 1).unwrap()])],
    );
    let mol_times_litre_inverse = UnitExpression::new(vec![
        UnitPower::parse("mol", 1).unwrap(),
        UnitPower::parse("L", -1).unwrap(),
    ]);
    let molar = UnitExpression::single(UnitSymbol::Molar);
    assert_eq!(
        resolve_unit_expression(&mol_per_litre).unwrap(),
        resolve_unit_expression(&molar).unwrap()
    );
    assert_eq!(
        resolve_unit_expression(&mol_times_litre_inverse).unwrap(),
        resolve_unit_expression(&molar).unwrap()
    );
    assert_ne!(mol_per_litre, mol_times_litre_inverse);
    let quotient_quantity = Quantity::new(decimal("2"), mol_per_litre).unwrap();
    let inverse_quantity = Quantity::new(decimal("2"), mol_times_litre_inverse).unwrap();
    assert_eq!(quotient_quantity, inverse_quantity);
    assert!(!quotient_quantity.source_eq(&inverse_quantity));

    let implicit_exponent = UnitExpression::single(UnitSymbol::Mole);
    let explicit_exponent =
        UnitExpression::new(vec![UnitPower::parse_authored("mol", Some("+1")).unwrap()]);
    assert_eq!(
        resolve_unit_expression(&implicit_exponent).unwrap(),
        resolve_unit_expression(&explicit_exponent).unwrap()
    );
    let implicit_quantity = Quantity::new(decimal("1"), implicit_exponent).unwrap();
    let explicit_quantity = Quantity::new(decimal("1"), explicit_exponent).unwrap();
    assert_eq!(implicit_quantity, explicit_quantity);
    assert!(!implicit_quantity.source_eq(&explicit_quantity));

    let mass_units = [
        UnitExpression::single(UnitSymbol::Kilogram),
        UnitExpression::single(UnitSymbol::Gram),
        UnitExpression::single(UnitSymbol::Milligram),
    ];
    for value in -100..=100 {
        for source in &mass_units {
            let quantity = Quantity::new(decimal(&value.to_string()), source.clone()).unwrap();
            for target in &mass_units {
                let converted = quantity.value_in(target).unwrap();
                let target_factor = match resolve_unit_expression(target).unwrap() {
                    ResolvedUnit::Multiplicative { factor, .. } => factor,
                    ResolvedUnit::TemperaturePoint { .. } => unreachable!(),
                };
                assert_eq!(&converted * &target_factor, *quantity.canonical_value());
            }
        }
    }
}

#[test]
fn unit_and_quantity_failures_are_typed() {
    assert_eq!(
        UnitPower::parse("furlong", 1),
        Err(UnitError::UnknownSymbol("furlong".to_owned()))
    );
    assert_eq!(
        UnitPower::parse_authored("m", Some("not-an-integer")),
        Err(UnitError::InvalidExponent("not-an-integer".to_owned()))
    );
    let invalid_percent = UnitExpression::new(vec![
        UnitPower::parse("%", 1).unwrap(),
        UnitPower::parse("s", -1).unwrap(),
    ]);
    assert_eq!(
        resolve_unit_expression(&invalid_percent),
        Err(UnitError::RestrictedUnitMustBeStandalone(
            UnitSymbol::Percent
        ))
    );
    let invalid_kelvin = UnitExpression::new(vec![UnitPower::parse("K", 2).unwrap()]);
    assert_eq!(
        resolve_unit_expression(&invalid_kelvin),
        Err(UnitError::RestrictedUnitMustBeStandalone(
            UnitSymbol::Kelvin
        ))
    );

    let one_kg = Quantity::new(decimal("1"), UnitExpression::single(UnitSymbol::Kilogram)).unwrap();
    let thousand_g =
        Quantity::new(decimal("1000"), UnitExpression::single(UnitSymbol::Gram)).unwrap();
    assert_eq!(one_kg, thousand_g);
    assert!(!one_kg.source_eq(&thousand_g));
}

#[test]
fn temperature_points_are_affine_and_differences_are_multiplicative() {
    let freezing_c = TemperaturePoint::new(decimal("0.00"), TemperatureScale::DegreesCelsius)
        .expect("freezing point must be valid");
    let freezing_k = TemperaturePoint::new(decimal("273.15"), TemperatureScale::Kelvin)
        .expect("freezing point must be valid");
    assert_eq!(freezing_c, freezing_k);
    assert!(!freezing_c.source_eq(&freezing_k));
    assert_eq!(freezing_c.kelvin().to_string(), "5463/20");

    let negative_c = TemperaturePoint::new(decimal("-10"), TemperatureScale::DegreesCelsius)
        .expect("negative Celsius above absolute zero must be valid");
    assert_eq!(negative_c.kelvin().to_string(), "5263/20");
    assert_eq!(
        TemperaturePoint::new(decimal("-273.16"), TemperatureScale::DegreesCelsius),
        Err(TemperaturePointError::BelowAbsoluteZero)
    );

    let boiling = TemperaturePoint::new(decimal("100"), TemperatureScale::DegreesCelsius).unwrap();
    let interval = boiling.difference_from(&freezing_c);
    assert_eq!(interval.kelvin().to_string(), "100");
    assert_eq!(
        interval
            .value_in(TemperatureScale::DegreesCelsius)
            .to_string(),
        "100"
    );
}

#[test]
fn grouped_and_adduct_formulae_normalize_by_element_identity() {
    let calcium_hydroxide = FormulaSyntax {
        segments: vec![FormulaSegment {
            coefficient: count(1),
            parts: vec![
                atom("Ca", 1),
                FormulaPart::Group {
                    parts: vec![atom("O", 1), atom("H", 1)],
                    count: count(2),
                },
            ],
        }],
    };
    let expanded = FormulaSyntax {
        segments: vec![FormulaSegment {
            coefficient: count(1),
            parts: vec![atom("Ca", 1), atom("O", 2), atom("H", 2)],
        }],
    };
    let normalized = calcium_hydroxide.normalize(&registry()).unwrap();
    let expanded = expanded.normalize(&registry()).unwrap();
    assert_eq!(normalized, expanded);
    assert!(!normalized.source_eq(&expanded));

    let copper_sulfate_pentahydrate = FormulaSyntax {
        segments: vec![
            FormulaSegment {
                coefficient: count(1),
                parts: vec![atom("Cu", 1), atom("S", 1), atom("O", 4)],
            },
            FormulaSegment {
                coefficient: count(5),
                parts: vec![atom("H", 2), atom("O", 1)],
            },
        ],
    }
    .normalize(&registry())
    .unwrap();
    let composition = copper_sulfate_pentahydrate.composition();
    assert_eq!(
        composition[&ElementId::new(1).unwrap()],
        BigUint::from(10_u8)
    );
    assert_eq!(
        composition[&ElementId::new(8).unwrap()],
        BigUint::from(9_u8)
    );
    assert_eq!(
        composition[&ElementId::new(16).unwrap()],
        BigUint::from(1_u8)
    );
    assert_eq!(
        composition[&ElementId::new(29).unwrap()],
        BigUint::from(1_u8)
    );
}

#[test]
fn formula_normalization_property_matches_expansion() {
    for group_count in 1..=12 {
        for coefficient in 1..=12 {
            let grouped = FormulaSyntax {
                segments: vec![FormulaSegment {
                    coefficient: count(coefficient),
                    parts: vec![FormulaPart::Group {
                        parts: vec![atom("H", 2), atom("O", 1)],
                        count: count(group_count),
                    }],
                }],
            };
            let expanded = FormulaSyntax {
                segments: vec![FormulaSegment {
                    coefficient: count(1),
                    parts: vec![
                        atom("H", 2 * group_count * coefficient),
                        atom("O", group_count * coefficient),
                    ],
                }],
            };
            assert_eq!(
                grouped.normalize(&registry()).unwrap(),
                expanded.normalize(&registry()).unwrap()
            );
        }
    }
}

#[test]
fn formula_errors_charge_and_phase_are_exact_and_closed() {
    let unknown = FormulaSyntax {
        segments: vec![FormulaSegment {
            coefficient: count(1),
            parts: vec![atom("Xe", 1)],
        }],
    };
    assert_eq!(
        unknown.normalize(&registry()),
        Err(FormulaError::UnknownElement(
            ElementSymbol::new("Xe").unwrap()
        ))
    );
    assert_eq!(Charge::neutral().value(), &BigInt::from(0));
    assert_eq!(
        Charge::from_magnitude(BigUint::from(3_u8), ChargeSign::Negative)
            .unwrap()
            .value(),
        &BigInt::from(-3)
    );
    assert_ne!(Phase::Aqueous, Phase::Liquid);
    assert_eq!(
        serde_json::to_value(Phase::Gas).unwrap(),
        json!({ "kind": "gas" })
    );
    assert_eq!(
        serde_json::to_value(ChargeSign::Positive).unwrap(),
        json!({ "kind": "positive" })
    );
    assert_eq!(
        serde_json::to_value(TemperatureScale::Kelvin).unwrap(),
        json!({ "kind": "kelvin" })
    );
}

#[test]
fn canonical_serialization_and_typed_ids_are_stable() {
    let value = json!({"z": [3, 2, 1], "a": {"b": true, "a": null}});
    let first = canonical_json(&value).unwrap();
    let second = canonical_json(&value).unwrap();
    assert_eq!(first, second);
    assert_eq!(first, br#"{"a":{"a":null,"b":true},"z":[3,2,1]}"#);
    assert_eq!(
        canonical_json(&json!({"amount": 0.1})),
        Err(CanonicalJsonError::FloatingPointNumber)
    );

    let digest = ContentDigest::of_json(&json!({"b": 2, "a": 1})).unwrap();
    let experiment = ExperimentId::from_digest(digest);
    assert_eq!(experiment.to_string(), digest.to_string());
    assert_eq!(experiment.digest(), digest);
    assert!(FactId::new("nist.water-density").is_ok());
    assert!(AtomId::new("water[1].o").is_ok());
    assert!(FactId::new("water[1].o").is_err());
    for invalid in [
        "water[]",
        "water[0]",
        "water[01]",
        "water[1",
        "water[1]o",
        "[1]",
        "water.[1]",
    ] {
        assert!(AtomId::new(invalid).is_err(), "accepted `{invalid}`");
    }
    assert!(SubstanceId::new("pubchem:962").is_ok());
    assert!(FactId::new("contains spaces").is_err());
}

#[test]
fn deserialization_cannot_forge_validated_domain_invariants() {
    assert!(serde_json::from_value::<ElementSymbol>(json!("h")).is_err());
    assert!(serde_json::from_value::<ElementId>(json!(0)).is_err());

    let source = decimal("1.00");
    let encoded_source = serde_json::to_value(&source).unwrap();
    assert_eq!(
        serde_json::from_value::<SourceDecimal>(encoded_source.clone()).unwrap(),
        source
    );
    let mut forged_source = encoded_source;
    forged_source["coefficient"] = json!("999");
    assert!(serde_json::from_value::<SourceDecimal>(forged_source).is_err());
    let exact_zero = serde_json::to_value(decimal("0.00")).unwrap();
    assert!(exact_zero["precision"].get("significant_digits").is_none());

    let quantity =
        Quantity::new(decimal("1000"), UnitExpression::single(UnitSymbol::Gram)).unwrap();
    let encoded_quantity = serde_json::to_value(&quantity).unwrap();
    assert!(encoded_quantity["source_unit"].get("source").is_none());
    let decoded_quantity = serde_json::from_value::<Quantity>(encoded_quantity.clone()).unwrap();
    assert!(quantity.source_eq(&decoded_quantity));
    let mut forged_quantity = encoded_quantity.clone();
    forged_quantity["dimension"]["mass"] = json!(7);
    assert!(serde_json::from_value::<Quantity>(forged_quantity).is_err());
    let mut forged_unit = encoded_quantity;
    forged_unit["source_unit"]["dividend"]["factors"][0]["symbol"] = json!("kg");
    assert!(serde_json::from_value::<Quantity>(forged_unit).is_err());

    let mut forged_derivation = serde_json::to_value(&quantity).unwrap();
    forged_derivation["conversion_derivation"]["steps"][0]["unit_factor"]["denominator"] =
        json!("1");
    assert!(serde_json::from_value::<Quantity>(forged_derivation).is_err());

    let temperature =
        TemperaturePoint::new(decimal("0"), TemperatureScale::DegreesCelsius).unwrap();
    let mut forged_temperature = serde_json::to_value(&temperature).unwrap();
    forged_temperature["kelvin"]["numerator"] = json!("0");
    assert!(serde_json::from_value::<TemperaturePoint>(forged_temperature).is_err());
}

#[test]
fn numeric_precision_conformance_golden_matches() {
    #[derive(Deserialize)]
    struct Input {
        values: Vec<String>,
    }

    let input: Input = serde_json::from_value(fixture(
        "conformance/quantities-types/numeric-precision-001.input.json",
    ))
    .unwrap();
    let values = input
        .values
        .iter()
        .map(|source| {
            let parsed = decimal(source);
            let mut value = json!({
                "source": parsed.lexeme(),
                "canonical": parsed.canonical_lexeme(),
                "coefficient": parsed.coefficient().to_string(),
                "scale": parsed.scale(),
                "decimal_places": parsed.precision().decimal_places,
                "written_digits": parsed.precision().written_digits,
                "exact": parsed.exact_value(),
            });
            if let Some(significant_digits) = parsed.precision().significant_digits {
                value
                    .as_object_mut()
                    .unwrap()
                    .insert("significant_digits".to_owned(), json!(significant_digits));
            }
            value
        })
        .collect::<Vec<_>>();
    assert_canonical_golden(
        &json!({ "values": values }),
        "conformance/quantities-types/numeric-precision-001.domain.json",
    );
}

#[test]
fn unit_and_temperature_conformance_golden_matches() {
    #[derive(Deserialize)]
    struct TemperatureInput {
        source: String,
        scale: TemperatureScale,
    }

    #[derive(Deserialize)]
    struct Input {
        units: Vec<String>,
        temperatures: Vec<TemperatureInput>,
    }

    let input: Input = serde_json::from_value(fixture(
        "conformance/quantities-types/unit-temperature-001.input.json",
    ))
    .unwrap();
    let units = input
        .units
        .iter()
        .map(|source| {
            let symbol = UnitSymbol::from_str(source).unwrap();
            let mut value = serde_json::to_value(
                resolve_unit_expression(&UnitExpression::single(symbol)).unwrap(),
            )
            .unwrap();
            value
                .as_object_mut()
                .expect("resolved unit serializes as an object")
                .insert("symbol".to_owned(), json!(source));
            value
        })
        .collect::<Vec<_>>();
    let equivalent_concentration = resolve_unit_expression(&UnitExpression::quotient(
        UnitProduct::new(vec![UnitPower::parse("mol", 1).unwrap()]),
        vec![UnitProduct::new(vec![UnitPower::parse("L", 1).unwrap()])],
    ))
    .unwrap();
    let ResolvedUnit::Multiplicative { dimension, factor } = equivalent_concentration else {
        unreachable!()
    };
    let temperatures = input
        .temperatures
        .iter()
        .map(|input| {
            let point = TemperaturePoint::new(decimal(&input.source), input.scale).unwrap();
            json!({ "source": input.source, "scale": input.scale, "kelvin": point.kelvin() })
        })
        .collect::<Vec<_>>();
    let invalid_percent = UnitExpression::new(vec![
        UnitPower::parse("%", 1).unwrap(),
        UnitPower::parse("s", -1).unwrap(),
    ]);
    let invalid_kelvin = UnitExpression::new(vec![UnitPower::parse("K", 2).unwrap()]);
    let rejections = vec![
        json!({
            "input": "furlong",
            "error": UnitPower::parse("furlong", 1).unwrap_err().to_string(),
        }),
        json!({
            "input": "%/s",
            "error": resolve_unit_expression(&invalid_percent).unwrap_err().to_string(),
        }),
        json!({
            "input": "K^2",
            "error": resolve_unit_expression(&invalid_kelvin).unwrap_err().to_string(),
        }),
        json!({
            "input": "-273.16 degC",
            "error": TemperaturePoint::new(
                decimal("-273.16"),
                TemperatureScale::DegreesCelsius,
            )
            .unwrap_err()
            .to_string(),
        }),
    ];
    let actual = json!({
        "units": units,
        "equivalent_concentration": { "dimension": dimension, "factor": factor },
        "temperatures": temperatures,
        "rejections": rejections,
    });
    assert_canonical_golden(
        &actual,
        "conformance/quantities-types/unit-temperature-001.domain.json",
    );
}

#[test]
fn quantity_conversion_conformance_golden_matches() {
    #[derive(Deserialize)]
    struct CaseInput {
        name: String,
        source_decimal: String,
        source_unit: UnitExpression,
        target_unit: UnitExpression,
    }

    #[derive(Deserialize)]
    struct Input {
        cases: Vec<CaseInput>,
    }

    let input: Input = serde_json::from_value(fixture(
        "conformance/quantities-types/quantity-conversion-001.input.json",
    ))
    .unwrap();
    let cases = input
        .cases
        .iter()
        .map(|input| {
            let quantity =
                Quantity::new(decimal(&input.source_decimal), input.source_unit.clone()).unwrap();
            let conversion = quantity.convert_to(&input.target_unit).unwrap();
            json!({
                "name": input.name,
                "canonical_value": quantity.canonical_value(),
                "dimension": quantity.dimension(),
                "conversion": conversion,
            })
        })
        .collect::<Vec<_>>();
    assert_canonical_golden(
        &json!({ "cases": cases }),
        "conformance/quantities-types/quantity-conversion-001.domain.json",
    );
}

#[test]
fn formula_conformance_golden_matches() {
    #[derive(Deserialize)]
    struct FormulaInput {
        name: String,
        #[serde(flatten)]
        syntax: FormulaSyntax,
    }

    #[derive(Deserialize)]
    struct Input {
        elements: Vec<Element>,
        formulae: Vec<FormulaInput>,
    }

    let input: Input = serde_json::from_value(fixture(
        "conformance/formula-species/formula-normalization-001.input.json",
    ))
    .unwrap();
    let registry = StaticElementRegistry::new(input.elements).unwrap();
    let formulae = input
        .formulae
        .iter()
        .map(|input| {
            let normalized = input.syntax.normalize(&registry).unwrap();
            let composition = normalized
                .composition()
                .iter()
                .map(|(element, count)| {
                    json!({ "element": element.atomic_number(), "count": count.to_string() })
                })
                .collect::<Vec<_>>();
            json!({ "name": input.name, "composition": composition })
        })
        .collect::<Vec<_>>();
    assert_canonical_golden(
        &json!({ "formulae": formulae }),
        "conformance/formula-species/formula-normalization-001.domain.json",
    );
}

#[test]
fn charge_phase_conformance_golden_matches() {
    #[derive(Deserialize)]
    struct ChargeInput {
        magnitude: String,
        sign: Option<ChargeSign>,
    }

    #[derive(Deserialize)]
    struct Input {
        charges: Vec<ChargeInput>,
        phases: Vec<Phase>,
    }

    let input: Input = serde_json::from_value(fixture(
        "conformance/formula-species/charge-phase-001.input.json",
    ))
    .unwrap();
    let charges = input
        .charges
        .iter()
        .map(|input| match input.sign {
            Some(sign) => {
                Charge::from_magnitude(BigUint::from_str(&input.magnitude).unwrap(), sign).unwrap()
            }
            None => Charge::neutral(),
        })
        .collect::<Vec<_>>();
    let all_distinct = input
        .phases
        .iter()
        .enumerate()
        .all(|(index, phase)| !input.phases[index + 1..].contains(phase));
    assert_canonical_golden(
        &json!({ "charges": charges, "phases": input.phases, "all_distinct": all_distinct }),
        "conformance/formula-species/charge-phase-001.domain.json",
    );
}

#[test]
fn canonical_identity_conformance_golden_matches() {
    #[derive(Deserialize)]
    struct Input {
        value: Value,
        fact_id: String,
        substance_id: String,
    }

    let input: Input = serde_json::from_value(fixture(
        "conformance/artifacts/canonical-identity-001.input.json",
    ))
    .unwrap();
    let canonical = canonical_json(&input.value).unwrap();
    let digest = ContentDigest::of_json(&input.value).unwrap();
    let experiment_id = ExperimentId::from_digest(digest);
    let actual = json!({
        "canonical": String::from_utf8(canonical).unwrap(),
        "sha256": digest,
        "experiment_id": experiment_id,
        "fact_id": FactId::new(input.fact_id).unwrap(),
        "substance_id": SubstanceId::new(input.substance_id).unwrap(),
        "floating_point_rejected": canonical_json(&json!({ "value": 0.1 })).is_err(),
    });
    assert_canonical_golden(
        &actual,
        "conformance/artifacts/canonical-identity-001.domain.json",
    );
}
