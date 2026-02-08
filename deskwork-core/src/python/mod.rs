//! Python environment management using UV.
//!
//! This module provides functionality for managing Python virtual environments
//! and running Python scripts using UV, which is downloaded on-demand and stored
//! in the OS temp directory.
//!
//! UV is a fast Python package installer and resolver written in Rust.
//! It is downloaded via the external tools system when first needed.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tracing::{debug, info, warn};

use crate::external_tools;
use crate::external_tools::manifest::load_manifest;
use crate::external_tools::paths;
use crate::external_tools::types::ExternalToolId;

// ============================================================================
// UV binary management
// ============================================================================

/// Ensures the UV binary is available and returns its path.
///
/// Checks if UV has been installed via the external tools system.
/// If UV is not installed, returns an error instructing the user to install it.
pub fn ensure_uv_available() -> Result<PathBuf> {
    let manifest = load_manifest().context("Failed to load tools manifest")?;

    if !manifest.is_installed(ExternalToolId::Uv) {
        anyhow::bail!(
            "UV is not installed. Please install it to use Python features.\n\
             UV can be installed via the external tools manager."
        );
    }

    let uv_path = paths::get_uv_binary_path();

    if !uv_path.exists() {
        anyhow::bail!(
            "UV is marked as installed but the binary was not found at {}. \
             Please reinstall UV.",
            uv_path.display()
        );
    }

    debug!("UV binary available at {}", uv_path.display());
    Ok(uv_path)
}

/// Checks if UV is installed without returning an error.
pub fn is_uv_installed() -> bool {
    match load_manifest() {
        Ok(manifest) => {
            if !manifest.is_installed(ExternalToolId::Uv) {
                return false;
            }
            paths::get_uv_binary_path().exists()
        }
        Err(_) => false,
    }
}

// ============================================================================
// Virtual environment management
// ============================================================================

/// Returns the path to the Python executable in a virtual environment.
///
/// - Unix: `{venv_path}/bin/python`
/// - Windows: `{venv_path}/Scripts/python.exe`
pub fn get_venv_python(venv_path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        venv_path.join("Scripts").join("python.exe")
    }
    #[cfg(not(windows))]
    {
        venv_path.join("bin").join("python")
    }
}

/// Returns the default venv path for a given name.
///
/// Path: `{temp}/deskwork/venvs/{name}/`
pub fn default_venv_path(name: &str) -> PathBuf {
    paths::get_venvs_dir().join(name)
}

/// Creates a Python virtual environment at the specified path.
///
/// Uses UV to create the virtual environment, which is significantly
/// faster than the standard `python -m venv` approach.
pub fn create_venv(venv_path: &Path) -> Result<()> {
    let uv_path = ensure_uv_available()?;

    info!("Creating virtual environment at {}", venv_path.display());

    let mut cmd = Command::new(&uv_path);
    cmd.arg("venv").arg(venv_path);

    // Apply external tools PATH so UV can find any required binaries
    if let Err(e) = external_tools::apply_to_command(&mut cmd) {
        warn!("Failed to apply external tools env to UV venv: {}", e);
    }

    let output = cmd
        .output()
        .with_context(|| "Failed to execute UV venv command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to create virtual environment: {}", stderr);
    }

    debug!("Virtual environment created successfully");
    Ok(())
}

/// Ensures a virtual environment exists at the given path, creating it if needed.
pub fn ensure_venv(venv_path: &Path) -> Result<()> {
    let python_path = get_venv_python(venv_path);
    if python_path.exists() {
        debug!(
            "Virtual environment already exists at {}",
            venv_path.display()
        );
        return Ok(());
    }
    create_venv(venv_path)
}

/// Installs Python packages into a virtual environment using UV.
pub fn pip_install(venv_path: &Path, packages: &[&str]) -> Result<()> {
    if packages.is_empty() {
        return Ok(());
    }

    let uv_path = ensure_uv_available()?;
    let python_path = get_venv_python(venv_path);

    info!(
        "Installing packages into {}: {:?}",
        venv_path.display(),
        packages
    );

    let mut cmd = Command::new(&uv_path);
    cmd.arg("pip")
        .arg("install")
        .arg("--python")
        .arg(&python_path);

    for package in packages {
        cmd.arg(package);
    }

    if let Err(e) = external_tools::apply_to_command(&mut cmd) {
        warn!(
            "Failed to apply external tools env to UV pip install: {}",
            e
        );
    }

    let output = cmd
        .output()
        .with_context(|| "Failed to execute UV pip install")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to install packages: {}", stderr);
    }

    debug!("Packages installed successfully");
    Ok(())
}

