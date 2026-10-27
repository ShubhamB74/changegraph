//! Small helpers for building throwaway Git repositories.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

pub fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .context("failed to run `git`; is it installed?")?;
    if !out.status.success() {
        bail!(
            "git {} failed in {}: {}",
            args.join(" "),
            dir.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn init_repo(dir: &Path) -> Result<()> {
    git(dir, &["init", "-q"])?;
    git(dir, &["symbolic-ref", "HEAD", "refs/heads/main"])?;
    git(dir, &["config", "user.email", "eval@example.com"])?;
    git(dir, &["config", "user.name", "Eval"])?;
    git(dir, &["config", "commit.gpgsign", "false"])?;
    Ok(())
}

pub fn commit_all(dir: &Path, message: &str) -> Result<String> {
    git(dir, &["add", "-A"])?;
    git(dir, &["commit", "-q", "--allow-empty", "-m", message])?;
    git(dir, &["rev-parse", "HEAD"])
}

pub fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst).with_context(|| format!("creating {}", dst.display()))?;
    for entry in fs::read_dir(src).with_context(|| format!("reading {}", src.display()))? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

pub fn write_file(root: &Path, rel: &str, content: &str) -> Result<()> {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, content).with_context(|| format!("writing {}", path.display()))
}
