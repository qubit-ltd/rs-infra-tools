// =============================================================================
//    Copyright (c) 2025 - 2026 Haixing Hu.
//
//    SPDX-License-Identifier: Apache-2.0
//
//    Licensed under the Apache License, Version 2.0.
// =============================================================================
//! Resolves and runs the latest main-branch infrastructure tools from a shared cache.

use std::env;
use std::fs;
use std::fs::File;
use std::fs::OpenOptions;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use fs2::FileExt;
use sha2::Digest;
use sha2::Sha256;

const TOOLS: &[Tool] = &[
    Tool::new("rs-infra-ci", "rs-infra-ci", "qubit-infra-ci"),
    Tool::new(
        "rs-infra-coverage",
        "rs-infra-coverage",
        "qubit-infra-coverage",
    ),
    Tool::new(
        "rs-infra-dependency",
        "rs-infra-dependency",
        "qubit-infra-dependency",
    ),
    Tool::new("rs-infra-pages", "rs-infra-pages", "qubit-infra-pages"),
    Tool::new("rs-infra-style", "rs-infra-style", "qubit-infra-style"),
    Tool::new("rs-infra-verify", "rs-infra-verify", "qubit-infra-verify"),
];
const MANAGER: Tool = Tool::new("rs-infra-tools", "rs-infra-tools", "qubit-infra-tools");

#[derive(Clone, Copy)]
struct Tool {
    name: &'static str,
    repository: &'static str,
    package: &'static str,
}

impl Tool {
    const fn new(name: &'static str, repository: &'static str, package: &'static str) -> Self {
        Self {
            name,
            repository,
            package,
        }
    }

    fn source(self) -> String {
        format!("https://github.com/qubit-ltd/{}.git", self.repository)
    }
}

struct ResolvedTool {
    tool: Tool,
    revision: String,
    executable: PathBuf,
}

/// Resolves the latest main revision, builds it once in the user cache, and runs it.
///
/// When `tool` is `rs-infra-ci`, all supported task tools are prepared and exposed
/// through `RS_INFRA_BIN_DIR` so the CI orchestrator uses the same revision snapshot.
/// Network lookup and Git/Cargo dependency fetches are retried with bounded backoff.
pub fn run_latest(project: &Path, tool: &str, args: &[String]) -> Result<i32> {
    let selected = find_tool(tool)?;
    let mut names = vec![selected.name];
    if selected.name == "rs-infra-ci" {
        names.extend(
            TOOLS
                .iter()
                .map(|item| item.name)
                .filter(|name| *name != selected.name),
        );
    }

    let cache = cache_root()?.join("rs-infra");
    let host = host_target()?;
    let rustc_release = rustc_release()?;
    // The stable project bootstrap uses this checkout for the dynamic shell
    // entrypoints and shared libraries invoked by infrastructure tools.
    let manager_source = manager_source(&cache)?;
    let mut resolved = Vec::with_capacity(names.len());
    for name in names {
        let selected = find_tool(name)?;
        let revision = resolve_main(selected)?;
        let tool_cache = cache
            .join("tools")
            .join(selected.name)
            .join(&revision)
            .join(&host)
            .join(&rustc_release);
        let lock_path = cache.join("locks").join(format!(
            "{}-{revision}-{host}-{rustc_release}.lock",
            selected.name
        ));
        let _lock = CacheLock::acquire(&lock_path)?;
        let source_dir = cache.join("sources").join(selected.name).join(&revision);
        let executable = ensure_built(selected, &revision, &source_dir, &tool_cache)?;
        resolved.push(ResolvedTool {
            tool: selected,
            revision,
            executable,
        });
    }

    let bin_dir = invocation_bin_dir(&cache, &resolved)?;
    let selected_tool = resolved
        .iter()
        .find(|resolved_tool| resolved_tool.tool.name == selected.name)
        .context("selected tool was not resolved")?;
    let mut command = Command::new(&selected_tool.executable);
    command.args(args).current_dir(project);
    command.env("RS_INFRA_PROJECT_ROOT", project);
    command.env("RS_INFRA_CACHE_DIR", cache.parent().unwrap_or(&cache));
    command.env("RS_INFRA_SHARED_ROOT", &manager_source);
    command.env("RS_INFRA_BIN_DIR", &bin_dir);
    command.env(
        "RS_INFRA_TOOLS_BIN",
        env::current_exe().context("failed to resolve current executable")?,
    );
    command.env("PATH", prepend_path(&bin_dir));
    for item in &resolved {
        eprintln!("rs-infra: {}@{} ({host})", item.tool.name, item.revision);
    }
    let status = command
        .status()
        .with_context(|| format!("failed to start {}", selected.name))?;
    Ok(status.code().unwrap_or(1))
}

