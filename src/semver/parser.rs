use std::{error::Error, fmt};

use super::{
    range::{ComparatorOperand, Comparison, ComparisonSet, Range},
    version::Version,
};

pub(super) struct Parser;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    input: String,
}

impl ParseError {
    fn new(input: &str) -> Self {
        Self {
            input: input.to_owned(),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "invalid semantic version or range: {:?}",
            self.input
        )
    }
}

impl Error for ParseError {}

impl Parser {
    pub(super) fn new() -> Self {
        Self
    }

    pub(super) fn parse_full_expression(&self, expression: String) -> Result<Range, ParseError> {
        if expression.trim().is_empty() {
            return Err(ParseError::new(&expression));
        }

        let mut alternatives = Vec::new();
        for part in expression.split("||") {
            if part.trim().is_empty() {
                return Err(ParseError::new(&expression));
            }

            let mut comparisons = Vec::new();
            for expression in part.split_whitespace() {
                comparisons.extend(self.parse_single_expression(expression)?);
            }
            alternatives.push(ComparisonSet::new(comparisons));
        }

        Ok(Range::new(alternatives))
    }

    fn parse_single_expression(&self, expression: &str) -> Result<Vec<Comparison>, ParseError> {
        if let Some(value) = expression
            .strip_prefix(">=")
            .or_else(|| expression.strip_prefix("=>"))
        {
            return Ok(single(ComparatorOperand::GreaterEq, parse_version(value)?));
        }
        if let Some(value) = expression
            .strip_prefix("<=")
            .or_else(|| expression.strip_prefix("=<"))
        {
            return Ok(single(ComparatorOperand::LessEq, parse_version(value)?));
        }
        if let Some(value) = expression.strip_prefix('>') {
            return Ok(single(ComparatorOperand::Greater, parse_version(value)?));
        }
        if let Some(value) = expression.strip_prefix('<') {
            return Ok(single(ComparatorOperand::Less, parse_version(value)?));
        }
        if let Some(value) = expression.strip_prefix('^') {
            let version = parse_version(value)?;
            let upper = if version.major == 0 {
                Version {
                    major: 0,
                    minor: version.minor + 1,
                    patch: 0,
                }
            } else {
                Version {
                    major: version.major + 1,
                    minor: 0,
                    patch: 0,
                }
            };
            return Ok(range(version, upper));
        }
        if let Some(value) = expression.strip_prefix('~') {
            let version = parse_version(value)?;
            let upper = Version {
                major: version.major,
                minor: version.minor + 1,
                patch: 0,
            };
            return Ok(range(version, upper));
        }

        let parts: Vec<&str> = expression.split('.').collect();
        if parts.len() == 2 && is_wildcard(parts[1]) {
            let major = parse_number(parts[0], expression)?;
            return Ok(range(
                Version {
                    major,
                    minor: 0,
                    patch: 0,
                },
                Version {
                    major: major + 1,
                    minor: 0,
                    patch: 0,
                },
            ));
        }
        if parts.len() == 3 && is_wildcard(parts[2]) {
            let major = parse_number(parts[0], expression)?;
            let minor = parse_number(parts[1], expression)?;
            return Ok(range(
                Version {
                    major,
                    minor,
                    patch: 0,
                },
                Version {
                    major,
                    minor: minor + 1,
                    patch: 0,
                },
            ));
        }

        Ok(single(
            ComparatorOperand::Equals,
            parse_version(expression)?,
        ))
    }
}

fn parse_version(value: &str) -> Result<Version, ParseError> {
    Version::parse(value).ok_or_else(|| ParseError::new(value))
}

fn parse_number(value: &str, expression: &str) -> Result<u64, ParseError> {
    value.parse().map_err(|_| ParseError::new(expression))
}

fn is_wildcard(value: &str) -> bool {
    value.eq_ignore_ascii_case("x") || value == "*"
}

fn range(lower: Version, upper: Version) -> Vec<Comparison> {
    vec![
        Comparison::new(ComparatorOperand::GreaterEq, lower),
        Comparison::new(ComparatorOperand::Less, upper),
    ]
}

fn single(operand: ComparatorOperand, version: Version) -> Vec<Comparison> {
    vec![Comparison::new(operand, version)]
}

#[cfg(test)]
mod tests {
    use super::Parser;

    fn matches(expression: &str, version: &str) -> bool {
        Parser::new()
            .parse_full_expression(expression.to_owned())
            .unwrap()
            .matches(version)
    }

    #[test]
    fn parses_exact_comparators_and_boolean_groups() {
        assert!(matches("1.2.3", "1.2.3"));
        assert!(matches(">=1.2.3 <2.0.0", "1.8.0"));
        assert!(matches("1.2.3 || 2.0.0", "2.0.0"));
        assert!(!matches(">=1.2.3 <2.0.0", "2.0.0"));
    }

    #[test]
    fn expands_caret_tilde_and_wildcards() {
        assert!(matches("^1.2.3", "1.9.9"));
        assert!(!matches("^1.2.3", "2.0.0"));
        assert!(matches("~1.2.3", "1.2.9"));
        assert!(matches("1.2.x", "1.2.99"));
        assert!(matches("1.x", "1.9.0"));
    }

    #[test]
    fn rejects_malformed_expressions() {
        let parser = Parser::new();
        for expression in ["", "nope", ">=", "1.bad.x", "1.2.3 ||"] {
            assert!(
                parser.parse_full_expression(expression.to_owned()).is_err(),
                "{expression}"
            );
        }
    }
}
