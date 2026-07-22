mod support;

use dealer::hash::{Sha512, to_base64};
use dealer::package::{HashAlgorithm, TarballLocation};
use dealer::registry::RegistryClient;
use dealer::store::PackageStore;
use dealer::tarball::TarballCache;
use serde_json::json;
use support::TarballBuilder;
use support::registry::{PackumentBuilder, Route, TestRegistry, metadata_path, tarball_path};

fn fixture_tarball(name: &str, version: &str) -> Vec<u8> {
    TarballBuilder::new()
        .file("package.json", &support::manifest(name, version))
        .file("index.js", "module.exports = 1;\n")
        .build()
}

fn integrity_of(tarball: &[u8]) -> String {
    format!("sha512-{}", to_base64(&Sha512::digest(tarball)))
}

#[test]
fn resolves_a_range_to_the_highest_matching_version() {
    let registry = TestRegistry::start();
    let packument = PackumentBuilder::new(registry.base_url(), "left-pad")
        .version("1.0.0", &[])
        .version("1.4.2", &[])
        .version("2.0.0", &[])
        .build();
    registry.serve(&metadata_path("left-pad"), Route::json(&packument));

    let client = RegistryClient::with_base_url(registry.base_url());
    let resolved = client
        .packument("left-pad")
        .expect("metadata should be served")
        .resolve("^1.0.0")
        .expect("a version should satisfy the range");

    assert_eq!(resolved.version, "1.4.2");
    assert_eq!(
        resolved.tarball,
        TarballLocation::Remote(format!(
            "{}{}",
            registry.base_url(),
            tarball_path("left-pad", "1.4.2")
        ))
    );
}

#[test]
fn resolves_dist_tags_and_wildcards() {
    let registry = TestRegistry::start();
    let packument = PackumentBuilder::new(registry.base_url(), "left-pad")
        .version("1.0.0", &[])
        .version("2.0.0", &[])
        .tag("latest", "1.0.0")
        .tag("next", "2.0.0")
        .build();
    registry.serve(&metadata_path("left-pad"), Route::json(&packument));

    let client = RegistryClient::with_base_url(registry.base_url());
    let packument = client.packument("left-pad").expect("metadata should be served");

    assert_eq!(packument.resolve("latest").unwrap().version, "1.0.0");
    assert_eq!(packument.resolve("next").unwrap().version, "2.0.0");
    assert_eq!(packument.resolve("*").unwrap().version, "2.0.0");
}

#[test]
fn keeps_scoped_names_in_a_single_path_segment() {
    let registry = TestRegistry::start();
    let packument = PackumentBuilder::new(registry.base_url(), "@dealer/tools")
        .version("1.0.0", &[])
        .build();
    registry.serve(&metadata_path("@dealer/tools"), Route::json(&packument));

    let client = RegistryClient::with_base_url(registry.base_url());
    client
        .packument("@dealer/tools")
        .expect("scoped metadata should be served");

    assert_eq!(registry.requests(), vec!["/@dealer%2Ftools".to_string()]);
}

#[test]
fn reads_dependencies_and_integrity_from_the_packument() {
    let registry = TestRegistry::start();
    let tarball = fixture_tarball("left-pad", "1.0.0");
    let packument = PackumentBuilder::new(registry.base_url(), "left-pad")
        .version("1.0.0", &[("ansi", "^2.0.0")])
        .with_dist_field("1.0.0", "integrity", json!(integrity_of(&tarball)))
        .with_bin("1.0.0", json!("cli.js"))
        .build();
    registry.serve(&metadata_path("left-pad"), Route::json(&packument));

    let client = RegistryClient::with_base_url(registry.base_url());
    let resolved = client
        .packument("left-pad")
        .unwrap()
        .resolve("1.0.0")
        .expect("the exact version should resolve");

    assert_eq!(resolved.dependencies.get("ansi").map(String::as_str), Some("^2.0.0"));
    assert_eq!(resolved.bin.get("left-pad").map(String::as_str), Some("cli.js"));
    assert_eq!(resolved.integrity.unwrap().to_string(), integrity_of(&tarball));
}

