use std::{
    collections::{BTreeMap, BTreeSet},
    fs, io,
    path::{Path, PathBuf},
};

use chem_catalogue::{
    CatalogueDocument, CatalogueEnvelope, CreationMetadata, ElementCategoryRecord, ElementRecord,
    EvidenceSource, GeneralizedReactionRuleRecord, GraphPatternRecord, PremiseRecord,
    PublicationKind, ReactionRuleRecord, ReviewStatus, StructuralTraitDefinitionRecord,
    StructureRecord, StructureTemplateApplicationRecord, StructureTemplateRecord,
    ValencePremiseRecord, ValidatedCatalogueBundle,
};
use chem_domain::ContentDigest;
use chem_kernel::{
    CurrentArtifactIdentity, DerivationProvenance, expand_provisional, generate_frames,
    validate_provisional,
};
use chems_lang::format_source;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

const PACKAGE_FILES: [&str; 3] = ["candidate.json", "evidence.json", "example.chems"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CandidateShard {
    schema_version: u32,
    id: String,
    #[serde(default)]
    evidence: Vec<EvidenceSource>,
    #[serde(default)]
    premises: Vec<PremiseRecord>,
    #[serde(default)]
    valence_premises: Vec<ValencePremiseRecord>,
    #[serde(default)]
    structures: Vec<StructureRecord>,
    #[serde(default)]
    rules: Vec<ReactionRuleRecord>,
    #[serde(default)]
    elements: Vec<ElementRecord>,
    #[serde(default)]
    element_categories: Vec<ElementCategoryRecord>,
    #[serde(default)]
    structural_traits: Vec<StructuralTraitDefinitionRecord>,
    #[serde(default)]
    structure_templates: Vec<StructureTemplateRecord>,
    #[serde(default)]
    structure_applications: Vec<StructureTemplateApplicationRecord>,
    #[serde(default)]
    graph_patterns: Vec<GraphPatternRecord>,
    #[serde(default)]
    generalized_rules: Vec<GeneralizedReactionRuleRecord>,
}

#[derive(Debug)]
struct LoadedPackage {
    shard: CandidateShard,
    source: String,
    evidence: Vec<u8>,
}

#[derive(Debug)]
struct InspectionOutput {
    id: String,
    certificate: Vec<u8>,
    derivation: Vec<u8>,
    frames: Vec<u8>,
    certificate_digest: ContentDigest,
    derivation_digest: ContentDigest,
    frames_digest: ContentDigest,
}

#[derive(Debug)]
struct CandidateAssessment {
    packages: Vec<LoadedPackage>,
    catalogue_bytes: Vec<u8>,
    catalogue: ValidatedCatalogueBundle,
    inspections: Vec<InspectionOutput>,
}

#[derive(Debug, Serialize)]
struct ReviewRequest<'a> {
    schema_version: u32,
    status: &'static str,
    promotable: bool,
    catalogue_digest: ContentDigest,
    evidence_sources: Vec<String>,
    premises: Vec<String>,
    inspections: BTreeMap<&'a str, InspectionDigests>,
    required_external_artifact: &'static str,
    promotion_boundary: &'static str,
}

#[derive(Debug, Serialize)]
struct InspectionDigests {
    expanded_certificate: ContentDigest,
    derivation: ContentDigest,
    frames: ContentDigest,
}

pub(crate) fn catalogue_command(arguments: &[String]) -> Result<(), String> {
    match arguments.first().map(String::as_str) {
        Some("check") => check_command(&arguments[1..]),
        Some("promote") => promote_command(&arguments[1..]),
        _ => Err("catalogue requires `check` or `promote`".to_owned()),
    }
}

