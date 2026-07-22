mod support;

use std::fs;
use std::path::PathBuf;

use dealer::package::PackageSource;
use dealer::resolver::{InstallPlan, Resolver};
use serde_json::json;
use support::TarballBuilder;
use support::provider::FakeProvider;
use tempfile::TempDir;

fn requests(specifiers: &[&str]) -> Vec<PackageSource> {
    specifiers
        .iter()
        .map(|specifier| {
            PackageSource::parse_specifier(specifier)
                .unwrap_or_else(|error| panic!("`{specifier}` should parse: {error}"))
        })
        .collect()
}

fn resolve(provider: &FakeProvider, specifiers: &[&str]) -> InstallPlan {
    Resolver::new(provider)
        .resolve(&requests(specifiers))
        .expect("resolution should succeed")
}

fn rejection(provider: &FakeProvider, specifiers: &[&str]) -> String {
    Resolver::new(provider)
        .resolve(&requests(specifiers))
        .expect_err("resolution should fail")
        .to_string()
}

/// Writes a local `.tgz` whose manifest declares the given dependencies.
fn local_tarball(name: &str, version: &str, dependencies: serde_json::Value) -> (TempDir, PathBuf) {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let path = directory.path().join(format!("{name}-{version}.tgz"));
    let tarball = TarballBuilder::new()
        .file(
            "package.json",
            &support::manifest_with(name, version, json!({ "dependencies": dependencies })),
        )
        .file("index.js", "// entry\n")
        .build();

    fs::write(&path, tarball).expect("the fixture tarball should be written");

    (directory, path)
}

#[test]
fn resolves_direct_and_transitive_dependencies() {
    let provider = FakeProvider::new()
        .publish("app-core", "1.2.0", &[("logger", "^2.0.0")])
        .publish("logger", "2.1.0", &[("colors", "^1.0.0")])
        .publish("colors", "1.4.0", &[]);

    let plan = resolve(&provider, &["app-core@^1.0.0"]);

    assert_eq!(
        plan.identifiers(),
        vec!["app-core@1.2.0", "colors@1.4.0", "logger@2.1.0"]
    );
    assert_eq!(plan.roots().collect::<Vec<_>>(), vec!["app-core"]);
}

#[test]
fn picks_the_highest_version_that_satisfies_every_requirement() {
    let provider = FakeProvider::new()
        .publish("left", "1.0.0", &[("shared", "^1.0.0")])
        .publish("right", "1.0.0", &[("shared", ">=1.2.0")])
        .publish("shared", "1.0.0", &[])
        .publish("shared", "1.3.0", &[])
        .publish("shared", "1.9.0", &[])
        .publish("shared", "2.0.0", &[]);

    let plan = resolve(&provider, &["left@1.0.0", "right@1.0.0"]);

    assert_eq!(
        plan.get("shared").map(|package| package.version.as_str()),
        Some("1.9.0"),
        "2.0.0 is excluded by ^1.0.0 and 1.0.0 by >=1.2.0"
    );
}

/// A requirement discovered after a version was already chosen has to narrow
/// that choice rather than be ignored.
#[test]
fn revisits_a_choice_when_a_later_requirement_is_stricter() {
    let provider = FakeProvider::new()
        .publish("shared", "1.0.0", &[])
        .publish("shared", "2.0.0", &[])
        .publish("strict", "1.0.0", &[("shared", "^1.0.0")]);

    let plan = resolve(&provider, &["shared@*", "strict@1.0.0"]);

    assert_eq!(
        plan.get("shared").map(|package| package.version.as_str()),
        Some("1.0.0")
    );
}

#[test]
fn resolves_dist_tags() {
    let provider = FakeProvider::new()
        .publish("tagged", "1.0.0", &[])
        .publish("tagged", "2.0.0-experimental", &[])
        .tag("tagged", "next", "2.0.0-experimental");

    let plan = resolve(&provider, &["tagged@next"]);

    assert_eq!(plan.identifiers(), vec!["tagged@2.0.0-experimental"]);
}

#[test]
fn a_dependency_cycle_terminates() {
    let provider = FakeProvider::new()
        .publish("ping", "1.0.0", &[("pong", "^1.0.0")])
        .publish("pong", "1.0.0", &[("ping", "^1.0.0")]);

    let plan = resolve(&provider, &["ping@^1.0.0"]);

    assert_eq!(plan.identifiers(), vec!["ping@1.0.0", "pong@1.0.0"]);
}

