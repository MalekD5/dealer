use super::{
    parser::{ParseError, Parser},
    version::Version,
};

/// Semantic version parsing and comparison support.
pub fn select_version(
    expression: String,
    all_versions: Vec<String>,
) -> Result<Vec<String>, ParseError> {
    let parser = Parser::new();
    let range = parser.parse_full_expression(expression)?;

    let mut candidates: Vec<String> = all_versions
        .into_iter()
        .filter(|version| range.matches(version))
        .collect();

    candidates.sort_by_key(|version| std::cmp::Reverse(Version::parse(version)));
    Ok(candidates)
}

/// Selects the highest semantic version from a list of candidates.
pub fn select_max_version(candidates: Vec<String>) -> Option<String> {
    candidates
        .into_iter()
        .filter(|version| Version::parse(version).is_some())
        .max_by_key(|version| Version::parse(version))
}
