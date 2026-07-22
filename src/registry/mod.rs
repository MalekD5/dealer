//! npm registry metadata and tarball fetching.

pub mod client;
pub mod packument;

pub use client::RegistryClient;
pub use packument::Packument;