/// Installs Python packages from a requirements file into a virtual environment.
pub fn pip_install_requirements(venv_path: &Path, requirements_file: &Path) -> Result<()> {
    let uv_path = ensure_uv_available()?;
    let python_path = get_venv_python(venv_path);

    info!(
        "Installing requirements from {} into {}",
        requirements_file.display(),
        venv_path.display()
    );

    let mut cmd = Command::new(&uv_path);
    cmd.arg("pip")
        .arg("install")
        .arg("--python")
        .arg(&python_path)
        .arg("-r")
        .arg(requirements_file);

    if let Err(e) = external_tools::apply_to_command(&mut cmd) {
        warn!(
            "Failed to apply external tools env to UV pip install -r: {}",
            e
        );
    }

    let output = cmd
        .output()
        .with_context(|| "Failed to execute UV pip install -r")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to install requirements: {}", stderr);
    }

    debug!("Requirements installed successfully");
    Ok(())
}

// ============================================================================
// Python script execution
// ============================================================================

/// Runs a Python script using the Python interpreter from a virtual environment.
pub fn run_python_script(venv_path: &Path, script: &Path, args: &[&str]) -> Result<Output> {
    let python_path = get_venv_python(venv_path);

    if !python_path.exists() {
        anyhow::bail!(
            "Python executable not found at {}. Was the venv created?",
            python_path.display()
        );
    }

    debug!(
        "Running Python script: {} {:?} with args {:?}",
        python_path.display(),
        script.display(),
        args
    );

    let mut cmd = Command::new(&python_path);
    cmd.arg(script);
    for arg in args {
        cmd.arg(arg);
    }

    if let Err(e) = external_tools::apply_to_command(&mut cmd) {
        warn!("Failed to apply external tools env to Python script: {}", e);
    }

    let output = cmd
        .output()
        .with_context(|| format!("Failed to execute Python script: {}", script.display()))?;

    Ok(output)
}

/// Runs a Python module using the `-m` flag.
pub fn run_python_module(venv_path: &Path, module: &str, args: &[&str]) -> Result<Output> {
    let python_path = get_venv_python(venv_path);

    if !python_path.exists() {
        anyhow::bail!(
            "Python executable not found at {}. Was the venv created?",
            python_path.display()
        );
    }

    debug!(
        "Running Python module: {} -m {} {:?}",
        python_path.display(),
        module,
        args
    );

    let mut cmd = Command::new(&python_path);
    cmd.arg("-m").arg(module);
    for arg in args {
        cmd.arg(arg);
    }

    if let Err(e) = external_tools::apply_to_command(&mut cmd) {
        warn!("Failed to apply external tools env to Python module: {}", e);
    }

    let output = cmd
        .output()
        .with_context(|| format!("Failed to execute Python module: {}", module))?;

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_venv_python() {
        let venv = Path::new("/tmp/test-venv");
        let python = get_venv_python(venv);

        #[cfg(windows)]
        assert!(python.ends_with("Scripts/python.exe"));

        #[cfg(not(windows))]
        assert!(python.ends_with("bin/python"));
    }

    #[test]
    fn test_default_venv_path() {
        let path = default_venv_path("my-skill");
        let path_str = path.to_string_lossy();
        assert!(path_str.contains("deskwork"));
        assert!(path_str.contains("venvs"));
        assert!(path_str.contains("my-skill"));
    }

    #[test]
    fn test_is_uv_installed_returns_bool() {
        // Should not panic regardless of UV installation state
        let _ = is_uv_installed();
    }

    #[test]
    #[ignore] // Run with `cargo test -- --ignored` — requires UV installed
    fn test_ensure_uv_available() {
        let result = ensure_uv_available();
        if let Ok(uv_path) = result {
            assert!(uv_path.exists(), "UV binary should exist after check");
        }
    }

    #[test]
    #[ignore] // Run with `cargo test -- --ignored` — requires UV installed and Python
    fn test_create_venv_and_check_python() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let venv_path = temp_dir.path().join("test-venv");

        create_venv(&venv_path).expect("Failed to create venv");

        let python_path = get_venv_python(&venv_path);
        assert!(
            python_path.exists(),
            "Python binary should exist at {}",
            python_path.display()
        );

        let output = Command::new(&python_path)
            .arg("--version")
            .output()
            .expect("Failed to run Python");

        assert!(output.status.success(), "Python --version should succeed");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let version_output = format!("{}{}", stdout, stderr);
        assert!(
            version_output.contains("Python"),
            "Python version output should contain 'Python', got: {}",
            version_output
        );
    }
}