/// Resolves and builds every supported tool without changing the project tree.
pub fn prewarm(_project: &Path) -> Result<()> {
    let cache = cache_root()?.join("rs-infra");
    let host = host_target()?;
    let rustc_release = rustc_release()?;
    let _manager_source = manager_source(&cache)?;
    let mut resolved = Vec::with_capacity(TOOLS.len());
    for tool in TOOLS {
        let revision = resolve_main(*tool)?;
        let tool_cache = cache
            .join("tools")
            .join(tool.name)
            .join(&revision)
            .join(&host)
            .join(&rustc_release);
        let lock_path = cache.join("locks").join(format!(
            "{}-{revision}-{host}-{rustc_release}.lock",
            tool.name
        ));
        let _lock = CacheLock::acquire(&lock_path)?;
        let source_dir = cache.join("sources").join(tool.name).join(&revision);
        let executable = ensure_built(*tool, &revision, &source_dir, &tool_cache)?;
        resolved.push(ResolvedTool {
            tool: *tool,
            revision,
            executable,
        });
    }
    let _ = invocation_bin_dir(&cache, &resolved)?;
    Ok(())
}

fn manager_source(cache: &Path) -> Result<PathBuf> {
    if let Some(source) = env::var_os("RS_INFRA_SHARED_ROOT") {
        let source = PathBuf::from(source);
        if source.join(".git").is_dir() && source.join(".infra/bin").is_dir() {
            return Ok(source);
        }
        bail!("RS_INFRA_SHARED_ROOT does not contain an rs-infra-tools checkout");
    }
    let revision = resolve_main(MANAGER)?;
    let source = cache.join("sources").join(MANAGER.name).join(&revision);
    let lock_path = cache
        .join("locks")
        .join(format!("manager-source-{revision}.lock"));
    let _lock = CacheLock::acquire(&lock_path)?;
    checkout_source(MANAGER, &revision, &source)?;
    Ok(source)
}

fn find_tool(name: &str) -> Result<Tool> {
    TOOLS
        .iter()
        .copied()
        .find(|tool| tool.name == name)
        .with_context(|| format!("unknown infrastructure tool '{name}'"))
}

