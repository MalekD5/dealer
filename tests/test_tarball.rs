mod support;

use std::fs;
use std::path::Path;

use dealer::tarball::{extract_package, read_manifest};
use support::{CHARACTER_DEVICE, REGULAR_FILE, TarballBuilder, corrupt_checksum, gzip};
use tempfile::TempDir;

fn extract(
    tarball: &[u8],
) -> (
    TempDir,
    Result<(), Box<dyn std::error::Error + Send + Sync>>,
) {
    let destination = tempfile::tempdir().expect("temporary directory should be created");
    let outcome = extract_package(tarball, destination.path());

    (destination, outcome)
}

fn extract_expecting_success(tarball: &[u8]) -> TempDir {
    let (destination, outcome) = extract(tarball);
    outcome.expect("archive should extract");

    destination
}

fn rejection(tarball: &[u8]) -> String {
    let (_destination, outcome) = extract(tarball);

    outcome.expect_err("archive should be rejected").to_string()
}

fn read(root: &Path, path: &str) -> String {
    fs::read_to_string(root.join(path)).unwrap_or_else(|_| panic!("{path} should exist"))
}

#[test]
fn extracts_a_package_and_strips_the_root_directory() {
    let tarball = TarballBuilder::new()
        .file("package.json", &support::manifest("fixture", "1.0.0"))
        .file("index.js", "module.exports = 1;\n")
        .directory("lib")
        .file("lib/nested/deep.js", "// deep\n")
        .build();

    let destination = extract_expecting_success(&tarball);
    let root = destination.path();

    assert!(
        !root.join("package").exists(),
        "the npm root should be stripped"
    );
    assert_eq!(read(root, "index.js"), "module.exports = 1;\n");
    assert_eq!(read(root, "lib/nested/deep.js"), "// deep\n");
    assert!(root.join("lib").is_dir());
}

#[test]
fn reads_the_manifest_without_touching_the_disk() {
    let tarball = TarballBuilder::new()
        .file("index.js", "// entry\n")
        .file("package.json", &support::manifest("fixture", "2.3.4"))
        .build();

    let manifest = read_manifest(&tarball).expect("manifest should be readable");

    assert_eq!(manifest["name"], "fixture");
    assert_eq!(manifest["version"], "2.3.4");
}

#[test]
fn reports_a_tarball_with_no_manifest() {
    let tarball = TarballBuilder::new().file("index.js", "// entry\n").build();

    let failure = read_manifest(&tarball).expect_err("a manifest is required");

    assert!(
        failure
            .to_string()
            .contains("does not contain a package.json"),
        "{failure}"
    );
}

#[test]
fn supports_gnu_long_names_and_pax_path_overrides() {
    let long_path = format!("lib/{}/module.js", "nested/".repeat(20));
    let tarball = TarballBuilder::new()
        .long_named_file(&long_path, "// long\n")
        .pax_named_file("lib/pax-named.js", "// pax\n")
        .build();

    let destination = extract_expecting_success(&tarball);
    let root = destination.path();

    assert_eq!(read(root, &long_path), "// long\n");
    assert_eq!(read(root, "lib/pax-named.js"), "// pax\n");
    assert!(
        !root.join("placeholder").exists(),
        "the pax override should replace the header name"
    );
}

#[test]
fn rejects_paths_that_climb_out_of_the_package() {
    let tarball = TarballBuilder::new()
        .raw("package/../../escaped.js", "// escaped\n", REGULAR_FILE)
        .build();

    assert!(rejection(&tarball).contains("escapes its directory"));
}

#[test]
fn rejects_absolute_paths() {
    let tarball = TarballBuilder::new()
        .raw("/etc/dealer-owned", "// escaped\n", REGULAR_FILE)
        .build();

    assert!(rejection(&tarball).contains("absolute path"));
}

/// A backslash is an ordinary character to tar but a separator on Windows.
#[test]
fn rejects_backslash_traversal() {
    let tarball = TarballBuilder::new()
        .raw("package\\..\\..\\escaped.js", "// escaped\n", REGULAR_FILE)
        .build();

    assert!(rejection(&tarball).contains("escapes its directory"));
}

