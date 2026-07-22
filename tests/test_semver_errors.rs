use dealer::semver::select_version;

fn assert_invalid(expression: &str) {
    let error = select_version(expression.to_owned(), vec!["1.2.3".to_owned()])
        .expect_err("expression should be rejected");

    assert!(
        error
            .to_string()
            .contains("invalid semantic version or range"),
        "unexpected error: {error}"
    );
}

#[test]
fn rejects_empty_expressions_and_alternatives() {
    for expression in ["", "   ", "|| 1.2.3", "1.2.3 ||", "1.2.3 || || 2.0.0"] {
        assert_invalid(expression);
    }
}

#[test]
fn rejects_incomplete_or_malformed_versions() {
    for expression in ["1", "1.2", "1.2.3.4", "one.2.3", ">=", "^", "~"] {
        assert_invalid(expression);
    }
}

#[test]
fn rejects_malformed_wildcards() {
    for expression in ["*.1", "1.x.3", "1.2.x.4", "x.1.2"] {
        assert_invalid(expression);
    }
}

#[test]
fn rejects_ranges_whose_upper_bound_overflows() {
    for expression in [
        "^18446744073709551615.0.0",
        "^0.18446744073709551615.0",
        "^0.0.18446744073709551615",
        "~1.18446744073709551615.0",
        "18446744073709551615.x",
        "1.18446744073709551615.x",
    ] {
        assert_invalid(expression);
    }
}