fn resolve_main(tool: Tool) -> Result<String> {
    let source = tool.source();
    let output = retry("resolve main revision", || {
        Command::new("git")
            .args(["ls-remote", &source, "refs/heads/main"])
            .output()
            .context("failed to start git ls-remote")
    })?;
    if !output.status.success() {
        bail!(
            "failed to resolve {} main: {}",
            tool.name,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let text =
        String::from_utf8(output.stdout).context("git ls-remote returned non-UTF-8 output")?;
    let revision = text
        .split_whitespace()
        .next()
        .context("main branch returned no revision")?;
    if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!(
            "{} main returned an invalid Git SHA '{revision}'",
            tool.name
        );
    }
    Ok(revision.to_ascii_lowercase())
}

fn ensure_built(
    tool: Tool,
    revision: &str,
    source_dir: &Path,
    tool_cache: &Path,
) -> Result<PathBuf> {
    let executable = tool_cache.join("bin").join(executable_file_name(tool.name));
    if executable.is_file() {
        return Ok(executable);
    }
    fs::create_dir_all(source_dir.parent().context("source cache has no parent")?)?;
    fs::create_dir_all(tool_cache)?;
    checkout_source(tool, revision, source_dir)?;

    let manifest = source_dir.join("Cargo.toml");
    if !manifest.is_file() {
        bail!("{} at {revision} has no Cargo.toml", tool.name);
    }
    retry("fetch Cargo dependencies", || {
        Command::new("cargo")
            .args(["fetch", "--locked", "--manifest-path"])
            .arg(&manifest)
            .current_dir(source_dir)
            .output()
            .context("failed to start cargo fetch")
    })?;
    let target_dir = tool_cache.join("target");
    let output = Command::new("cargo")
        .args([
            "build",
            "--release",
            "--locked",
            "--offline",
            "--manifest-path",
        ])
        .arg(&manifest)
        .args([
            "--package",
            tool.package,
            "--bin",
            tool.name,
            "--target-dir",
        ])
        .arg(&target_dir)
        .current_dir(source_dir)
        .output()
        .context("failed to start cargo build")?;
    if !output.status.success() {
        bail!(
            "failed to build {}@{revision}:\n{}{}",
            tool.name,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let built = target_dir
        .join("release")
        .join(executable_file_name(tool.name));
    if !built.is_file() {
        bail!("cargo build succeeded but {} is missing", built.display());
    }
    let bin_dir = executable
        .parent()
        .context("tool executable has no parent")?;
    fs::create_dir_all(bin_dir)?;
    let temporary = bin_dir.join(format!(".{}.tmp-{}", tool.name, std::process::id()));
    fs::copy(&built, &temporary).with_context(|| format!("failed to publish {}", tool.name))?;
    fs::rename(&temporary, &executable)
        .with_context(|| format!("failed to install {}", tool.name))?;
    Ok(executable)
}

fn checkout_source(tool: Tool, revision: &str, source_dir: &Path) -> Result<()> {
    let source = tool.source();
    if !source_dir.join(".git").exists() {
        fs::create_dir_all(source_dir)?;
        let init = Command::new("git").arg("init").arg(source_dir).output()?;
        if !init.status.success() {
            bail!("failed to initialize source cache for {}", tool.name);
        }
        let remote = Command::new("git")
            .args(["-C"])
            .arg(source_dir)
            .args(["remote", "add", "origin", &source])
            .output()?;
        if !remote.status.success() {
            bail!("failed to configure source cache for {}", tool.name);
        }
    }
    let fetch = retry("fetch tool source", || {
        Command::new("git")
            .args(["-C"])
            .arg(source_dir)
            .args(["fetch", "--force", "--depth", "1", "origin", revision])
            .output()
            .context("failed to start git fetch")
    })?;
    if !fetch.status.success() {
        bail!(
            "failed to fetch {}@{revision}: {}",
            tool.name,
            String::from_utf8_lossy(&fetch.stderr).trim()
        );
    }
    let checkout = Command::new("git")
        .args(["-C"])
        .arg(source_dir)
        .args(["checkout", "--detach", "--force", "FETCH_HEAD"])
        .output()?;
    if !checkout.status.success() {
        bail!(
            "failed to check out {}@{revision}: {}",
            tool.name,
            String::from_utf8_lossy(&checkout.stderr).trim()
        );
    }
    let head = Command::new("git")
        .args(["-C"])
        .arg(source_dir)
        .args(["rev-parse", "HEAD"])
        .output()?;
    let actual = String::from_utf8(head.stdout)?.trim().to_owned();
    if !head.status.success() || actual != revision {
        bail!(
            "source cache for {} resolved to {actual}, expected {revision}",
            tool.name
        );
    }
    Ok(())
}

fn retry<F>(description: &str, mut operation: F) -> Result<std::process::Output>
where
    F: FnMut() -> Result<std::process::Output>,
{
    let attempts = env::var("RS_INFRA_NETWORK_MAX_ATTEMPTS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(4);
    let mut delay = env::var("RS_INFRA_NETWORK_RETRY_DELAY_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(2);
    for attempt in 1..=attempts {
        let output = operation()?;
        if output.status.success() {
            return Ok(output);
        }
        if attempt == attempts {
            bail!(
                "{description} failed after {attempt}/{attempts} attempts: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        eprintln!(
            "warning: {description} failed (attempt {attempt}/{attempts}); retrying in {delay}s"
        );
        thread::sleep(Duration::from_secs(delay));
        delay = delay.saturating_mul(2);
    }
    bail!("{description} failed without an attempt")
}

fn invocation_bin_dir(cache: &Path, tools: &[ResolvedTool]) -> Result<PathBuf> {
    let mut manifest = String::new();
    for tool in tools {
        manifest.push_str(tool.tool.name);
        manifest.push('@');
        manifest.push_str(&tool.revision);
        manifest.push('\n');
    }
    let key = format!("{:x}", Sha256::digest(manifest.as_bytes()));
    let invocation_dir = cache.join("invocations").join(&key);
    let bin_dir = invocation_dir.join("bin");
    let lock_path = cache.join("locks").join(format!("invocation-{key}.lock"));
    let _lock = CacheLock::acquire(&lock_path)?;
    let manifest_path = invocation_dir.join("manifest.txt");
    if bin_dir.is_dir()
        && fs::read_to_string(&manifest_path).is_ok_and(|cached| cached == manifest)
        && tools
            .iter()
            .all(|tool| bin_dir.join(executable_file_name(tool.tool.name)).is_file())
    {
        return Ok(bin_dir);
    }
    if invocation_dir.exists() {
        fs::remove_dir_all(&invocation_dir)?;
    }
    let parent = invocation_dir
        .parent()
        .context("invocation path has no parent")?;
    fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".tmp-{key}-{}", std::process::id()));
    if temporary.exists() {
        fs::remove_dir_all(&temporary)?;
    }
    fs::create_dir_all(temporary.join("bin"))?;
    for tool in tools {
        let destination = temporary
            .join("bin")
            .join(executable_file_name(tool.tool.name));
        if fs::hard_link(&tool.executable, &destination).is_err() {
            fs::copy(&tool.executable, &destination)?;
        }
    }
    fs::write(temporary.join("manifest.txt"), manifest.as_bytes())?;
    fs::rename(&temporary, &invocation_dir)?;
    Ok(bin_dir)
}

fn executable_file_name(name: &str) -> String {
    format!("{name}{}", env::consts::EXE_SUFFIX)
}

fn cache_root() -> Result<PathBuf> {
    env::var_os("RS_INFRA_CACHE_DIR")
        .map(PathBuf::from)
        .or_else(|| env::var_os("XDG_CACHE_HOME").map(|path| PathBuf::from(path).join("qubit")))
        .or_else(|| env::var_os("HOME").map(|path| PathBuf::from(path).join(".cache/qubit")))
        .context("RS_INFRA_CACHE_DIR, XDG_CACHE_HOME, or HOME must be set")
}

fn host_target() -> Result<String> {
    let output = Command::new("rustc")
        .arg("-vV")
        .output()
        .context("failed to start rustc")?;
    if !output.status.success() {
        bail!("rustc -vV failed");
    }
    let text = String::from_utf8(output.stdout)?;
    text.lines()
        .find_map(|line| line.strip_prefix("host: "))
        .map(str::to_owned)
        .context("rustc -vV did not report the host target")
}

fn rustc_release() -> Result<String> {
    let output = Command::new("rustc")
        .arg("--version")
        .output()
        .context("failed to start rustc")?;
    if !output.status.success() {
        bail!("rustc --version failed");
    }
    let version = String::from_utf8(output.stdout)?;
    let version = version.trim().replace(' ', "-");
    Ok(version.replace('/', "-"))
}

fn prepend_path(path: &Path) -> std::ffi::OsString {
    let mut paths = vec![path.to_path_buf()];
    if let Some(existing) = env::var_os("PATH") {
        paths.extend(env::split_paths(&existing));
    }
    env::join_paths(paths).unwrap_or_else(|_| env::var_os("PATH").unwrap_or_default())
}

struct CacheLock {
    file: File,
}

impl CacheLock {
    fn acquire(path: &Path) -> Result<Self> {
        fs::create_dir_all(path.parent().context("lock path has no parent")?)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(path)
            .with_context(|| format!("failed to open cache lock {}", path.display()))?;
        file.lock_exclusive()
            .with_context(|| format!("failed to lock cache entry {}", path.display()))?;
        Ok(Self { file })
    }
}

impl Drop for CacheLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::ResolvedTool;
    use super::Tool;
    use super::executable_file_name;
    use super::invocation_bin_dir;

    #[test]
    fn invocation_bin_dir_contains_tools_and_repairs_incomplete_cache() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory
            .path()
            .join("source")
            .join(executable_file_name("rs-infra-verify"));
        std::fs::create_dir_all(executable.parent().unwrap()).unwrap();
        std::fs::write(&executable, b"tool binary").unwrap();
        let tools = [ResolvedTool {
            tool: Tool::new("rs-infra-verify", "rs-infra-verify", "qubit-infra-verify"),
            revision: "0123456789abcdef0123456789abcdef01234567".to_owned(),
            executable,
        }];

        let bin_dir = invocation_bin_dir(directory.path(), &tools).unwrap();
        let installed = bin_dir.join(executable_file_name("rs-infra-verify"));
        assert_eq!(std::fs::read(&installed).unwrap(), b"tool binary");

        std::fs::remove_file(&installed).unwrap();
        let repaired = invocation_bin_dir(directory.path(), &tools).unwrap();
        assert_eq!(
            std::fs::read(repaired.join(executable_file_name("rs-infra-verify"))).unwrap(),
            b"tool binary"
        );
    }
}
