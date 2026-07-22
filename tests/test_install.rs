//! The milestone scenario: create a project, install a package, check what
//! landed in node_modules, and run a script that uses it.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::{Value, json};
use support::TarballBuilder;
use support::registry::{PackumentBuilder, TestRegistry};
use tempfile::TempDir;

/// A temporary project with its own dealer home, so nothing touches a real
/// cache and no test can observe another's state.
struct Fixture {
    project: TempDir,
    home: TempDir,
    registry: String,
}

impl Fixture {
    /// Starts an empty project pointed at a registry that cannot answer, so an
    /// unintended network call fails loudly instead of reaching npm.
    fn new() -> Self {
        Self::with_registry("http://127.0.0.1:1")
    }

    fn with_registry(registry: &str) -> Self {
        Self {
            project: tempfile::tempdir().expect("project directory should be created"),
            home: tempfile::tempdir().expect("dealer home should be created"),
            registry: registry.to_string(),
        }
    }

    fn path(&self) -> &Path {
        self.project.path()
    }

    fn node_modules(&self) -> PathBuf {
        self.path().join("node_modules")
    }

    fn dealer(&self, arguments: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_dealer"))
            .current_dir(self.path())
            .env("DEALER_HOME", self.home.path())
            .env("DEALER_REGISTRY", &self.registry)
            .args(arguments)
            .output()
            .expect("dealer should start")
    }

    /// Runs dealer and fails the test with its output when it does not succeed.
    fn run(&self, arguments: &[&str]) -> String {
        let output = self.dealer(arguments);

        assert!(
            output.status.success(),
            "`dealer {}` failed\nstdout: {}\nstderr: {}",
            arguments.join(" "),
            text(&output.stdout),
            text(&output.stderr)
        );

        text(&output.stdout)
    }

    fn write(&self, path: &str, contents: &str) {
        let path = self.path().join(path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("parent directory should be created");
        }

        fs::write(path, contents).expect("fixture file should be written");
    }

    fn write_manifest(&self, manifest: Value) {
        self.write("package.json", &manifest.to_string());
    }

    fn read_manifest(&self) -> Value {
        serde_json::from_slice(&fs::read(self.path().join("package.json")).unwrap())
            .expect("the manifest should stay valid JSON")
    }