fn check_command(arguments: &[String]) -> Result<(), String> {
    let (output, package_paths) = check_arguments(arguments)?;
    if output.exists() {
        return Err(format!(
            "CHEMS-A003 output directory `{}` must not already exist",
            output.display()
        ));
    }
    reject_output_inside_package(&output, &package_paths)?;
    let assessment = assess_packages(&package_paths)?;
    let review = review_request(&assessment.catalogue, &assessment.inspections);
    write_outputs(
        &output,
        &assessment.catalogue_bytes,
        assessment.catalogue.digest(),
        &assessment.inspections,
        &review,
    )?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "candidate-inspection-only",
            "promotable": false,
            "packages": assessment.packages.iter().map(|package| &package.shard.id).collect::<Vec<_>>(),
            "catalogue_digest": assessment.catalogue.digest(),
            "output": output,
        }))
        .map_err(|error| error.to_string())?
    );
    Ok(())
}

fn assess_packages(package_paths: &[PathBuf]) -> Result<CandidateAssessment, String> {
    let mut packages = package_paths
        .iter()
        .map(|path| load_package(path))
        .collect::<Result<Vec<_>, _>>()?;
    packages.sort_by(|left, right| left.shard.id.cmp(&right.shard.id));
    ensure_unique_package_ids(&packages)?;
    let envelope = merge_packages(&packages)?;
    let catalogue_bytes = pretty_json(&envelope)?;
    let catalogue = ValidatedCatalogueBundle::from_json(&catalogue_bytes)
        .map_err(|error| format!("CHEMS-A020 candidate catalogue is invalid: {error}"))?;
    let inspections = inspect_examples(&packages, &catalogue)?;
    Ok(CandidateAssessment {
        packages,
        catalogue_bytes,
        catalogue,
        inspections,
    })
}

fn promote_command(arguments: &[String]) -> Result<(), String> {
    let (output, attestation, package_paths) = promote_arguments(arguments)?;
    if output.exists() {
        return Err(format!(
            "CHEMS-A003 output directory `{}` must not already exist",
            output.display()
        ));
    }
    reject_output_inside_package(&output, &package_paths)?;
    let assessment = assess_packages(&package_paths)?;
    let review_bytes = fs::read(&attestation).map_err(|error| io_error(&attestation, &error))?;
    let review_digest = assessment
        .catalogue
        .validate_review_attestation(&review_bytes)
        .map_err(|error| format!("CHEMS-A021 review attestation is invalid: {error}"))?;
    fs::create_dir(&output).map_err(|error| io_error(&output, &error))?;
    write_file(&output.join("catalogue.json"), &assessment.catalogue_bytes)?;
    write_file(&output.join("review.json"), &review_bytes)?;
    write_file(
        &output.join("catalogue.digest"),
        format!("{}\n", assessment.catalogue.digest()).as_bytes(),
    )?;
    write_file(
        &output.join("review.digest"),
        format!("{review_digest}\n").as_bytes(),
    )?;
    let manifest = pretty_json(&json!({
        "schema_version": 1,
        "catalogue_digest": assessment.catalogue.digest(),
        "review_digest": review_digest,
        "catalogue": "catalogue.json",
        "review": "review.json",
    }))?;
    write_file(&output.join("promotion.json"), &manifest)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "promoted",
            "catalogue_digest": assessment.catalogue.digest(),
            "review_digest": review_digest,
            "packages": assessment
                .packages
                .iter()
                .map(|package| package.shard.id.clone())
                .collect::<Vec<_>>(),
            "output": output,
        }))
        .map_err(|error| error.to_string())?
    );
    Ok(())
}

fn reject_output_inside_package(output: &Path, packages: &[PathBuf]) -> Result<(), String> {
    let parent = output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let parent = fs::canonicalize(parent).map_err(|error| io_error(parent, &error))?;
    for package in packages {
        let package = fs::canonicalize(package).map_err(|error| io_error(package, &error))?;
        if parent.starts_with(&package) {
            return Err(format!(
                "CHEMS-A003 output directory cannot be inside candidate package `{}`",
                package.display()
            ));
        }
    }
    Ok(())
}

