use super::version::Version;

#[derive(Debug)]
pub(super) enum ComparatorOperand {
    Greater,
    Less,
    Equals,
    GreaterEq,
    LessEq,
}

pub(super) struct Range {
    alternatives: Vec<ComparisonSet>,
}

pub(super) struct ComparisonSet {
    comparisons: Vec<Comparison>,
}

pub(super) struct Comparison {
    operand: ComparatorOperand,
    version: Version,
}

impl Range {
    pub(super) fn new(alternatives: Vec<ComparisonSet>) -> Self {
        Self { alternatives }
    }

    pub(super) fn matches(&self, version: &str) -> bool {
        let Some(version) = Version::parse(version) else {
            return false;
        };

        self.alternatives.iter().any(|set| {
            set.comparisons
                .iter()
                .all(|comparison| comparison.matches(version))
        })
    }
}

impl ComparisonSet {
    pub(super) fn new(comparisons: Vec<Comparison>) -> Self {
        Self { comparisons }
    }
}

impl Comparison {
    pub(super) fn new(operand: ComparatorOperand, version: Version) -> Self {
        Self { operand, version }
    }

    fn matches(&self, version: Version) -> bool {
        match self.operand {
            ComparatorOperand::Equals => version == self.version,
            ComparatorOperand::Greater => version > self.version,
            ComparatorOperand::GreaterEq => version >= self.version,
            ComparatorOperand::Less => version < self.version,
            ComparatorOperand::LessEq => version <= self.version,
        }
    }
}