#[test]
fn falls_back_to_the_legacy_shasum() {
    let registry = TestRegistry::start();
    let packument = PackumentBuilder::new(registry.base_url(), "left-pad")
        .version("1.0.0", &[])
        .with_dist_field(
            "1.0.0",
            "shasum",
            json!("da39a3ee5e6b4b0d3255bfef95601890afd80709"),
        )
        .build();
    registry.serve(&metadata_path("left-pad"), Route::json(&packument));

    let client = RegistryClient::with_base_url(registry.base_url());
    let resolved = client.packument("left-pad").unwrap().resolve("1.0.0").unwrap();
    let integrity = resolved.integrity.expect("the shasum should be read");

    assert_eq!(integrity.algorithm(), HashAlgorithm::Sha1);
    assert_eq!(integrity.to_hex(), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
}

#[test]
fn reports_a_package_the_registry_does_not_have() {
    let registry = TestRegistry::start();
    let client = RegistryClient::with_base_url(registry.base_url());

    let failure = client
        .packument("nonexistent")
        .expect_err("a missing package should be an error");

    assert!(
        failure.to_string().contains("was not found in"),
        "{failure}"
    );
}

#[test]
fn reports_server_side_failures_with_the_status() {
    let registry = TestRegistry::start();
    registry.serve(&metadata_path("flaky"), Route::status(503));

    let client = RegistryClient::with_base_url(registry.base_url());
    let failure = client.packument("flaky").expect_err("503 should be an error");

    assert!(failure.to_string().contains("503"), "{failure}");
    assert!(failure.to_string().contains("may be down"), "{failure}");
}

#[test]
fn reports_a_range_that_nothing_satisfies() {
    let registry = TestRegistry::start();
    let packument = PackumentBuilder::new(registry.base_url(), "left-pad")
        .version("1.0.0", &[])
        .version("1.1.0", &[])
        .build();
    registry.serve(&metadata_path("left-pad"), Route::json(&packument));

    let client = RegistryClient::with_base_url(registry.base_url());
    let failure = client
        .packument("left-pad")
        .unwrap()
        .resolve("^9.0.0")
        .expect_err("no version satisfies the range");

    assert!(failure.to_string().contains("no published version"), "{failure}");
    assert!(failure.to_string().contains("1.1.0"), "{failure}");
}

#[test]
fn fetches_each_packument_once() {
    let registry = TestRegistry::start();
    let packument = PackumentBuilder::new(registry.base_url(), "left-pad")
        .version("1.0.0", &[])
        .build();
    registry.serve(&metadata_path("left-pad"), Route::json(&packument));

    let client = RegistryClient::with_base_url(registry.base_url());
    for _ in 0..3 {
        client.packument("left-pad").expect("metadata should be served");
    }

    assert_eq!(registry.request_count("/left-pad"), 1);
}

#[test]
fn downloads_a_tarball_once_and_reuses_the_cache() {
    let registry = TestRegistry::start();
    let tarball = fixture_tarball("left-pad", "1.0.0");
    let packument = PackumentBuilder::new(registry.base_url(), "left-pad")
        .version("1.0.0", &[])
        .with_dist_field("1.0.0", "integrity", json!(integrity_of(&tarball)))
        .build();
    registry.publish("left-pad", "1.0.0", &packument, tarball.clone());

    let cache_root = tempfile::tempdir().expect("cache directory should be created");
    let client = RegistryClient::with_base_url(registry.base_url());
    let cache = TarballCache::new(cache_root.path().to_path_buf(), &client);
    let resolved = client.packument("left-pad").unwrap().resolve("1.0.0").unwrap();

    let first = cache.acquire(&resolved).expect("the tarball should download");
    let second = cache.acquire(&resolved).expect("the cache should answer");

    assert!(!first.reused, "the first acquisition downloads");
    assert!(second.reused, "the second acquisition is served from the cache");
    assert_eq!(first.bytes, tarball);
    assert_eq!(first.digest, second.digest);
    assert_eq!(registry.request_count(&tarball_path("left-pad", "1.0.0")), 1);
}

#[test]
fn rejects_a_tarball_that_fails_its_checksum() {
    let registry = TestRegistry::start();
    let tarball = fixture_tarball("left-pad", "1.0.0");
    let packument = PackumentBuilder::new(registry.base_url(), "left-pad")
        .version("1.0.0", &[])
        .with_dist_field("1.0.0", "integrity", json!(integrity_of(b"different bytes")))
        .build();
    registry.publish("left-pad", "1.0.0", &packument, tarball);

    let cache_root = tempfile::tempdir().expect("cache directory should be created");
    let client = RegistryClient::with_base_url(registry.base_url());
    let cache = TarballCache::new(cache_root.path().to_path_buf(), &client);
    let resolved = client.packument("left-pad").unwrap().resolve("1.0.0").unwrap();

    let failure = cache
        .acquire(&resolved)
        .expect_err("a mismatched checksum should be rejected");

    assert!(failure.to_string().contains("integrity check failed"), "{failure}");
}

#[test]
fn the_store_extracts_once_and_reuses_the_entry() {
    let root = tempfile::tempdir().expect("store directory should be created");
    let store = PackageStore::new(root.path().to_path_buf());
    let tarball = fixture_tarball("left-pad", "1.0.0");

    assert!(!store.contains("left-pad@1.0.0-abcdef"));

    let first = store.insert("left-pad@1.0.0-abcdef", &tarball).unwrap();
    let second = store.insert("left-pad@1.0.0-abcdef", &tarball).unwrap();

    assert!(!first.reused);
    assert!(second.reused);
    assert_eq!(first.path, second.path);
    assert!(store.contains("left-pad@1.0.0-abcdef"));
    assert_eq!(
        std::fs::read_to_string(first.path.join("index.js")).unwrap(),
        "module.exports = 1;\n"
    );
}

/// An entry left behind by an interrupted install has no ready marker and must
/// be replaced rather than trusted.
#[test]
fn the_store_replaces_an_incomplete_entry() {
    let root = tempfile::tempdir().expect("store directory should be created");
    let store = PackageStore::new(root.path().to_path_buf());
    let leftover = store.package_path("left-pad@1.0.0-abcdef");

    std::fs::create_dir_all(&leftover).unwrap();
    std::fs::write(leftover.join("stale.js"), "// interrupted").unwrap();

    let entry = store
        .insert("left-pad@1.0.0-abcdef", &fixture_tarball("left-pad", "1.0.0"))
        .unwrap();

    assert!(!entry.reused);
    assert!(!entry.path.join("stale.js").exists());
    assert!(entry.path.join("index.js").exists());
}
