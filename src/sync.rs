use std::{env::temp_dir, path::Path};

use grammar::process_grammars;
use rust::process_rust_extension;
use smol::{fs, process::Command};

use crate::{
    manifest::ExtensionManifest,
    output::{Extension, ExtensionKind, Grammar, Source},
    registry::RegistryExtension,
};

mod grammar;
mod rust;

pub async fn process_extension(
    extension: RegistryExtension,
) -> anyhow::Result<Option<(Extension, Vec<Grammar>)>> {
    tracing::info!("Synching extension");

    let name = extension.name.clone();
    let repo = extension.repository.clone();

    let tmp_repo = temp_dir().join(&name);
    checkout_git_repo(&repo, &extension.rev, &tmp_repo).await?;

    let extension_dir = if let Some(path) = &extension.path {
        tmp_repo.join(path)
    } else {
        tmp_repo.clone()
    };

    let manifest = extension_dir.join("extension.toml");
    if !manifest.exists() {
        tracing::error!("Missing extension.toml");
        fs::remove_dir_all(&tmp_repo).await?;
        return Ok(None);
    }

    tracing::debug!("Reading extension manifest");
    let manifest = fs::read_to_string(manifest).await?;
    let manifest: ExtensionManifest = toml::from_str(&manifest)?;

    let src = prefetch_git_repo(&repo, &extension.rev, false).await?;
    let grammars = process_grammars(manifest.grammars, &name).await?;

    let (kind, root) = if extension_dir.join("Cargo.toml").exists() {
        process_rust_extension(&extension, &extension_dir, &name).await?
    } else {
        (ExtensionKind::Plain, extension.path.clone())
    };

    fs::remove_dir_all(&tmp_repo).await?;

    Ok(Some((
        Extension {
            name,
            version: manifest.version,
            src,
            root,
            grammars: grammars.ids,
            kind,
        },
        grammars.grammars,
    )))
}

async fn checkout_git_repo(repo: &str, rev: &str, dest: &Path) -> anyhow::Result<()> {
    tracing::info!("Checking out repository");

    if dest.exists() {
        fs::remove_dir_all(dest).await?;
    }

    tracing::info!("Cloning repository");
    let clone = Command::new("git")
        .args(["clone", repo, &dest.to_string_lossy()])
        .output()
        .await?;

    if !clone.status.success() {
        anyhow::bail!("Failed to clone repository");
    }

    tracing::info!("Fetching revision");
    let fetch = Command::new("git")
        .args(["fetch", "origin", rev])
        .current_dir(dest)
        .output()
        .await?;

    if !fetch.status.success() {
        anyhow::bail!("Failed to fetch revision");
    }

    tracing::info!("Checking out revision");
    let checkout = Command::new("git")
        .args(["checkout", rev])
        .current_dir(dest)
        .output()
        .await?;

    if !checkout.status.success() {
        anyhow::bail!("Failed to checkout revision");
    }

    Ok(())
}

async fn prefetch_git_repo(
    repo: &str,
    rev: &str,
    fetch_submodules: bool,
) -> anyhow::Result<Source> {
    tracing::info!("Pre-fetching git source");

    let rev = if (rev.len() == 40 && rev.chars().all(|char| char.is_ascii_hexdigit()))
        || rev.starts_with("refs/")
    {
        rev.to_owned()
    } else {
        format!("refs/heads/{rev}")
    };

    let mut args = vec!["--url", repo, "--rev", &rev];
    if fetch_submodules {
        args.push("--fetch-submodules");
    }
    args.push("--quiet");

    let prefetch = Command::new("nix-prefetch-git")
        .args(&args)
        .output()
        .await?;

    if !prefetch.status.success() {
        anyhow::bail!("Failed to pre-fetch repository");
    }

    let src: Source = serde_json::from_slice(&prefetch.stdout)?;
    tracing::info!(src = ?src, "Pre-fetched git hash");

    Ok(src)
}
