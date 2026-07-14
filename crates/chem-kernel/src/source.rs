use chem_domain::{SourceDecimal, UnitExpression, UnitPower, UnitProduct};
use chems_lang::{
    ChemicalSyntaxKind, NameSyntaxKind, QuantitySyntaxKind, SourceNode, SourceNodeKind,
};

pub(crate) fn descendants(
    node: &SourceNode,
    predicate: impl Copy + Fn(&SourceNodeKind) -> bool,
) -> Vec<&SourceNode> {
    let mut output = Vec::new();
    collect_descendants(node, predicate, &mut output);
    output
}

fn collect_descendants<'a>(
    node: &'a SourceNode,
    predicate: impl Copy + Fn(&SourceNodeKind) -> bool,
    output: &mut Vec<&'a SourceNode>,
) {
    if predicate(&node.kind) {
        output.push(node);
    }
    for child in &node.children {
        collect_descendants(child, predicate, output);
    }
}

pub(crate) fn first_descendant(
    node: &SourceNode,
    predicate: impl Copy + Fn(&SourceNodeKind) -> bool,
) -> Option<&SourceNode> {
    if predicate(&node.kind) {
        return Some(node);
    }
    node.children
        .iter()
        .find_map(|child| first_descendant(child, predicate))
}

pub(crate) fn direct_child(
    node: &SourceNode,
    predicate: impl Copy + Fn(&SourceNodeKind) -> bool,
) -> Option<&SourceNode> {
    node.children.iter().find(|child| predicate(&child.kind))
}

pub(crate) fn value_names(node: &SourceNode) -> Vec<&SourceNode> {
    descendants(node, |kind| {
        matches!(
            kind,
            SourceNodeKind::Name {
                form: NameSyntaxKind::ValueIdentifier
            }
        )
    })
}

pub(crate) fn qualified_names(node: &SourceNode) -> Vec<&SourceNode> {
    descendants(node, |kind| {
        matches!(
            kind,
            SourceNodeKind::Name {
                form: NameSyntaxKind::QualifiedName
            }
        )
    })
}

pub(crate) fn stage_references(node: &SourceNode) -> Vec<&SourceNode> {
    descendants(node, |kind| {
        matches!(
            kind,
            SourceNodeKind::Name {
                form: NameSyntaxKind::StageReference
            }
        )
    })
}

pub(crate) fn quantities(node: &SourceNode) -> Vec<&SourceNode> {
    descendants(node, |kind| {
        matches!(
            kind,
            SourceNodeKind::Quantity {
                form: QuantitySyntaxKind::Quantity
            }
        )
    })
}

pub(crate) fn species_nodes(node: &SourceNode) -> Vec<&SourceNode> {
    descendants(node, |kind| {
        matches!(
            kind,
            SourceNodeKind::Chemical {
                form: ChemicalSyntaxKind::Species
            }
        )
    })
}

pub(crate) fn parse_quantity_parts(
    node: &SourceNode,
) -> Result<(SourceDecimal, UnitExpression), String> {
    let decimal = first_descendant(node, |kind| {
        matches!(
            kind,
            SourceNodeKind::Quantity {
                form: QuantitySyntaxKind::Decimal
            }
        )
    })
    .and_then(|node| node.lexeme.as_deref())
    .ok_or_else(|| "quantity has no decimal".to_owned())?;
    let unit = first_descendant(node, |kind| {
        matches!(
            kind,
            SourceNodeKind::Quantity {
                form: QuantitySyntaxKind::UnitExpression
            }
        )
    })
    .and_then(|node| node.lexeme.as_deref())
    .ok_or_else(|| "quantity has no unit expression".to_owned())?;
    Ok((
        SourceDecimal::parse(decimal).map_err(|error| error.to_string())?,
        parse_unit_expression(unit)?,
    ))
}

fn parse_unit_expression(source: &str) -> Result<UnitExpression, String> {
    let mut products = source.split('/').map(parse_unit_product);
    let dividend = products
        .next()
        .ok_or_else(|| "unit expression is empty".to_owned())??;
    let divisors = products.collect::<Result<Vec<_>, _>>()?;
    Ok(UnitExpression::quotient(dividend, divisors))
}

fn parse_unit_product(source: &str) -> Result<UnitProduct, String> {
    let factors = source
        .split('*')
        .map(|factor| {
            let (symbol, authored_exponent) = factor
                .split_once('^')
                .map_or((factor, None), |(symbol, exponent)| {
                    (symbol, Some(exponent))
                });
            UnitPower::parse_authored(symbol, authored_exponent).map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(UnitProduct::new(factors))
}
