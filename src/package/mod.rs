//! Models describing where a package comes from and what it resolves to.

pub mod integrity;
pub mod resolved;
pub mod source;

pub use integrity::{HashAlgorithm, Integrity};
pub use resolved::{ResolvedPackage, TarballLocation, read_bin, read_dependencies};
pub use source::PackageSource;