#[test]
fn rejects_windows_drive_paths() {
    let tarball = TarballBuilder::new()
        .raw("C:/Windows/dealer-owned", "// escaped\n", REGULAR_FILE)
        .build();

    assert!(rejection(&tarball).contains("Windows drive"));
}

#[test]
fn rejects_links_that_point_outside_the_package() {
    let tarball = TarballBuilder::new()
        .file("package.json", &support::manifest("fixture", "1.0.0"))
        .symlink("escape.js", "../../../secrets.txt")
        .build();

    assert!(rejection(&tarball).contains("escapes the package"));
}

#[test]
fn rejects_links_with_absolute_targets() {
    let tarball = TarballBuilder::new()
        .symlink("escape.js", "/etc/passwd")
        .build();

    assert!(rejection(&tarball).contains("escapes the package"));
}

#[test]
fn rejects_links_to_files_the_package_does_not_contain() {
    let tarball = TarballBuilder::new()
        .symlink("dangling.js", "missing.js")
        .build();

    assert!(rejection(&tarball).contains("does not contain"));
}

#[test]
fn materialises_links_that_stay_inside_the_package() {
    let tarball = TarballBuilder::new()
        .file("index.js", "// entry\n")
        .file("lib/real.js", "// real\n")
        .symlink("alias.js", "index.js")
        .symlink("lib/sibling.js", "real.js")
        .hard_link("copy.js", "package/index.js")
        .build();

    let destination = extract_expecting_success(&tarball);
    let root = destination.path();

    assert_eq!(read(root, "alias.js"), "// entry\n");
    assert_eq!(read(root, "lib/sibling.js"), "// real\n");
    assert_eq!(read(root, "copy.js"), "// entry\n");
}

#[test]
fn rejects_entries_that_are_not_files_directories_or_links() {
    let tarball = TarballBuilder::new()
        .raw("package/console", "", CHARACTER_DEVICE)
        .build();

    assert!(rejection(&tarball).contains("may only contain files"));
}

#[test]
fn rejects_a_corrupt_header_checksum() {
    let tar = TarballBuilder::new()
        .file("index.js", "// entry\n")
        .into_tar();

    assert!(rejection(&gzip(&corrupt_checksum(tar))).contains("checksum"));
}

#[test]
fn rejects_a_truncated_archive() {
    let mut tar = TarballBuilder::new()
        .file("index.js", &"x".repeat(2000))
        .into_tar();
    tar.truncate(700);

    assert!(rejection(&gzip(&tar)).contains("ends in the middle"));
}

#[test]
fn rejects_data_that_is_not_a_gzip_stream() {
    assert!(rejection(b"this is not a tarball").contains("decompress"));
}

#[cfg(unix)]
#[test]
fn keeps_the_executable_bit_on_program_files() {
    use std::os::unix::fs::PermissionsExt;

    let tarball = TarballBuilder::new()
        .executable("cli.js", "#!/usr/bin/env node\n")
        .file("index.js", "// entry\n")
        .build();

    let destination = extract_expecting_success(&tarball);
    let mode = |path: &str| {
        fs::metadata(destination.path().join(path))
            .expect("file should exist")
            .permissions()
            .mode()
            & 0o111
    };

    assert_ne!(mode("cli.js"), 0, "cli.js should stay executable");
    assert_eq!(
        mode("index.js"),
        0,
        "plain files should not become executable"
    );
}

#[test]
fn extraction_is_repeatable() {
    let tarball = TarballBuilder::new()
        .file("package.json", &support::manifest("fixture", "1.0.0"))
        .file("index.js", "// entry\n")
        .build();

    let destination = extract_expecting_success(&tarball);
    extract_package(&tarball, destination.path()).expect("re-extraction should succeed");

    assert_eq!(read(destination.path(), "index.js"), "// entry\n");
}
