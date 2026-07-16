#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Eq, Ord)]
pub(crate) struct Version {
    pub(crate) major: u64,
    pub(crate) minor: u64,
    pub(crate) patch: u64,
}

impl Version {
    pub(crate) fn parse(version: &str) -> Option<Version> {
        let mut parts = version.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }

        Some(Version {
            major,
            minor,
            patch,
        })
    }
}