fn check_arguments(arguments: &[String]) -> Result<(PathBuf, Vec<PathBuf>), String> {
    let mut output = None;
    let mut packages = Vec::new();
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--out" => {
                index += 1;
                let path = arguments
                    .get(index)
                    .ok_or("CHEMS-A001 `--out` requires a directory")?;
                if output.replace(PathBuf::from(path)).is_some() {
                    return Err("CHEMS-A001 `--out` may be specified only once".to_owned());
                }
            }
            option if option.starts_with('-') => {
                return Err(format!("CHEMS-A001 unknown option `{option}`"));
            }
            path => packages.push(PathBuf::from(path)),
        }
        index += 1;
    }
    let output = output.ok_or("CHEMS-A001 catalogue check requires `--out <directory>`")?;
    if packages.is_empty() {
        return Err("CHEMS-A001 catalogue check requires a candidate package".to_owned());
    }
    Ok((output, packages))
}

fn promote_arguments(arguments: &[String]) -> Result<(PathBuf, PathBuf, Vec<PathBuf>), String> {
    let mut output = None;
    let mut attestation = None;
    let mut packages = Vec::new();
    let mut index = 0;
    while index < arguments.len() {
        match arguments[index].as_str() {
            "--out" => {
                index += 1;
                let path = arguments
                    .get(index)
                    .ok_or("CHEMS-A001 `--out` requires a path")?;
                if output.replace(PathBuf::from(path)).is_some() {
                    return Err("CHEMS-A001 `--out` may be specified only once".to_owned());
                }
            }
            "--attestation" => {
                index += 1;
                let path = arguments
                    .get(index)
                    .ok_or("CHEMS-A001 `--attestation` requires a path")?;
                if attestation.replace(PathBuf::from(path)).is_some() {
                    return Err("CHEMS-A001 `--attestation` may be specified only once".to_owned());
                }
            }
            option if option.starts_with('-') => {
                return Err(format!("CHEMS-A001 unknown option `{option}`"));
            }
            path => packages.push(PathBuf::from(path)),
        }
        index += 1;
    }
    let output = output.ok_or("CHEMS-A001 catalogue promote requires `--out <directory>`")?;
    let attestation =
        attestation.ok_or("CHEMS-A001 catalogue promote requires `--attestation <review.json>`")?;
    if packages.is_empty() {
        return Err("CHEMS-A001 catalogue promote requires a candidate package".to_owned());
    }
    Ok((output, attestation, packages))
}

fn load_package(root: &Path) -> Result<LoadedPackage, String> {
    let root_metadata = fs::symlink_metadata(root).map_err(|error| io_error(root, &error))?;
    if !root_metadata.is_dir() || root_metadata.file_type().is_symlink() {
        return Err(format!(
            "CHEMS-A002 candidate package `{}` must be a real directory",
            root.display()
        ));
    }
    let mut actual = BTreeSet::new();
    for entry in fs::read_dir(root).map_err(|error| io_error(root, &error))? {
        let entry = entry.map_err(|error| io_error(root, &error))?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "CHEMS-A002 candidate package filename is not UTF-8".to_owned())?;
        let metadata =
            fs::symlink_metadata(entry.path()).map_err(|error| io_error(&entry.path(), &error))?;
        if !metadata.is_file() || metadata.file_type().is_symlink() {
            return Err(format!(
                "CHEMS-A002 candidate entry `{}` must be a real file",
                entry.path().display()
            ));
        }
        actual.insert(name);
    }
    let expected = PACKAGE_FILES.map(str::to_owned).into_iter().collect();
    if actual != expected {
        return Err(format!(
            "CHEMS-A002 candidate package `{}` must contain exactly candidate.json, example.chems, and evidence.json",
            root.display()
        ));
    }
    let candidate_path = root.join("candidate.json");
    let candidate_bytes =
        fs::read(&candidate_path).map_err(|error| io_error(&candidate_path, &error))?;
    let shard: CandidateShard = serde_json::from_slice(&candidate_bytes)
        .map_err(|error| format!("CHEMS-A004 {}: {error}", candidate_path.display()))?;
    if shard.schema_version != 1 || !valid_shard_id(&shard.id) {
        return Err(format!(
            "CHEMS-A004 candidate shard `{}` has an unsupported schema or unsafe id",
            shard.id
        ));
    }
    if shard.premises.iter().any(|premise| {
        premise.review.status != ReviewStatus::Provisional || !premise.review.reviewers.is_empty()
    }) {
        return Err(format!(
            "CHEMS-A004 candidate shard `{}` may contain only provisional premises with no reviewers",
            shard.id
        ));
    }
    let source_path = root.join("example.chems");
    let evidence_path = root.join("evidence.json");
    let source =
        fs::read_to_string(&source_path).map_err(|error| io_error(&source_path, &error))?;
    let formatted = format_source(&source)
        .map_err(|error| format!("CHEMS-A010 {}: {error}", source_path.display()))?;
    if formatted != source {
        return Err(format!(
            "CHEMS-A010 {} is not canonically formatted",
            source_path.display()
        ));
    }
    Ok(LoadedPackage {
        shard,
        source,
        evidence: fs::read(&evidence_path).map_err(|error| io_error(&evidence_path, &error))?,
    })
}

