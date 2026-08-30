use std::{
    collections::BTreeMap,
    env::temp_dir,
    num::NonZero,
    path::{Path, PathBuf},
    sync::Arc,
};

use cargo_lock::Lockfile;
use futures_util::stream::FuturesUnordered;
use serde_json::Value;
use smol::{fs, lock::Semaphore, process::Command, stream::StreamExt};
use tracing::Instrument;

use super::prefetch_git_repo;
use crate::{
    output::{CargoLock, ExtensionKind},
    registry::RegistryExtension,
};

pub struct CargoWorkspace {
    lockfile: PathBuf,
    root: Option<String>,
}

pub async fn process_rust_extension(
    extension: &RegistryExtension,
    dir: &Path,
    name: &str,
) -> anyhow::Result<(ExtensionKind, Option<String>)> {
    let workspace = find_cargo_workspace(dir, extension.path.as_deref()).await?;
    let has_stored_lockfile = process_cargo_lockfile(&workspace, dir, name).await?;

    let kind = calculate_rust_extension_kind(name, &workspace, has_stored_lockfile).await?;
    let root = calculate_rust_extension_root(&workspace, extension.path.as_deref());

    Ok((kind, root))
}

async fn find_cargo_workspace(dir: &Path, path: Option<&str>) -> anyhow::Result<CargoWorkspace> {
    let Some(path) = path else {
        let lockfile = dir.join("Cargo.lock");
        tracing::info!(
            lockfile = ?lockfile,
            root = ".",
            "Using extension lockfile"
        );

        return Ok(CargoWorkspace {
            lockfile,
            root: None,
        });
    };

    let metadata = Command::new("cargo")
        .args(["metadata", "--format-version=1", "--no-deps"])
        .current_dir(dir)
        .output()
        .await?;

    if metadata.status.success() {
        let metadata_json: Value = serde_json::from_slice(&metadata.stdout)?;
        if let Some(workspace_root) = metadata_json
            .get("workspace_root")
            .and_then(|value| value.as_str())
            .map(Path::new)
            && workspace_root != dir
        {
            let lockfile = workspace_root.join("Cargo.lock");
            tracing::info!(
                lockfile = ?lockfile,
                root = ".",
                "Using workspace lockfile"
            );

            return Ok(CargoWorkspace {
                lockfile,
                root: None,
            });
        }
    }

    let lockfile = dir.join("Cargo.lock");
    tracing::info!(
        lockfile = ?lockfile,
        root = ?path,
        "Using extension lockfile"
    );

    Ok(CargoWorkspace {
        lockfile,
        root: Some(path.to_owned()),
    })
}

async fn process_cargo_lockfile(
    workspace: &CargoWorkspace,
    dir: &Path,
    name: &str,
) -> anyhow::Result<bool> {
    let generated = Path::new("generated/extensions");
    if !generated.exists() {
        fs::create_dir_all(generated).await?;
    }

    let stored_lockfile = generated.join(format!("{name}.lock"));
    if !workspace.lockfile.exists() {
        tracing::info!("Generating Cargo.lock");

        let generate = Command::new("cargo")
            .args(["generate-lockfile"])
            .current_dir(dir)
            .output()
            .await?;

        if !generate.status.success() {
            tracing::error!("Failed to generate Cargo.lock");
            return Err(anyhow::anyhow!("Failed to generate Cargo.lock"));
        }

        fs::copy(&workspace.lockfile, &stored_lockfile).await?;
        return Ok(true);
    }

    if stored_lockfile.exists() {
        tracing::info!("Removing old stored lockfile");
        fs::remove_file(&stored_lockfile).await?;
    }

    Ok(false)
}

fn calculate_rust_extension_root(workspace: &CargoWorkspace, path: Option<&str>) -> Option<String> {
    let path = path?;
    let Some(cargo_root) = &workspace.root else {
        return Some(path.to_owned());
    };

    let relative = Path::new(path).strip_prefix(cargo_root).ok()?;
    let relative = relative.to_string_lossy().to_string();

    if relative.is_empty() {
        None
    } else {
        Some(relative)
    }
}

