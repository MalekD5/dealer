use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use clap::Args;

use super::cli::CommandResult;
use crate::error::{Result, error};
use crate::linker::{LinkReport, LinkRequest, Linker};
use crate::manifest::manifest::PackageManifest;
use crate::package::{PackageSource, ResolvedPackage};
use crate::paths::{cache_directory, store_directory};
use crate::registry::RegistryClient;
use crate::resolver::{InstallPlan, RegistryProvider, Resolver};
use crate::store::PackageStore;
use crate::tarball::TarballCache;
use crate::util::create_directory;

#[derive(Debug, Args)]
pub struct InstallArgs {
    /// Packages to add, such as `left-pad@^1.3.0` or `./fixture.tgz`.
    pub packages: Vec<String>,

    /// Install the packages without recording them in package.json.
    #[arg(long)]
    pub no_save: bool,

    /// Skip devDependencies.
    #[arg(long)]
    pub production: bool,
}

/// Reads the project manifest, resolves dependencies, acquires and extracts
/// tarballs, then links `node_modules`.
pub fn install(args: InstallArgs, project: &Path) -> CommandResult {
    let started = Instant::now();
    let mut manifest = PackageManifest::load(project)?;

    let requested = args
        .packages
        .iter()
        .map(|specifier| {
            PackageSource::parse_specifier(specifier)
                .map_err(|reason| error(format!("`{specifier}` is not installable: {reason}")))
        })
        .collect::<Result<Vec<_>>>()?;

    let mut sources = requested.clone();
    sources.extend(manifest.install_sources(!args.production)?);

    let client = RegistryClient::new();
    let cache = TarballCache::new(cache_directory()?, &client);
    let store = PackageStore::new(store_directory()?);
    create_directory(cache.root())?;
    create_directory(store.root())?;

    let plan = Resolver::new(&RegistryProvider::new(&client, &cache)).resolve(&sources)?;
    let report = materialize(&plan, &cache, &store, &manifest.node_modules())?;

    if !args.no_save && !requested.is_empty() {
        record_requested(&mut manifest, &requested, &plan)?;
    }

    summarize(&plan, &report, started);

    Ok(ExitCode::SUCCESS)
}

/// Acquires and extracts every planned package, then links it into place.
fn materialize(
    plan: &InstallPlan,
    cache: &TarballCache<'_>,
    store: &PackageStore,
    node_modules: &Path,
) -> Result<LinkReport> {
    let mut requests = Vec::with_capacity(plan.len());

    for package in plan.packages() {
        let artifact = cache.acquire(package)?;
        let store_key = package.store_key(&artifact.digest);
        let entry = store.insert(&store_key, &artifact.bytes)?;

        requests.push(LinkRequest {
            package,
            store_key,
            store_path: entry.path,
        });
    }

    Linker::new(node_modules.to_path_buf()).link(&requests)
}

/// Writes the versions that were actually installed back into the manifest.
fn record_requested(
    manifest: &mut PackageManifest,
    requested: &[PackageSource],
    plan: &InstallPlan,
) -> Result<()> {
    for source in requested {
        match source {
            PackageSource::Registry { name, range } => {
                let Some(package) = plan.get(name) else {
                    continue;
                };

                // An explicit range is what the developer asked for; anything
                // else is pinned to the version that was chosen.
                let specifier = if range == "*" {
                    format!("^{}", package.version)
                } else {
                    range.clone()
                };
                manifest.set_dependency(name, &specifier);
            }
            PackageSource::LocalTarball { path, .. } => {
                let Some(package) = plan.packages().find(|package| matches_local(package, path))
                else {
                    continue;
                };

                manifest.set_dependency(&package.name, &format!("file:{}", path.display()));
            }
        }
    }

    manifest.save()
}

fn matches_local(package: &ResolvedPackage, path: &Path) -> bool {
    match &package.tarball {
        crate::package::TarballLocation::Local(installed) => {
            installed == path || std::path::absolute(path).is_ok_and(|path| *installed == path)
        }
        _ => false,
    }
}

fn summarize(plan: &InstallPlan, report: &LinkReport, started: Instant) {
    for identifier in &report.linked {
        println!("+ {identifier}");
    }
    for name in &report.removed {
        println!("- {name}");
    }
    for name in &report.skipped {
        eprintln!("! left node_modules/{name} alone; dealer did not create it");
    }

    let elapsed = started.elapsed().as_secs_f64();
    if plan.is_empty() {
        println!("no dependencies to install");
        return;
    }

    if report.linked.is_empty() && report.removed.is_empty() {
        println!("up to date, {} in {elapsed:.1}s", packages(plan.len()));
        return;
    }

    println!(
        "installed {} of {} in {elapsed:.1}s",
        packages(report.linked.len()),
        packages(plan.len())
    );
}

fn packages(count: usize) -> String {
    if count == 1 {
        "1 package".to_string()
    } else {
        format!("{count} packages")
    }
}
