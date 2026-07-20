use dealer::semver::{select_max_version, select_version};

fn versions(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

fn selected(expression: &str, candidates: &[&str]) -> Vec<String> {
    select_version(expression.to_owned(), versions(candidates)).unwrap()
}

#[test]
fn selects_an_exact_version() {
    assert_eq!(
        selected("1.2.3", &["1.2.2", "1.2.3", "1.2.4"]),
        versions(&["1.2.3"])
    );
}

#[test]
fn supports_comparison_operators() {
    let candidates = ["1.0.0", "1.5.0", "2.0.0", "2.5.0", "3.0.0"];

    assert_eq!(
        selected(">1.5.0", &candidates),
        versions(&["3.0.0", "2.5.0", "2.0.0"])
    );
    assert_eq!(
        selected(">=2.0.0", &candidates),
        versions(&["3.0.0", "2.5.0", "2.0.0"])
    );
    assert_eq!(
        selected("<2.0.0", &candidates),
        versions(&["1.5.0", "1.0.0"])
    );
    assert_eq!(
        selected("<=2.0.0", &candidates),
        versions(&["2.0.0", "1.5.0", "1.0.0"])
    );
}

#[test]
fn combines_comparisons_with_and() {
    assert_eq!(
        selected(">=1.2.0 <2.0.0", &["1.1.9", "1.2.0", "1.9.9", "2.0.0"]),
        versions(&["1.9.9", "1.2.0"])
    );
}

#[test]
fn combines_ranges_with_or() {
    assert_eq!(
        selected(
            "1.2.x || 2.0.x",
            &["1.2.0", "1.2.9", "1.3.0", "2.0.1", "2.1.0"]
        ),
        versions(&["2.0.1", "1.2.9", "1.2.0"])
    );
}

#[test]
fn expands_wildcard_ranges() {
    let candidates = ["1.0.0", "1.2.0", "1.2.9", "1.3.0", "2.0.0"];

    assert_eq!(
        selected("1.x", &candidates),
        versions(&["1.3.0", "1.2.9", "1.2.0", "1.0.0"])
    );
    assert_eq!(
        selected("1.2.*", &candidates),
        versions(&["1.2.9", "1.2.0"])
    );
}

#[test]
fn expands_tilde_ranges() {
    assert_eq!(
        selected("~1.2.3", &["1.2.2", "1.2.3", "1.2.9", "1.3.0"]),
        versions(&["1.2.9", "1.2.3"])
    );
}

#[test]
fn expands_caret_ranges_at_each_stability_level() {
    assert_eq!(
        selected("^1.2.3", &["1.2.3", "1.9.9", "2.0.0"]),
        versions(&["1.9.9", "1.2.3"])
    );
    assert_eq!(
        selected("^0.2.3", &["0.2.3", "0.2.9", "0.3.0"]),
        versions(&["0.2.9", "0.2.3"])
    );
    assert_eq!(
        selected("^0.0.3", &["0.0.2", "0.0.3", "0.0.4", "0.1.0"]),
        versions(&["0.0.3"])
    );
}

#[test]
fn ignores_invalid_candidate_versions() {
    assert_eq!(
        selected(">=1.0.0", &["invalid", "1.0", "1.0.0", "2.0.0.0"]),
        versions(&["1.0.0"])
    );
}

#[test]
fn selects_the_highest_valid_version() {
    assert_eq!(
        select_max_version(versions(&["1.9.0", "invalid", "2.0.0", "1.10.0"])),
        Some("2.0.0".to_owned())
    );
    assert_eq!(select_max_version(versions(&["invalid", "1.0"])), None);
}