async fn calculate_rust_extension_kind(
    name: &str,
    workspace: &CargoWorkspace,
    has_stored_lockfile: bool,
) -> anyhow::Result<ExtensionKind> {
    let cargo_hash = generate_cargo_hash(name, &workspace.lockfile).await?;

    let cargo_lock = if has_stored_lockfile {
        let output_hashes = calculate_cargo_output_hashes(&workspace.lockfile).await?;
        Some(CargoLock {
            lock_file: PathBuf::from(format!("/generated/extensions/{name}.lock")),
            output_hashes,
        })
    } else {
        None
    };

    Ok(ExtensionKind::Rust {
        cargo_root: workspace.root.clone(),
        cargo_hash,
        cargo_lock,
    })
}

async fn generate_cargo_hash(name: &str, lockfile: &Path) -> anyhow::Result<String> {
    let tmp_vendor = temp_dir().join(format!("{name}_vendor"));
    if tmp_vendor.exists() {
        fs::remove_dir_all(&tmp_vendor).await?;
    }

    tracing::info!(
        lockfile = ?lockfile,
        vendor = ?tmp_vendor,
        "Running Cargo vendor"
    );

    let vendor = Command::new("fetch-cargo-vendor-util")
        .args([
            "create-vendor-staging",
            &lockfile.to_string_lossy(),
            &tmp_vendor.to_string_lossy(),
        ])
        .output()
        .await?;

    if !vendor.status.success() {
        anyhow::bail!("Failed to vendor cargo dependencies");
    }

    let hash = Command::new("nix-hash")
        .args(["--type", "sha256", "--sri", &tmp_vendor.to_string_lossy()])
        .output()
        .await?;

    if !hash.status.success() {
        anyhow::bail!("Failed to hash cargo dependencies");
    }

    let cargo_hash = String::from_utf8_lossy(&hash.stdout).trim().to_owned();
    tracing::info!(hash = ?cargo_hash, "Pre-fetched cargo hash");

    fs::remove_dir_all(&tmp_vendor).await?;
    Ok(cargo_hash)
}

async fn calculate_cargo_output_hashes(
    lockfile: &Path,
) -> anyhow::Result<BTreeMap<String, String>> {
    tracing::info!("Calculating output hashes for git dependencies");

    let lockfile_content = fs::read_to_string(lockfile).await?;
    let lockfile: Lockfile = toml::from_str(&lockfile_content)?;

    let mut output = BTreeMap::new();
    let mut futures = FuturesUnordered::new();

    let limit = std::thread::available_parallelism().map_or(1, NonZero::get);
    let semaphore = Arc::new(Semaphore::new(limit));

    for package in lockfile.packages {
        let Some(source) = &package.source else {
            continue;
        };

        if !source.is_git() {
            continue;
        }

        let name = &package.name;
        let version = &package.version;

        let url = source.url().to_string();
        let rev = if let Some(rev) = source.precise() {
            rev.to_owned()
        } else {
            tracing::warn!(
                package = ?name,
                url = url,
                "Git dependency missing precise revision"
            );

            continue;
        };

        tracing::debug!(
            package = %name,
            version = %version,
            url = %url,
            rev = %rev,
            "Found git dependency"
        );

        let key = format!("{name}-{version}");
        let semaphore = Arc::clone(&semaphore);

        let span = tracing::info_span!(
            "fetch_git_dependency",
            package = %name,
            version = %version,
            url = %url,
            rev = %rev,
        );

        let future = async move {
            let _permit = semaphore.acquire().await;
            let src = prefetch_git_repo(&url, &rev, true).instrument(span).await?;
            anyhow::Ok((key, src.hash))
        };

        futures.push(future);
    }

    if futures.is_empty() {
        tracing::info!("No git dependencies found");
        return Ok(output);
    }

    while let Some(result) = futures.next().await {
        match result {
            Ok((key, hash)) => {
                tracing::info!(key = key, hash = hash, "Calculated git dependency hash");
                output.insert(key, hash);
            },

            Err(err) => {
                tracing::error!(
                    err = ?err,
                    "Failed to calculate git dependency hash"
                );
            },
        }
    }

    Ok(output)
}
