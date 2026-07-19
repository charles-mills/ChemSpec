//! Ordered validation pipeline for an external catalogue envelope.
//!
//! Stage ownership lives here so the public bundle API does not also become
//! the accumulator for every intermediate index. The ordering is deliberate:
//! later stages only receive indexes produced by earlier validated stages.

use super::{
    CatalogueDocument, CatalogueEnvelope, CatalogueError, CatalogueErrorCode, ContentDigest,
    G1ValidationContext, PublicationKind, ValidatedCatalogueBundle,
    ensure_rule_namespaces_disjoint, generalized, index_evidence, index_premises,
    index_structural_traits, normalize_document, pattern, validate_concrete_structure_traits,
    validate_elements_and_categories, validate_macroscopic_materials, validate_metadata,
    validate_production_reviews, validate_rules, validate_structure_templates_and_applications,
    validate_structures, validate_valence_premises,
};

struct ValidationPipeline {
    digest: ContentDigest,
    document: CatalogueDocument,
}

impl ValidationPipeline {
    fn prepare(envelope: CatalogueEnvelope) -> Result<Self, CatalogueError> {
        validate_metadata(&envelope.bundle)?;
        let computed = envelope.computed_digest()?;
        if computed != envelope.digest {
            return Err(CatalogueError::new(
                CatalogueErrorCode::DigestMismatch,
                format!("declared {} but computed {computed}", envelope.digest),
            ));
        }

        let mut document = envelope.bundle;
        normalize_document(&mut document);
        Ok(Self {
            digest: envelope.digest,
            document,
        })
    }

    fn run(self) -> Result<ValidatedCatalogueBundle, CatalogueError> {
        let Self { digest, document } = self;
        let evidence = index_evidence(&document.evidence)?;
        let premises = index_premises(&document.premises, &evidence)?;
        validate_publication(&document)?;
        let (elements, element_categories, element_category_members, element_membership_provenance) =
            validate_elements_and_categories(
                &document.elements,
                &document.element_categories,
                &premises,
            )?;
        let structural_traits = index_structural_traits(&document.structural_traits, &premises)?;
        let valence_premises = validate_valence_premises(&document.valence_premises, &premises)?;
        let (mut structures, mut structure_premises) =
            validate_structures(&document.structures, &premises, &document.valence_premises)?;
        let mut structure_traits = validate_concrete_structure_traits(
            &document.structures,
            &structures,
            &document.structural_traits,
            &structural_traits,
            &premises,
        )?;
        let g1 = validate_structure_templates_and_applications(G1ValidationContext {
            templates: &document.structure_templates,
            applications: &document.structure_applications,
            elements: &document.elements,
            element_index: &elements,
            category_members: &element_category_members,
            membership_provenance: &element_membership_provenance,
            trait_definitions: &document.structural_traits,
            trait_index: &structural_traits,
            premises: &premises,
            valence: &document.valence_premises,
            structures: &mut structures,
            structure_premises: &mut structure_premises,
            structure_traits: &mut structure_traits,
        })?;
        let graph_patterns = pattern::validate_graph_patterns(
            &document.graph_patterns,
            &elements,
            &document.structural_traits,
            &structural_traits,
            &premises,
        )?;
        let generalized_rules = generalized::validate_generalized_rules(
            &document.generalized_rules,
            &element_category_members,
            &element_membership_provenance,
            &structures,
            &structure_premises,
            &structure_traits,
            &document.structural_traits,
            &structural_traits,
            &document.structure_templates,
            &g1.templates,
            &document.structure_applications,
            &document.graph_patterns,
            &graph_patterns,
            &premises,
        )?;
        let rules = validate_rules(
            &document.rules,
            &structures,
            &structure_premises,
            &valence_premises,
            &document.valence_premises,
            &premises,
        )?;
        ensure_rule_namespaces_disjoint(&rules, &generalized_rules)?;
        let macroscopic_materials = validate_macroscopic_materials(
            &document.macroscopic_materials,
            &structures,
            &rules,
            &generalized_rules,
            &document.generalized_rules,
            &premises,
        )?;

        Ok(ValidatedCatalogueBundle {
            digest,
            document,
            structures,
            structure_premises,
            premises,
            evidence,
            valence_premises,
            rules,
            elements,
            element_categories,
            element_category_members,
            element_membership_provenance,
            structural_traits,
            structure_templates: g1.templates,
            structure_applications: g1.applications,
            structure_aliases: g1.aliases,
            structure_traits,
            structure_application_provenance: g1.provenance,
            graph_patterns,
            generalized_rules,
            macroscopic_materials,
        })
    }
}

fn validate_publication(document: &CatalogueDocument) -> Result<(), CatalogueError> {
    if matches!(document.publication, PublicationKind::Production) {
        validate_production_reviews(&document.premises)?;
    }
    Ok(())
}

pub(super) fn validate(
    envelope: CatalogueEnvelope,
) -> Result<ValidatedCatalogueBundle, CatalogueError> {
    ValidationPipeline::prepare(envelope)?.run()
}
