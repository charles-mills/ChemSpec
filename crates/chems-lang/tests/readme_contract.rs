use chems_lang::parse_source;

const README: &str = include_str!("../../../README.md");
const CANONICAL_SOURCE: &str = include_str!("../../../fixtures/silver-chloride.chems");

#[test]
fn readme_canonical_example_matches_the_reviewed_fixture() {
    let example = readme_chems_example();
    let parsed = parse_source(&example);

    assert!(parsed.diagnostics.is_empty(), "{:#?}", parsed.diagnostics);
    assert_eq!(example, CANONICAL_SOURCE);
}

fn readme_chems_example() -> String {
    let normalized = README.replace("\r\n", "\n");
    let opening = "```chems\n";
    let start = normalized
        .find(opening)
        .expect("README contains a chems fenced code block")
        + opening.len();
    let remainder = &normalized[start..];
    let end = remainder
        .find("\n```")
        .expect("README closes its chems fenced code block");

    format!("{}\n", &remainder[..end])
}