fn valid_shard_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
}

fn ensure_unique_package_ids(packages: &[LoadedPackage]) -> Result<(), String> {
    for pair in packages.windows(2) {
        if pair[0].shard.id == pair[1].shard.id {
            return Err(format!(
                "CHEMS-A005 duplicate candidate package id `{}`",
                pair[0].shard.id
            ));
        }
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn merge_packages(packages: &[LoadedPackage]) -> Result<CatalogueEnvelope, String> {
    let mut document = CatalogueDocument {
        schema_version: 1,
        name: "ChemSpec.Theoretical".to_owned(),
        version: "1".to_owned(),
        publication: PublicationKind::Working,
        created: CreationMetadata {
            created_on: "2026-07-14".to_owned(),
            created_by: "ChemSpec catalogue authoring compiler".to_owned(),
            notes: Some("Untrusted generated candidate pending host-selected AI review".to_owned()),
        },
        evidence: Vec::new(),
        premises: Vec::new(),
        valence_premises: Vec::new(),
        structures: Vec::new(),
        rules: Vec::new(),
        elements: Vec::new(),
        element_categories: Vec::new(),
        structural_traits: Vec::new(),
        structure_templates: Vec::new(),
        structure_applications: Vec::new(),
        graph_patterns: Vec::new(),
        generalized_rules: Vec::new(),
        macroscopic_materials: Vec::new(),
    };
    for package in packages {
        let shard = &package.shard;
        document.evidence.extend(shard.evidence.clone());
        document.premises.extend(shard.premises.clone());
        document
            .valence_premises
            .extend(shard.valence_premises.clone());
        document.structures.extend(shard.structures.clone());
        document.rules.extend(shard.rules.clone());
        document.elements.extend(shard.elements.clone());
        document
            .element_categories
            .extend(shard.element_categories.clone());
        document
            .structural_traits
            .extend(shard.structural_traits.clone());
        document
            .structure_templates
            .extend(shard.structure_templates.clone());
        document
            .structure_applications
            .extend(shard.structure_applications.clone());
        document.graph_patterns.extend(shard.graph_patterns.clone());
        document
            .generalized_rules
            .extend(shard.generalized_rules.clone());
    }
    reject_duplicates(&document)?;
    sort_document(&mut document);
    let mut envelope = CatalogueEnvelope {
        digest: ContentDigest::sha256(b"uncomputed candidate catalogue"),
        bundle: document,
    };
    envelope.digest = envelope
        .computed_digest()
        .map_err(|error| format!("CHEMS-A006 cannot digest candidate: {error}"))?;
    Ok(envelope)
}

fn reject_duplicates(document: &CatalogueDocument) -> Result<(), String> {
    unique(
        "evidence id",
        document.evidence.iter().map(|record| record.id.to_string()),
    )?;
    unique(
        "premise id",
        document.premises.iter().map(|record| record.id.to_string()),
    )?;
    unique(
        "valence premise",
        document
            .valence_premises
            .iter()
            .map(|record| record.premise_id.to_string()),
    )?;
    unique(
        "structure id",
        document
            .structures
            .iter()
            .map(|record| record.id().to_string()),
    )?;
    unique(
        "element symbol",
        document
            .elements
            .iter()
            .map(|record| record.symbol.to_string()),
    )?;
    unique(
        "element atomic number",
        document
            .elements
            .iter()
            .map(|record| record.atomic_number.to_string()),
    )?;
    unique(
        "element name",
        document.elements.iter().map(|record| record.name.clone()),
    )?;
    unique(
        "category id",
        document
            .element_categories
            .iter()
            .map(|record| record.id.to_string()),
    )?;
    unique(
        "trait id",
        document
            .structural_traits
            .iter()
            .map(|record| record.id.to_string()),
    )?;
    unique(
        "template id",
        document
            .structure_templates
            .iter()
            .map(|record| record.id().to_string()),
    )?;
    unique(
        "application id",
        document
            .structure_applications
            .iter()
            .map(|record| record.id.to_string()),
    )?;
    reject_structure_and_alias_collisions(document)?;
    unique(
        "application alias",
        document
            .structure_applications
            .iter()
            .flat_map(|record| record.aliases.iter().cloned()),
    )?;
    unique(
        "pattern id",
        document
            .graph_patterns
            .iter()
            .map(|record| record.id.to_string()),
    )?;
    unique(
        "reaction rule id",
        document
            .rules
            .iter()
            .map(|record| record.id.to_string())
            .chain(
                document
                    .generalized_rules
                    .iter()
                    .map(|record| record.id.to_string()),
            ),
    )
}

fn reject_structure_and_alias_collisions(document: &CatalogueDocument) -> Result<(), String> {
    let mut structure_ids = document
        .structures
        .iter()
        .map(|record| record.id().to_string())
        .collect::<BTreeSet<_>>();
    for application in &document.structure_applications {
        let id = application.id.to_string();
        if !structure_ids.insert(id.clone()) {
            return Err(format!(
                "CHEMS-A005 application id `{id}` collides with a structure id before merge"
            ));
        }
    }
    for alias in document
        .structure_applications
        .iter()
        .flat_map(|record| &record.aliases)
    {
        if structure_ids.contains(alias) {
            return Err(format!(
                "CHEMS-A005 application alias `{alias}` collides with a structure or application id before merge"
            ));
        }
    }
    Ok(())
}

fn unique(label: &str, values: impl IntoIterator<Item = String>) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value.clone()) {
            return Err(format!(
                "CHEMS-A005 duplicate {label} `{value}` before merge"
            ));
        }
    }
    Ok(())
}