#[test]
fn a_self_referential_package_terminates() {
    let provider = FakeProvider::new().publish("ouroboros", "1.0.0", &[("ouroboros", "^1.0.0")]);

    assert_eq!(
        resolve(&provider, &["ouroboros@1.0.0"]).identifiers(),
        vec!["ouroboros@1.0.0"]
    );
}

#[test]
fn reports_a_conflict_with_every_requirement_that_caused_it() {
    let provider = FakeProvider::new()
        .publish("old", "1.0.0", &[("shared", "^1.0.0")])
        .publish("new", "1.0.0", &[("shared", "^2.0.0")])
        .publish("shared", "1.0.0", &[])
        .publish("shared", "2.0.0", &[]);

    let failure = rejection(&provider, &["old@1.0.0", "new@1.0.0"]);

    assert!(
        failure.contains("cannot satisfy every requirement for `shared`"),
        "{failure}"
    );
    assert!(
        failure.contains("^1.0.0 is required by old@1.0.0"),
        "{failure}"
    );
    assert!(
        failure.contains("^2.0.0 is required by new@1.0.0"),
        "{failure}"
    );
}

#[test]
fn reports_a_root_range_nothing_satisfies() {
    let provider = FakeProvider::new().publish("shared", "1.0.0", &[]);

    let failure = rejection(&provider, &["shared@^9.0.0"]);

    assert!(
        failure.contains("^9.0.0 is required by the project"),
        "{failure}"
    );
}

/// The plan must not depend on the order the roots were given in, so repeated
/// installs of the same manifest agree.
#[test]
fn the_plan_is_deterministic() {
    let provider = FakeProvider::new()
        .publish("alpha", "1.0.0", &[("shared", "^1.0.0")])
        .publish("beta", "1.0.0", &[("shared", "^1.0.0")])
        .publish("shared", "1.4.0", &[]);

    let forwards = resolve(&provider, &["alpha@1.0.0", "beta@1.0.0"]);
    let backwards = resolve(&provider, &["beta@1.0.0", "alpha@1.0.0"]);

    assert_eq!(forwards, backwards);
    assert_eq!(
        forwards.identifiers(),
        vec!["alpha@1.0.0", "beta@1.0.0", "shared@1.4.0"]
    );
}

#[test]
fn each_package_is_looked_up_once_per_resolution() {
    let provider = FakeProvider::new()
        .publish("alpha", "1.0.0", &[("shared", "^1.0.0")])
        .publish("beta", "1.0.0", &[("shared", "^1.0.0")])
        .publish("shared", "1.4.0", &[]);

    resolve(&provider, &["alpha@1.0.0", "beta@1.0.0"]);

    assert_eq!(
        provider.lookups(),
        vec!["alpha", "beta", "shared"],
        "a package whose choice still holds is not re-examined"
    );
}

#[test]
fn resolves_a_local_tarball_and_its_registry_dependencies() {
    let provider = FakeProvider::new().publish("colors", "1.4.0", &[]);
    let (_directory, path) = local_tarball("fixture", "1.0.0", json!({ "colors": "^1.0.0" }));

    let plan = Resolver::new(&provider)
        .resolve(&[PackageSource::parse_specifier(&path.to_string_lossy()).unwrap()])
        .expect("a local tarball should resolve");

    assert_eq!(plan.identifiers(), vec!["colors@1.4.0", "fixture@1.0.0"]);
    assert_eq!(plan.roots().collect::<Vec<_>>(), vec!["fixture"]);
    assert!(
        plan.get("fixture").unwrap().integrity.is_some(),
        "a local tarball is its own checksum"
    );
}

#[test]
fn reports_a_local_tarball_that_no_range_accepts() {
    let provider = FakeProvider::new().publish("needs-fixture", "1.0.0", &[("fixture", "^2.0.0")]);
    let (_directory, path) = local_tarball("fixture", "1.0.0", json!({}));

    let failure = Resolver::new(&provider)
        .resolve(
            &requests(&["needs-fixture@1.0.0"])
                .into_iter()
                .chain([PackageSource::parse_specifier(&path.to_string_lossy()).unwrap()])
                .collect::<Vec<_>>(),
        )
        .expect_err("the pinned version does not satisfy the range")
        .to_string();

    assert!(
        failure.contains("a local tarball pins `fixture` to 1.0.0"),
        "{failure}"
    );
    assert!(
        failure.contains("^2.0.0 is required by needs-fixture@1.0.0"),
        "{failure}"
    );
}
