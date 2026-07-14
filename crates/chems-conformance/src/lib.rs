use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt, fs,
    path::Path,
};

use serde::Deserialize;
use serde_json::Value;

pub use chem_domain::{CanonicalJsonError, canonical_json, lowercase_hex, sha256};

pub const REQUIREMENTS_PATH: &str = "conformance/requirements.json";
pub const MANIFEST_PATH: &str = "conformance/manifest.json";
pub const REQUIREMENTS_SCHEMA_PATH: &str = "conformance/requirements.schema.json";
pub const MANIFEST_SCHEMA_PATH: &str = "conformance/manifest.schema.json";
pub const GRAMMAR_PATH: &str = "grammar/chems.ebnf";
pub const RESERVED_WORDS_PATH: &str = "conformance/reserved-words.txt";

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequirementsDocument {
    pub schema_version: u32,
    pub specification: String,
    pub requirements: Vec<Requirement>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requirement {
    pub id: String,
    pub section: String,
    #[serde(default = "default_occurrence")]
    pub occurrence: usize,
    pub component: String,
}

const fn default_occurrence() -> usize {
    1
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub requirements: String,
    pub components: Vec<Component>,
    pub cases: Vec<Case>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub id: String,
    pub fixture_directory: String,
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub id: String,
    pub component: String,
    pub requirements: Vec<String>,
    #[serde(default)]
    pub input: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub catalogue: Option<String>,
    pub expected: ExpectedResult,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedResult {
    pub state: String,
    #[serde(default)]
    pub diagnostics: Vec<ExpectedDiagnostic>,
    #[serde(default)]
    pub domain: Option<String>,
    #[serde(default)]
    pub formatted_source: Option<String>,
    #[serde(default)]
    pub cst_sha256: Option<String>,
    #[serde(default)]
    pub ast_sha256: Option<String>,
    #[serde(default)]
    pub catalogue_sha256: Option<String>,
    #[serde(default)]
    pub catalogue_review: Option<String>,
    #[serde(default)]
    pub ast: Option<String>,
    #[serde(default)]
    pub hir: Option<String>,
    #[serde(default)]
    pub derivation: Option<String>,
    #[serde(default)]
    pub artifact: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedDiagnostic {
    pub code: String,
    pub severity: String,
    pub primary_span: ExpectedSpan,
    #[serde(default)]
    pub related_spans: Vec<ExpectedSpan>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedSpan {
    pub fixture: String,
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentCoverage {
    pub component: String,
    pub cases: usize,
    pub covered_requirements: usize,
    pub total_requirements: usize,
}

#[derive(Debug, Clone)]
pub struct ValidationSummary {
    pub requirements: usize,
    pub grammar_productions: usize,
    pub reserved_words: usize,
    pub components: usize,
    pub cases: usize,
    pub coverage: Vec<ComponentCoverage>,
}

impl ValidationSummary {
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.requirements > 0
            && self.cases > 0
            && self
                .coverage
                .iter()
                .all(|item| item.cases > 0 && item.covered_requirements == item.total_requirements)
    }
}

#[derive(Debug)]
pub struct ValidationError {
    message: String,
}

impl ValidationError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ValidationError {}

/// Validates every executable Slice 0 contract rooted at `root`.
///
/// # Errors
///
/// Returns an error when a schema or data file is unreadable, a requirement or
/// case is malformed, a referenced section or fixture is absent, the grammar is
/// inconsistent, or the reserved-word sources disagree.
pub fn validate_repository(root: &Path) -> Result<ValidationSummary, ValidationError> {
    let requirements_schema = read_json_value(&root.join(REQUIREMENTS_SCHEMA_PATH))?;
    let manifest_schema = read_json_value(&root.join(MANIFEST_SCHEMA_PATH))?;
    let requirements_value = read_json_value(&root.join(REQUIREMENTS_PATH))?;
    let manifest_value = read_json_value(&root.join(MANIFEST_PATH))?;
    validate_json_schema(
        REQUIREMENTS_SCHEMA_PATH,
        &requirements_schema,
        REQUIREMENTS_PATH,
        &requirements_value,
    )?;
    validate_json_schema(
        MANIFEST_SCHEMA_PATH,
        &manifest_schema,
        MANIFEST_PATH,
        &manifest_value,
    )?;
    let requirements: RequirementsDocument =
        deserialize_json(REQUIREMENTS_PATH, requirements_value)?;
    let manifest: Manifest = deserialize_json(MANIFEST_PATH, manifest_value)?;

    if requirements.schema_version != 1 {
        return Err(ValidationError::new(format!(
            "unsupported requirements schema version {}",
            requirements.schema_version
        )));
    }
    if manifest.schema_version != 1 {
        return Err(ValidationError::new(format!(
            "unsupported manifest schema version {}",
            manifest.schema_version
        )));
    }
    if manifest.requirements != REQUIREMENTS_PATH {
        return Err(ValidationError::new(format!(
            "manifest requirements path must be `{REQUIREMENTS_PATH}`"
        )));
    }

    let specification_path = root.join(&requirements.specification);
    let specification = read_text(&specification_path)?;
    let component_ids = validate_components(root, &manifest.components)?;
    let requirement_owners =
        validate_requirements(&specification, &requirements.requirements, &component_ids)?;
    validate_cases(root, &manifest.cases, &component_ids, &requirement_owners)?;

    let grammar = read_text(&root.join(GRAMMAR_PATH))?;
    let grammar_report = validate_grammar(&grammar)?;
    let reserved_words = read_reserved_words(&root.join(RESERVED_WORDS_PATH))?;
    validate_reserved_words(&specification, &grammar_report.keywords, &reserved_words)?;

    let coverage = coverage(
        &requirements.requirements,
        &manifest.components,
        &manifest.cases,
    );
    Ok(ValidationSummary {
        requirements: requirements.requirements.len(),
        grammar_productions: grammar_report.productions,
        reserved_words: reserved_words.len(),
        components: manifest.components.len(),
        cases: manifest.cases.len(),
        coverage,
    })
}

fn read_text(path: &Path) -> Result<String, ValidationError> {
    fs::read_to_string(path).map_err(|error| {
        ValidationError::new(format!("could not read {}: {error}", path.display()))
    })
}

fn read_json_value(path: &Path) -> Result<Value, ValidationError> {
    let source = read_text(path)?;
    serde_json::from_str(&source).map_err(|error| {
        ValidationError::new(format!("invalid JSON in {}: {error}", path.display()))
    })
}

fn deserialize_json<T: for<'de> Deserialize<'de>>(
    name: &str,
    value: Value,
) -> Result<T, ValidationError> {
    serde_json::from_value(value)
        .map_err(|error| ValidationError::new(format!("invalid data in {name}: {error}")))
}

fn validate_json_schema(
    schema_name: &str,
    schema: &Value,
    instance_name: &str,
    instance: &Value,
) -> Result<(), ValidationError> {
    let validator = jsonschema::draft202012::new(schema).map_err(|error| {
        ValidationError::new(format!("invalid JSON Schema in {schema_name}: {error}"))
    })?;
    validator.validate(instance).map_err(|error| {
        ValidationError::new(format!(
            "{instance_name} does not satisfy {schema_name}: {error}"
        ))
    })
}

fn validate_components(
    root: &Path,
    components: &[Component],
) -> Result<HashSet<String>, ValidationError> {
    if components.is_empty() {
        return Err(ValidationError::new(
            "conformance manifest must declare at least one component",
        ));
    }
    let mut ids = HashSet::new();
    for component in components {
        if !valid_kebab_id(&component.id) {
            return Err(ValidationError::new(format!(
                "invalid component id `{}`",
                component.id
            )));
        }
        if !ids.insert(component.id.clone()) {
            return Err(ValidationError::new(format!(
                "duplicate component id `{}`",
                component.id
            )));
        }
        if component.description.trim().is_empty() {
            return Err(ValidationError::new(format!(
                "component `{}` has no description",
                component.id
            )));
        }
        let expected_directory = format!("conformance/{}", component.id);
        if component.fixture_directory != expected_directory {
            return Err(ValidationError::new(format!(
                "component `{}` fixture directory must be `{expected_directory}`",
                component.id
            )));
        }
        let directory = root.join(&component.fixture_directory);
        if !directory.is_dir() {
            return Err(ValidationError::new(format!(
                "component `{}` fixture directory does not exist: {}",
                component.id,
                directory.display()
            )));
        }
    }
    Ok(ids)
}

fn validate_requirements(
    specification: &str,
    requirements: &[Requirement],
    component_ids: &HashSet<String>,
) -> Result<HashMap<String, String>, ValidationError> {
    if requirements.is_empty() {
        return Err(ValidationError::new(
            "requirements document must not be empty",
        ));
    }

    let headings = specification_headings(specification);
    let mut heading_counts = HashMap::<String, usize>::new();
    for heading in &headings {
        *heading_counts.entry(heading.clone()).or_default() += 1;
    }

    let mut owners = HashMap::new();
    let mut mapped = HashSet::<(String, usize)>::new();
    for requirement in requirements {
        if !valid_requirement_id(&requirement.id) {
            return Err(ValidationError::new(format!(
                "invalid requirement id `{}`",
                requirement.id
            )));
        }
        if owners
            .insert(requirement.id.clone(), requirement.component.clone())
            .is_some()
        {
            return Err(ValidationError::new(format!(
                "duplicate requirement id `{}`",
                requirement.id
            )));
        }
        if !component_ids.contains(&requirement.component) {
            return Err(ValidationError::new(format!(
                "requirement `{}` references unknown component `{}`",
                requirement.id, requirement.component
            )));
        }
        let available = heading_counts
            .get(&requirement.section)
            .copied()
            .unwrap_or_default();
        if requirement.occurrence == 0 || requirement.occurrence > available {
            return Err(ValidationError::new(format!(
                "requirement `{}` references missing specification section `{}` occurrence {}",
                requirement.id, requirement.section, requirement.occurrence
            )));
        }
        if !mapped.insert((requirement.section.clone(), requirement.occurrence)) {
            return Err(ValidationError::new(format!(
                "specification section `{}` occurrence {} has more than one requirement id",
                requirement.section, requirement.occurrence
            )));
        }
    }

    let mut seen = HashMap::<String, usize>::new();
    for heading in headings {
        let occurrence = seen.entry(heading.clone()).or_default();
        *occurrence += 1;
        if requires_mapping(&heading) && !mapped.contains(&(heading.clone(), *occurrence)) {
            return Err(ValidationError::new(format!(
                "normative specification section `{heading}` occurrence {occurrence} has no requirement id"
            )));
        }
    }
    Ok(owners)
}

fn specification_headings(specification: &str) -> Vec<String> {
    specification
        .lines()
        .filter_map(|line| {
            let level = line.bytes().take_while(|byte| *byte == b'#').count();
            if (2..=4).contains(&level) && line.as_bytes().get(level) == Some(&b' ') {
                Some(line[level + 1..].trim_matches('`').to_owned())
            } else {
                None
            }
        })
        .collect()
}

fn requires_mapping(heading: &str) -> bool {
    !matches!(
        heading,
        "Status and authority"
            | "Illustrative source shape"
            | "Specification requirement identifiers"
            | "Specification completion criteria"
            | "Implementation handoff"
    ) && !heading.starts_with("Decisions fixed by")
}

fn validate_cases(
    root: &Path,
    cases: &[Case],
    component_ids: &HashSet<String>,
    requirement_owners: &HashMap<String, String>,
) -> Result<(), ValidationError> {
    let mut ids = HashSet::new();
    for case in cases {
        if !valid_case_id(&case.id) {
            return Err(ValidationError::new(format!(
                "invalid conformance case id `{}`",
                case.id
            )));
        }
        if !ids.insert(case.id.clone()) {
            return Err(ValidationError::new(format!(
                "duplicate conformance case id `{}`",
                case.id
            )));
        }
        if !component_ids.contains(&case.component) {
            return Err(ValidationError::new(format!(
                "case `{}` references unknown component `{}`",
                case.id, case.component
            )));
        }
        if case.requirements.is_empty() {
            return Err(ValidationError::new(format!(
                "case `{}` does not reference a requirement",
                case.id
            )));
        }
        let mut case_requirements = HashSet::new();
        for requirement in &case.requirements {
            let Some(owner) = requirement_owners.get(requirement) else {
                return Err(ValidationError::new(format!(
                    "case `{}` references unknown requirement `{requirement}`",
                    case.id
                )));
            };
            if owner != &case.component {
                return Err(ValidationError::new(format!(
                    "case `{}` belongs to component `{}` but requirement `{requirement}` belongs to `{owner}`",
                    case.id, case.component
                )));
            }
            if !case_requirements.insert(requirement) {
                return Err(ValidationError::new(format!(
                    "case `{}` references requirement `{requirement}` more than once",
                    case.id
                )));
            }
        }
        for fixture in case_fixture_paths(case) {
            validate_fixture_path(root, fixture, &case.id, &case.component)?;
        }
        if !matches!(
            case.expected.state.as_str(),
            "malformed"
                | "ill-typed"
                | "incomplete"
                | "invalid"
                | "unsupported"
                | "validated-with-assumptions"
                | "validated"
                | "system-error"
        ) {
            return Err(ValidationError::new(format!(
                "case `{}` has invalid expected state `{}`",
                case.id, case.expected.state
            )));
        }
        validate_expected_diagnostics(root, case)?;
    }
    Ok(())
}

fn validate_fixture_path(
    root: &Path,
    fixture: &str,
    case_id: &str,
    component: &str,
) -> Result<(), ValidationError> {
    let relative = Path::new(fixture);
    let expected_directory = Path::new("conformance").join(component);
    if relative.parent() != Some(expected_directory.as_path()) {
        return Err(ValidationError::new(format!(
            "case `{case_id}` fixture must be directly owned by component `{component}`: {}",
            relative.display()
        )));
    }
    let Some(file_name) = relative.file_name().and_then(|name| name.to_str()) else {
        return Err(ValidationError::new(format!(
            "case `{case_id}` fixture name is not UTF-8"
        )));
    };
    let Some((fixture_case_id, extensions)) = file_name.split_once('.') else {
        return Err(ValidationError::new(format!(
            "case `{case_id}` fixture must have an extension: {file_name}"
        )));
    };
    if fixture_case_id != case_id
        || extensions.is_empty()
        || extensions
            .split('.')
            .any(|part| part.is_empty() || !part.chars().all(|c| c.is_ascii_lowercase()))
    {
        return Err(ValidationError::new(format!(
            "case `{case_id}` fixture must use `<case-id>.<lowercase-extensions>`: {file_name}"
        )));
    }

    let path = root.join(relative);
    if !path.is_file() {
        return Err(ValidationError::new(format!(
            "case `{case_id}` fixture does not exist: {}",
            path.display()
        )));
    }
    let canonical_root = root.canonicalize().map_err(|error| {
        ValidationError::new(format!(
            "could not resolve workspace root {}: {error}",
            root.display()
        ))
    })?;
    let canonical_directory = root
        .join(&expected_directory)
        .canonicalize()
        .map_err(|error| {
            ValidationError::new(format!(
                "could not resolve component directory {}: {error}",
                expected_directory.display()
            ))
        })?;
    let canonical_path = path.canonicalize().map_err(|error| {
        ValidationError::new(format!(
            "could not resolve fixture {}: {error}",
            path.display()
        ))
    })?;
    if !canonical_directory.starts_with(&canonical_root)
        || canonical_path.parent() != Some(canonical_directory.as_path())
    {
        return Err(ValidationError::new(format!(
            "case `{case_id}` fixture resolves outside its component directory: {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_expected_diagnostics(root: &Path, case: &Case) -> Result<(), ValidationError> {
    for diagnostic in &case.expected.diagnostics {
        if !valid_diagnostic_code(&diagnostic.code) {
            return Err(ValidationError::new(format!(
                "case `{}` has invalid diagnostic code `{}`",
                case.id, diagnostic.code
            )));
        }
        if !matches!(
            diagnostic.severity.as_str(),
            "Error" | "Warning" | "Information"
        ) {
            return Err(ValidationError::new(format!(
                "case `{}` has invalid diagnostic severity `{}`",
                case.id, diagnostic.severity
            )));
        }
        for span in std::iter::once(&diagnostic.primary_span).chain(&diagnostic.related_spans) {
            if span.start > span.end {
                return Err(ValidationError::new(format!(
                    "case `{}` diagnostic `{}` has a reversed span {}..{}",
                    case.id, diagnostic.code, span.start, span.end
                )));
            }
            let fixture_length = fs::metadata(root.join(&span.fixture))
                .map_err(|error| {
                    ValidationError::new(format!(
                        "could not inspect diagnostic fixture {}: {error}",
                        span.fixture
                    ))
                })?
                .len();
            if span.end > fixture_length {
                return Err(ValidationError::new(format!(
                    "case `{}` diagnostic `{}` span {}..{} exceeds fixture length {fixture_length}",
                    case.id, diagnostic.code, span.start, span.end
                )));
            }
        }
    }
    Ok(())
}

fn valid_diagnostic_code(code: &str) -> bool {
    let bytes = code.as_bytes();
    bytes.len() == 10
        && bytes.starts_with(b"CHEMS-")
        && matches!(
            bytes[6],
            b'L' | b'P' | b'T' | b'C' | b'K' | b'F' | b'I' | b'S'
        )
        && bytes[7..].iter().all(u8::is_ascii_digit)
}

fn case_fixture_paths(case: &Case) -> Vec<&str> {
    let mut paths = case
        .input
        .iter()
        .chain(case.source.iter())
        .chain(case.catalogue.iter())
        .chain(case.expected.domain.iter())
        .chain(case.expected.formatted_source.iter())
        .chain(case.expected.cst_sha256.iter())
        .chain(case.expected.ast_sha256.iter())
        .chain(case.expected.catalogue_sha256.iter())
        .chain(case.expected.catalogue_review.iter())
        .chain(case.expected.ast.iter())
        .chain(case.expected.hir.iter())
        .chain(case.expected.derivation.iter())
        .chain(case.expected.artifact.iter())
        .map(String::as_str)
        .collect::<Vec<_>>();
    for diagnostic in &case.expected.diagnostics {
        paths.push(&diagnostic.primary_span.fixture);
        paths.extend(
            diagnostic
                .related_spans
                .iter()
                .map(|span| span.fixture.as_str()),
        );
    }
    paths
}

fn coverage(
    requirements: &[Requirement],
    components: &[Component],
    cases: &[Case],
) -> Vec<ComponentCoverage> {
    let mut requirements_by_component = HashMap::<&str, BTreeSet<&str>>::new();
    for requirement in requirements {
        requirements_by_component
            .entry(&requirement.component)
            .or_default()
            .insert(&requirement.id);
    }
    let mut covered_by_component = HashMap::<&str, BTreeSet<&str>>::new();
    let mut cases_by_component = HashMap::<&str, usize>::new();
    for case in cases {
        *cases_by_component.entry(&case.component).or_default() += 1;
        covered_by_component
            .entry(&case.component)
            .or_default()
            .extend(case.requirements.iter().map(String::as_str));
    }
    components
        .iter()
        .map(|component| ComponentCoverage {
            component: component.id.clone(),
            cases: cases_by_component
                .get(component.id.as_str())
                .copied()
                .unwrap_or_default(),
            covered_requirements: covered_by_component
                .get(component.id.as_str())
                .map_or(0, BTreeSet::len),
            total_requirements: requirements_by_component
                .get(component.id.as_str())
                .map_or(0, BTreeSet::len),
        })
        .collect()
}

#[derive(Debug)]
struct GrammarReport {
    productions: usize,
    keywords: BTreeSet<String>,
}

fn validate_grammar(source: &str) -> Result<GrammarReport, ValidationError> {
    if !source.is_ascii() {
        return Err(ValidationError::new(
            "normative grammar must use ASCII notation",
        ));
    }
    let stripped = strip_ebnf_comments(source)?;
    let mut bodies = BTreeMap::<String, String>::new();
    let mut current_name: Option<String> = None;
    let mut current_body = String::new();

    for line in stripped.lines() {
        if current_name.is_none() {
            let Some(equals) = line.find('=') else {
                if !line.trim().is_empty() {
                    return Err(ValidationError::new(format!(
                        "unexpected grammar text outside a production: `{}`",
                        line.trim()
                    )));
                }
                continue;
            };
            let name = line[..equals].trim();
            if !valid_production_name(name) {
                return Err(ValidationError::new(format!(
                    "invalid grammar production name `{name}`"
                )));
            }
            current_name = Some(name.to_owned());
            current_body.push_str(&line[equals + 1..]);
        } else {
            current_body.push(' ');
            current_body.push_str(line.trim());
        }

        if current_body.trim_end().ends_with(';') {
            let name = current_name.take().expect("production name is present");
            if bodies
                .insert(name.clone(), current_body.trim().to_owned())
                .is_some()
            {
                return Err(ValidationError::new(format!(
                    "duplicate grammar production `{name}`"
                )));
            }
            current_body.clear();
        }
    }
    if let Some(name) = current_name {
        return Err(ValidationError::new(format!(
            "unterminated grammar production `{name}`"
        )));
    }
    if !bodies.contains_key("document") {
        return Err(ValidationError::new(
            "grammar does not define the `document` root production",
        ));
    }

    let names = bodies.keys().cloned().collect::<HashSet<_>>();
    let mut references = HashMap::<String, BTreeSet<String>>::new();
    let mut keywords = BTreeSet::new();
    for (name, body) in &bodies {
        let (identifiers, literals) = grammar_body_tokens(body)?;
        for identifier in &identifiers {
            if !names.contains(identifier) {
                return Err(ValidationError::new(format!(
                    "production `{name}` references undefined production `{identifier}`"
                )));
            }
        }
        references.insert(name.clone(), identifiers);
        keywords.extend(literals.into_iter().filter(|literal| {
            !literal.is_empty()
                && literal
                    .chars()
                    .all(|character| character.is_ascii_alphabetic())
        }));
    }

    let mut reachable = HashSet::new();
    let mut pending = vec!["document".to_owned()];
    while let Some(name) = pending.pop() {
        if !reachable.insert(name.clone()) {
            continue;
        }
        if let Some(children) = references.get(&name) {
            pending.extend(children.iter().cloned());
        }
    }
    let unreachable = names.difference(&reachable).cloned().collect::<Vec<_>>();
    if !unreachable.is_empty() {
        return Err(ValidationError::new(format!(
            "unreachable grammar productions: {}",
            unreachable.join(", ")
        )));
    }

    Ok(GrammarReport {
        productions: bodies.len(),
        keywords,
    })
}

fn strip_ebnf_comments(source: &str) -> Result<String, ValidationError> {
    let bytes = source.as_bytes();
    let mut output = String::with_capacity(source.len());
    let mut index = 0;
    let mut depth = 0_u32;
    while index < bytes.len() {
        if bytes.get(index..index + 2) == Some(b"(*") {
            depth += 1;
            index += 2;
        } else if bytes.get(index..index + 2) == Some(b"*)") {
            if depth == 0 {
                return Err(ValidationError::new("grammar contains an unmatched `*)`"));
            }
            depth -= 1;
            index += 2;
        } else {
            if depth == 0 {
                output.push(char::from(bytes[index]));
            } else if bytes[index] == b'\n' {
                output.push('\n');
            }
            index += 1;
        }
    }
    if depth != 0 {
        return Err(ValidationError::new(
            "grammar contains an unclosed `(* ... *)` comment",
        ));
    }
    Ok(output)
}

fn grammar_body_tokens(
    body: &str,
) -> Result<(BTreeSet<String>, BTreeSet<String>), ValidationError> {
    let bytes = body.as_bytes();
    let mut identifiers = BTreeSet::new();
    let mut literals = BTreeSet::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'"' {
            let start = index + 1;
            index += 1;
            while index < bytes.len() && bytes[index] != b'"' {
                index += 1;
            }
            if index == bytes.len() {
                return Err(ValidationError::new(
                    "grammar contains an unclosed string literal",
                ));
            }
            literals.insert(body[start..index].to_owned());
            index += 1;
        } else if bytes[index].is_ascii_lowercase() {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_lowercase()
                    || bytes[index].is_ascii_digit()
                    || bytes[index] == b'-')
            {
                index += 1;
            }
            identifiers.insert(body[start..index].to_owned());
        } else {
            index += 1;
        }
    }
    Ok((identifiers, literals))
}

fn read_reserved_words(path: &Path) -> Result<BTreeSet<String>, ValidationError> {
    let source = read_text(path)?;
    let mut words = BTreeSet::new();
    for line in source.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !line
            .chars()
            .all(|character| character.is_ascii_alphabetic())
        {
            return Err(ValidationError::new(format!(
                "invalid reserved word `{line}`"
            )));
        }
        if !words.insert(line.to_owned()) {
            return Err(ValidationError::new(format!(
                "duplicate reserved word `{line}`"
            )));
        }
    }
    Ok(words)
}

fn validate_reserved_words(
    specification: &str,
    grammar_keywords: &BTreeSet<String>,
    reserved_words: &BTreeSet<String>,
) -> Result<(), ValidationError> {
    let documented = documented_reserved_words(specification)?;
    if &documented != reserved_words {
        let missing = documented
            .difference(reserved_words)
            .cloned()
            .collect::<Vec<_>>();
        let extra = reserved_words
            .difference(&documented)
            .cloned()
            .collect::<Vec<_>>();
        return Err(ValidationError::new(format!(
            "reserved-word data differs from specification; missing [{}], extra [{}]",
            missing.join(", "),
            extra.join(", ")
        )));
    }
    let absent = grammar_keywords
        .difference(reserved_words)
        .cloned()
        .collect::<Vec<_>>();
    if !absent.is_empty() {
        return Err(ValidationError::new(format!(
            "grammar keywords missing from reserved-word data: {}",
            absent.join(", ")
        )));
    }
    Ok(())
}

fn documented_reserved_words(specification: &str) -> Result<BTreeSet<String>, ValidationError> {
    let heading = "### Reserved words";
    let start = specification
        .find(heading)
        .ok_or_else(|| ValidationError::new("specification has no Reserved words section"))?;
    let after_heading = &specification[start + heading.len()..];
    let fence_start = after_heading
        .find("```text")
        .ok_or_else(|| ValidationError::new("Reserved words section has no text code block"))?;
    let words_start = fence_start + "```text".len();
    let remainder = &after_heading[words_start..];
    let fence_end = remainder
        .find("```")
        .ok_or_else(|| ValidationError::new("Reserved words code block is not closed"))?;
    Ok(remainder[..fence_end]
        .split_whitespace()
        .map(str::to_owned)
        .collect())
}

fn valid_requirement_id(id: &str) -> bool {
    let Some((prefix, number)) = id.split_once('-') else {
        return false;
    };
    (3..=4).contains(&prefix.len())
        && prefix
            .chars()
            .all(|character| character.is_ascii_uppercase())
        && number.len() == 3
        && number.chars().all(|character| character.is_ascii_digit())
}

fn valid_kebab_id(id: &str) -> bool {
    let mut parts = id.split('-');
    parts.next().is_some_and(|part| {
        !part.is_empty() && part.chars().all(|character| character.is_ascii_lowercase())
    }) && parts.all(|part| {
        !part.is_empty() && part.chars().all(|character| character.is_ascii_lowercase())
    })
}

fn valid_case_id(id: &str) -> bool {
    id.rsplit_once('-').is_some_and(|(stem, suffix)| {
        valid_kebab_id(stem)
            && suffix.len() == 3
            && suffix.chars().all(|character| character.is_ascii_digit())
    })
}

fn valid_production_name(name: &str) -> bool {
    !name.is_empty()
        && name.as_bytes()[0].is_ascii_lowercase()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::Path};

    use serde_json::json;

    use super::{
        CanonicalJsonError, Case, ExpectedResult, canonical_json, lowercase_hex, sha256,
        valid_case_id, valid_kebab_id, validate_cases, validate_fixture_path,
    };

    #[test]
    fn case_ids_require_kebab_case_and_a_three_digit_suffix() {
        assert!(valid_case_id("silver-chloride-001"));
        assert!(!valid_case_id("silver-chloride"));
        assert!(!valid_case_id("Silver-Chloride-001"));
        assert!(!valid_case_id("silver-chloride-01"));
    }

    #[test]
    fn kebab_ids_reject_empty_segments() {
        assert!(valid_kebab_id("kernel-tactics"));
        assert!(!valid_kebab_id("kernel--tactics"));
        assert!(!valid_kebab_id("-kernel"));
        assert!(!valid_kebab_id("kernel-"));
    }

    #[test]
    fn a_case_cannot_cover_another_components_requirement() {
        let case = Case {
            id: "cross-component-001".to_owned(),
            component: "parsing".to_owned(),
            requirements: vec!["TYP-001".to_owned()],
            input: None,
            source: None,
            catalogue: None,
            expected: ExpectedResult {
                state: "validated".to_owned(),
                diagnostics: Vec::new(),
                domain: None,
                formatted_source: None,
                cst_sha256: None,
                ast_sha256: None,
                catalogue_sha256: None,
                catalogue_review: None,
                ast: None,
                hir: None,
                derivation: None,
                artifact: None,
            },
        };
        let component_ids = ["parsing".to_owned(), "quantities-types".to_owned()]
            .into_iter()
            .collect();
        let requirement_owners =
            HashMap::from([("TYP-001".to_owned(), "quantities-types".to_owned())]);
        let error = validate_cases(Path::new("."), &[case], &component_ids, &requirement_owners)
            .expect_err("cross-component coverage must be rejected");
        assert!(error.to_string().contains("belongs to `quantities-types`"));
    }

    #[test]
    fn fixture_paths_must_match_the_case_and_component() {
        let error = validate_fixture_path(
            Path::new("."),
            "conformance/parsing/another-case-001.chems",
            "expected-case-001",
            "parsing",
        )
        .expect_err("mismatched fixture name must be rejected before file access");
        assert!(error.to_string().contains("<case-id>"));

        let error = validate_fixture_path(
            Path::new("."),
            "conformance/catalogue/expected-case-001.chems",
            "expected-case-001",
            "parsing",
        )
        .expect_err("cross-component fixture must be rejected before file access");
        assert!(error.to_string().contains("directly owned"));
    }

    #[cfg(unix)]
    #[test]
    fn fixture_symlinks_cannot_escape_the_component_directory() {
        use std::{
            fs,
            os::unix::fs::symlink,
            time::{SystemTime, UNIX_EPOCH},
        };

        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should follow Unix epoch")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("chems-conformance-{}-{nonce}", std::process::id()));
        let component = root.join("conformance/parsing");
        fs::create_dir_all(&component).expect("component directory should be created");
        let outside = root.join("outside.chems");
        fs::write(&outside, "chems 1\n").expect("outside fixture should be created");
        let link = component.join("escape-001.chems");
        symlink(&outside, &link).expect("fixture symlink should be created");

        let error = validate_fixture_path(
            &root,
            "conformance/parsing/escape-001.chems",
            "escape-001",
            "parsing",
        )
        .expect_err("escaping fixture symlink must be rejected");
        assert!(error.to_string().contains("resolves outside"));
        fs::remove_dir_all(root).expect("temporary conformance tree should be removed");
    }

    #[test]
    fn canonical_json_sorts_keys_and_has_no_whitespace() {
        let value = json!({"z": [3, 2, 1], "a": {"b": true, "a": null}});
        assert_eq!(
            canonical_json(&value).expect("integer JSON is canonicalizable"),
            br#"{"a":{"a":null,"b":true},"z":[3,2,1]}"#
        );
    }

    #[test]
    fn canonical_json_rejects_binary_float_values() {
        assert_eq!(
            canonical_json(&json!({"amount": 0.1})),
            Err(CanonicalJsonError::FloatingPointNumber)
        );
    }

    #[test]
    fn sha256_matches_published_vectors() {
        assert_eq!(
            lowercase_hex(&sha256(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            lowercase_hex(&sha256(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