fn sort_document(document: &mut CatalogueDocument) {
    document
        .evidence
        .sort_by(|left, right| left.id.cmp(&right.id));
    document
        .premises
        .sort_by(|left, right| left.id.cmp(&right.id));
    document
        .valence_premises
        .sort_by(|left, right| left.premise_id.cmp(&right.premise_id));
    document
        .structures
        .sort_by(|left, right| left.id().cmp(right.id()));
    document.rules.sort_by(|left, right| left.id.cmp(&right.id));
    document.elements.sort_by_key(|record| record.atomic_number);
    document
        .element_categories
        .sort_by(|left, right| left.id.cmp(&right.id));
    document
        .structural_traits
        .sort_by(|left, right| left.id.cmp(&right.id));
    document
        .structure_templates
        .sort_by(|left, right| left.id().cmp(right.id()));
    document
        .structure_applications
        .sort_by(|left, right| left.id.cmp(&right.id));
    document
        .graph_patterns
        .sort_by(|left, right| left.id.cmp(&right.id));
    document
        .generalized_rules
        .sort_by(|left, right| left.id.cmp(&right.id));
}

fn inspect_examples(
    packages: &[LoadedPackage],
    catalogue: &ValidatedCatalogueBundle,
) -> Result<Vec<InspectionOutput>, String> {
    packages
        .iter()
        .map(|package| {
            let source_name = format!("{}/example.chems", package.shard.id);
            let expanded =
                expand_provisional(&source_name, &package.source, catalogue, &package.evidence)
                    .map_err(|error| {
                        format!(
                            "CHEMS-A030 {} example expansion failed ({:?}): {error}",
                            package.shard.id,
                            error.class()
                        )
                    })?;
            let current = CurrentArtifactIdentity::from_expanded(&expanded).map_err(|error| {
                format!("CHEMS-A032 {} identity failed: {error}", package.shard.id)
            })?;
            let derivation = validate_provisional(&expanded, catalogue).map_err(|error| {
                format!(
                    "CHEMS-A031 {} kernel validation failed ({:?}): {error}",
                    package.shard.id,
                    error.class()
                )
            })?;
            if derivation.provenance() != DerivationProvenance::Provisional {
                return Err("CHEMS-A090 provisional derivation changed provenance".to_owned());
            }
            let frames = generate_frames(&derivation, current).map_err(|error| {
                format!(
                    "CHEMS-A032 {} frame projection failed: {error}",
                    package.shard.id
                )
            })?;
            let certificate = inspection_artifact(
                "expanded-certificate",
                &serde_json::from_slice(
                    &expanded
                        .semantic_json()
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?,
            )?;
            let derivation = inspection_artifact(
                "derivation",
                &serde_json::from_slice(
                    &derivation
                        .canonical_json()
                        .map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?,
            )?;
            let frames = inspection_artifact(
                "frames",
                &serde_json::from_slice(
                    &frames.canonical_json().map_err(|error| error.to_string())?,
                )
                .map_err(|error| error.to_string())?,
            )?;
            Ok(InspectionOutput {
                id: package.shard.id.clone(),
                certificate_digest: ContentDigest::sha256(&certificate),
                derivation_digest: ContentDigest::sha256(&derivation),
                frames_digest: ContentDigest::sha256(&frames),
                certificate,
                derivation,
                frames,
            })
        })
        .collect()
}

fn inspection_artifact(kind: &str, value: &Value) -> Result<Vec<u8>, String> {
    let wrapper = json!({
        "schema_version": 1,
        "status": "candidate-inspection-only",
        "promotable": false,
        "artifact": kind,
        "value": value,
    });
    let mut bytes = serde_json::to_vec_pretty(&wrapper).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn review_request<'a>(
    catalogue: &ValidatedCatalogueBundle,
    inspections: &'a [InspectionOutput],
) -> ReviewRequest<'a> {
    ReviewRequest {
        schema_version: 1,
        status: "pending-ai-review",
        promotable: false,
        catalogue_digest: catalogue.digest(),
        evidence_sources: catalogue
            .document()
            .evidence
            .iter()
            .map(|record| record.id.to_string())
            .collect(),
        premises: catalogue
            .document()
            .premises
            .iter()
            .map(|record| record.id.to_string())
            .collect(),
        inspections: inspections
            .iter()
            .map(|inspection| {
                (
                    inspection.id.as_str(),
                    InspectionDigests {
                        expanded_certificate: inspection.certificate_digest,
                        derivation: inspection.derivation_digest,
                        frames: inspection.frames_digest,
                    },
                )
            })
            .collect(),
        required_external_artifact: "none",
        promotion_boundary: "None: a validated bundle is publishable as-is.",
    }
}

fn write_outputs(
    output: &Path,
    catalogue: &[u8],
    digest: ContentDigest,
    inspections: &[InspectionOutput],
    review: &ReviewRequest<'_>,
) -> Result<(), String> {
    fs::create_dir(output).map_err(|error| io_error(output, &error))?;
    write_file(&output.join("catalogue.json"), catalogue)?;
    write_file(
        &output.join("catalogue.digest"),
        format!("{digest}\n").as_bytes(),
    )?;
    write_file(&output.join("review-request.json"), &pretty_json(review)?)?;
    let inspection_root = output.join("inspections");
    fs::create_dir(&inspection_root).map_err(|error| io_error(&inspection_root, &error))?;
    for inspection in inspections {
        let directory = inspection_root.join(&inspection.id);
        fs::create_dir(&directory).map_err(|error| io_error(&directory, &error))?;
        write_file(
            &directory.join("expanded-certificate.json"),
            &inspection.certificate,
        )?;
        write_file(&directory.join("derivation.json"), &inspection.derivation)?;
        write_file(&directory.join("frames.json"), &inspection.frames)?;
    }
    Ok(())
}

fn pretty_json(value: &impl Serialize) -> Result<Vec<u8>, String> {
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    fs::write(path, bytes).map_err(|error| io_error(path, &error))
}

fn io_error(path: &Path, error: &io::Error) -> String {
    format!("CHEMS-A003 {}: {error}", path.display())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn physical_shard() -> CandidateShard {
        serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../catalogue/candidates/periodic-table-and-alkali-water/candidate.json"
        )))
        .unwrap()
    }

    fn merge_error(shard: CandidateShard) -> String {
        merge_packages(&[LoadedPackage {
            shard,
            source: String::new(),
            evidence: Vec::new(),
        }])
        .unwrap_err()
    }

    #[test]
    fn shard_ids_are_safe_output_components() {
        assert!(valid_shard_id("periodic-table-118"));
        for invalid in ["", "UPPER", "../escape", "-start", "end-", "two--dash"] {
            assert!(!valid_shard_id(invalid), "{invalid}");
        }
    }

    #[test]
    fn duplicate_detection_covers_aliases_and_element_facts() {
        assert!(unique("test", ["a".to_owned(), "b".to_owned()]).is_ok());
        assert_eq!(
            unique("alias", ["same".to_owned(), "same".to_owned()]).unwrap_err(),
            "CHEMS-A005 duplicate alias `same` before merge"
        );
    }

    #[test]
    fn every_merge_identity_class_rejects_duplicates_before_validation() {
        let mut shard = physical_shard();
        shard.evidence.push(shard.evidence[0].clone());
        assert!(merge_error(shard).contains("duplicate evidence id"));

        let mut shard = physical_shard();
        shard.premises.push(shard.premises[0].clone());
        assert!(merge_error(shard).contains("duplicate premise id"));

        let mut shard = physical_shard();
        shard
            .valence_premises
            .push(shard.valence_premises[0].clone());
        assert!(merge_error(shard).contains("duplicate valence premise"));

        let mut shard = physical_shard();
        shard.structures.push(shard.structures[0].clone());
        assert!(merge_error(shard).contains("duplicate structure id"));

        let mut shard = physical_shard();
        shard.elements.push(shard.elements[0].clone());
        assert!(merge_error(shard).contains("duplicate element symbol"));

        let mut shard = physical_shard();
        shard
            .element_categories
            .push(shard.element_categories[0].clone());
        assert!(merge_error(shard).contains("duplicate category id"));

        let mut shard = physical_shard();
        let trait_record: StructuralTraitDefinitionRecord = serde_json::from_value(json!({
            "id": "Traits.Duplicate",
            "sites": {},
            "premise_ids": ["premise.elements.iupac-periodic-table"]
        }))
        .unwrap();
        shard.structural_traits = vec![trait_record.clone(), trait_record];
        assert!(merge_error(shard).contains("duplicate trait id"));

        let mut shard = physical_shard();
        shard
            .structure_templates
            .push(shard.structure_templates[0].clone());
        assert!(merge_error(shard).contains("duplicate template id"));

        let mut shard = physical_shard();
        shard
            .structure_applications
            .push(shard.structure_applications[0].clone());
        assert!(merge_error(shard).contains("duplicate application id"));

        let mut shard = physical_shard();
        shard.structure_applications[0].id = shard.structures[0].id().clone();
        assert!(merge_error(shard).contains("collides with a structure id"));

        let mut shard = physical_shard();
        shard.structure_applications[0]
            .aliases
            .insert("duplicate-alias".to_owned());
        shard.structure_applications[1]
            .aliases
            .insert("duplicate-alias".to_owned());
        assert!(merge_error(shard).contains("duplicate application alias"));

        let mut shard = physical_shard();
        shard.graph_patterns.push(shard.graph_patterns[0].clone());
        assert!(merge_error(shard).contains("duplicate pattern id"));

        let mut shard = physical_shard();
        shard
            .generalized_rules
            .push(shard.generalized_rules[0].clone());
        assert!(merge_error(shard).contains("duplicate reaction rule id"));
    }
}