    /// Writes a fixture tarball into the project and returns its `./` path.
    fn add_tarball(&self, name: &str, version: &str, tarball: Vec<u8>) -> String {
        let file_name = format!("{name}-{version}.tgz");
        fs::write(self.path().join(&file_name), tarball).expect("tarball should be written");

        format!("./{file_name}")
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A package with a library entry point and a command line entry point.
fn greeter_tarball(version: &str, dependencies: Value) -> Vec<u8> {
    TarballBuilder::new()
        .file(
            "package.json",
            &support::manifest_with(
                "greeter",
                version,
                json!({
                    "main": "index.js",
                    "bin": { "greeter": "cli.js" },
                    "dependencies": dependencies,
                }),
            ),
        )
        .file(
            "index.js",
            &format!("module.exports = function () {{ return 'greeter {version}'; }};\n"),
        )
        .executable(
            "cli.js",
            "#!/usr/bin/env node\nconsole.log(require('./index.js')());\n",
        )
        .file("lib/nested/helper.js", "module.exports = 'helper';\n")
        .build()
}

fn plain_tarball(name: &str, version: &str, dependencies: Value) -> Vec<u8> {
    TarballBuilder::new()
        .file(
            "package.json",
            &support::manifest_with(name, version, json!({ "dependencies": dependencies })),
        )
        .file(
            "index.js",
            &format!("module.exports = '{name}@{version}';\n"),
        )
        .build()
}

/// The shim a platform actually executes.
fn shim_path(node_modules: &Path, name: &str) -> PathBuf {
    let shim = node_modules.join(".bin").join(name);

    if cfg!(windows) {
        shim.with_extension("cmd")
    } else {
        shim
    }
}

fn node_is_available() -> bool {
    Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

#[test]
fn installs_a_package_from_a_tarball_and_runs_a_script_that_uses_it() {
    let fixture = Fixture::new();

    // 1. Initialize a temporary JavaScript project.
    let created = fixture.run(&["init"]);
    assert!(created.contains("created package.json"), "{created}");

    let mut manifest = fixture.read_manifest();
    manifest["scripts"] = json!({
        "greet": "greeter",
        "echo": "echo ran the script",
    });
    fixture.write_manifest(manifest);

    // 2. Install a package from a tarball.
    let specifier = fixture.add_tarball("greeter", "1.0.0", greeter_tarball("1.0.0", json!({})));
    let installed = fixture.run(&["install", &specifier]);
    assert!(installed.contains("+ greeter@1.0.0"), "{installed}");

    // 3. Verify its files and .bin link.
    let node_modules = fixture.node_modules();
    let package = node_modules.join("greeter");
    assert_eq!(
        fs::read_to_string(package.join("index.js")).unwrap(),
        "module.exports = function () { return 'greeter 1.0.0'; };\n"
    );
    assert!(
        package.join("lib/nested/helper.js").is_file(),
        "nested files should survive extraction"
    );
    assert!(
        !package.join("package").exists(),
        "the tarball's root directory should be stripped"
    );

    let shim = shim_path(&node_modules, "greeter");
    assert!(shim.is_file(), "{} should exist", shim.display());

    // The shim a platform runs varies, but the POSIX one is always written and
    // spells the target the same way everywhere.
    let posix_shim = fs::read_to_string(node_modules.join(".bin/greeter")).unwrap();
    assert!(
        posix_shim.contains("greeter/cli.js"),
        "the shim should point at the package's bin script: {posix_shim}"
    );

    assert_eq!(
        fixture.read_manifest()["dependencies"]["greeter"],
        json!(specifier.replace("./", "file:./")),
        "the tarball should be recorded in the manifest"
    );

    // 4. Run a package script successfully.
    let echoed = fixture.run(&["run", "echo"]);
    assert!(echoed.contains("ran the script"), "{echoed}");

    if node_is_available() {
        let greeted = fixture.run(&["run", "greet"]);
        assert!(
            greeted.contains("greeter 1.0.0"),
            "the script should reach the shim through PATH: {greeted}"
        );
    }
}

#[test]
fn installs_transitive_dependencies_from_a_registry() {
    let registry = TestRegistry::start();
    registry.publish(
        "greeter",
        "1.2.0",
        &PackumentBuilder::new(registry.base_url(), "greeter")
            .version("1.2.0", &[("colors", "^1.0.0")])
            .with_bin("1.2.0", json!({ "greeter": "cli.js" }))
            .build(),
        greeter_tarball("1.2.0", json!({ "colors": "^1.0.0" })),
    );
    registry.publish(
        "colors",
        "1.4.1",
        &PackumentBuilder::new(registry.base_url(), "colors")
            .version("1.0.0", &[])
            .version("1.4.1", &[])
            .build(),
        plain_tarball("colors", "1.4.1", json!({})),
    );

    let fixture = Fixture::with_registry(registry.base_url());
    fixture.write_manifest(json!({
        "name": "fixture-project",
        "version": "1.0.0",
        "dependencies": { "greeter": "^1.0.0" },
    }));

    let installed = fixture.run(&["install"]);

    assert!(installed.contains("+ greeter@1.2.0"), "{installed}");
    assert!(
        installed.contains("+ colors@1.4.1"),
        "the transitive dependency should be installed: {installed}"
    );
    assert!(fixture.node_modules().join("greeter/index.js").is_file());
    assert!(fixture.node_modules().join("colors/index.js").is_file());
}

#[test]
fn scoped_packages_land_in_a_scope_directory() {
    let registry = TestRegistry::start();
    registry.publish(
        "@dealer/tools",
        "2.0.0",
        &PackumentBuilder::new(registry.base_url(), "@dealer/tools")
            .version("2.0.0", &[])
            .build(),
        plain_tarball("@dealer/tools", "2.0.0", json!({})),
    );

    let fixture = Fixture::with_registry(registry.base_url());
    fixture.write_manifest(json!({
        "name": "fixture-project",
        "version": "1.0.0",
        "dependencies": { "@dealer/tools": "^2.0.0" },
    }));

    fixture.run(&["install"]);

    assert!(
        fixture
            .node_modules()
            .join("@dealer/tools/index.js")
            .is_file(),
        "a scoped package should keep its scope directory"
    );
}

#[test]
fn repeated_installs_are_idempotent() {
    let fixture = Fixture::new();
    fixture.write_manifest(json!({ "name": "fixture-project", "version": "1.0.0" }));

    let specifier = fixture.add_tarball("greeter", "1.0.0", greeter_tarball("1.0.0", json!({})));
    fixture.run(&["install", &specifier]);

    let entry = fixture.node_modules().join("greeter/index.js");
    let first = fs::read(&entry).expect("the package should be installed");

    let second = fixture.run(&["install"]);

    assert!(second.contains("up to date"), "{second}");
    assert!(!second.contains("+ greeter"), "{second}");
    assert_eq!(fs::read(&entry).unwrap(), first);
}

#[test]
fn a_package_dropped_from_the_manifest_is_removed() {
    let fixture = Fixture::new();
    fixture.write_manifest(json!({ "name": "fixture-project", "version": "1.0.0" }));

    let specifier = fixture.add_tarball("greeter", "1.0.0", greeter_tarball("1.0.0", json!({})));
    fixture.run(&["install", &specifier]);
    assert!(fixture.node_modules().join("greeter").is_dir());

    fixture.write_manifest(json!({ "name": "fixture-project", "version": "1.0.0" }));
    let removed = fixture.run(&["install"]);

    assert!(removed.contains("- greeter"), "{removed}");
    assert!(!fixture.node_modules().join("greeter").exists());
    assert!(
        !shim_path(&fixture.node_modules(), "greeter").exists(),
        "the shim should go with the package"
    );
}

/// Anything dealer did not put in node_modules belongs to somebody else.
#[test]
fn directories_dealer_did_not_create_are_left_alone() {
    let fixture = Fixture::new();
    fixture.write_manifest(json!({ "name": "fixture-project", "version": "1.0.0" }));
    fixture.write("node_modules/greeter/index.js", "// installed by hand\n");

    let specifier = fixture.add_tarball("greeter", "1.0.0", greeter_tarball("1.0.0", json!({})));
    let output = fixture.dealer(&["install", &specifier]);

    assert!(output.status.success(), "stderr: {}", text(&output.stderr));
    assert!(
        text(&output.stderr).contains("dealer did not create it"),
        "stderr: {}",
        text(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(fixture.node_modules().join("greeter/index.js")).unwrap(),
        "// installed by hand\n"
    );
}

#[test]
fn run_forwards_arguments_written_after_a_double_dash() {
    let fixture = Fixture::new();
    fixture.write_manifest(json!({
        "name": "fixture-project",
        "version": "1.0.0",
        "scripts": { "echo": "echo" },
    }));

    let output = fixture.run(&["run", "echo", "--", "--flag", "plain value"]);

    assert!(output.contains("--flag"), "{output}");
    assert!(output.contains("plain value"), "{output}");
}

/// A script is a shell command line, so quotes written inside it have to reach
/// the shell exactly as the author wrote them.
#[test]
fn run_preserves_quoting_inside_a_script() {
    if !node_is_available() {
        return;
    }

    let fixture = Fixture::new();
    fixture.write_manifest(json!({
        "name": "fixture-project",
        "version": "1.0.0",
        "scripts": { "quoted": "node -e \"console.log('quoted output')\"" },
    }));

    let output = fixture.run(&["run", "quoted"]);

    assert!(output.contains("quoted output"), "{output}");
}

#[test]
fn run_reports_the_scripts_exit_code() {
    let fixture = Fixture::new();
    fixture.write_manifest(json!({
        "name": "fixture-project",
        "version": "1.0.0",
        "scripts": { "fail": "exit 23" },
    }));

    let output = fixture.dealer(&["run", "fail"]);

    assert_eq!(
        output.status.code(),
        Some(23),
        "stderr: {}",
        text(&output.stderr)
    );
    assert!(text(&output.stderr).contains("exited with code 23"));
}

#[test]
fn install_reports_a_project_with_no_manifest() {
    let fixture = Fixture::new();
    let output = fixture.dealer(&["install"]);

    assert!(!output.status.success());
    assert!(
        text(&output.stderr).contains("run `dealer init`"),
        "stderr: {}",
        text(&output.stderr)
    );
}

#[test]
fn install_reports_a_registry_that_does_not_have_the_package() {
    let registry = TestRegistry::start();
    let fixture = Fixture::with_registry(registry.base_url());
    fixture.write_manifest(json!({
        "name": "fixture-project",
        "version": "1.0.0",
        "dependencies": { "missing": "^1.0.0" },
    }));

    let output = fixture.dealer(&["install"]);

    assert!(!output.status.success());
    assert!(
        text(&output.stderr).contains("`missing` was not found"),
        "stderr: {}",
        text(&output.stderr)
    );
}
