// SPDX-License-Identifier: Apache-2.0 OR MIT

//! RUSTC wrapper implementation for cargo-llvm-cov
//!
//! This module implements a rustc wrapper that adds coverage instrumentation flags
//! when invoked by cargo. Instead of setting RUSTFLAGS globally, we use RUSTC_WRAPPER
//! which provides better interaction with user-configured RUSTFLAGS and more control
//! over which crates get instrumented.

use std::{
    env,
    ffi::OsString,
    process::{Command, ExitCode},
};

use anyhow::{Context as _, Result};

/// Run as a rustc wrapper
///
/// When cargo-llvm-cov is invoked as RUSTC_WRAPPER, this function:
/// 1. Receives the path to rustc as the first argument
/// 2. Receives all rustc arguments
/// 3. Adds coverage instrumentation flags based on environment variables
/// 4. Calls the real rustc with the modified arguments
pub(crate) fn run_wrapper() -> ExitCode {
    match try_run_wrapper() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("cargo-llvm-cov wrapper error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn try_run_wrapper() -> Result<ExitCode> {
    let mut args = env::args_os();

    // First arg is our binary name, skip it
    args.next();

    // Second arg is the rustc path
    let rustc = args.next().context("rustc wrapper called without rustc path")?;

    // Remaining args are rustc arguments
    let rustc_args: Vec<OsString> = args.collect();

    // Check if we should add instrumentation for this invocation
    let should_instrument = should_instrument();

    // Debug logging if CARGO_LLVM_COV_WRAPPER_DEBUG is set
    if env::var_os("CARGO_LLVM_COV_WRAPPER_DEBUG").is_some() {
        if let (Some(crate_name), Some(pkg_name)) =
            (env::var_os("CARGO_CRATE_NAME"), env::var_os("CARGO_PKG_NAME"))
        {
            eprintln!(
                "cargo-llvm-cov wrapper: crate={}, pkg={}, primary={}, instrument={}",
                crate_name.to_string_lossy(),
                pkg_name.to_string_lossy(),
                env::var_os("CARGO_PRIMARY_PACKAGE").is_some(),
                should_instrument
            );
        }
    }

    // Build the final argument list: coverage flags + original args
    let final_args = if should_instrument {
        let mut coverage_flags = Vec::new();
        add_coverage_flags(&mut coverage_flags)?;
        coverage_flags.extend(rustc_args);
        coverage_flags
    } else {
        rustc_args
    };

    // Execute rustc
    let status = Command::new(&rustc)
        .args(&final_args)
        .status()
        .with_context(|| format!("failed to execute rustc: {}", rustc.to_string_lossy()))?;

    Ok(if status.success() { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// Determine if we should instrument this rustc invocation
fn should_instrument() -> bool {
    // Check if cargo-llvm-cov environment is active
    let Some(_) = env::var_os("CARGO_LLVM_COV") else {
        return false;
    };

    // If we can't determine the crate name, it might be a rustc invocation
    // that's not part of a cargo build (e.g., rustc --version check)
    // In this case, don't instrument
    if env::var_os("CARGO_CRATE_NAME").is_none() && env::var_os("CARGO_PKG_NAME").is_none() {
        return false;
    }

    // Check if this is a coverage_target_only build and we're not on the target
    if let (Some(coverage_target), Some(target)) =
        (env::var_os("CARGO_LLVM_COV_TARGET_ONLY"), env::var_os("TARGET"))
    {
        if target != coverage_target {
            return false;
        }
    }

    // When using RUSTC_WORKSPACE_WRAPPER, Cargo automatically only calls us
    // for workspace members, not dependencies. When using RUSTC_WRAPPER (with
    // --dep-coverage), we instrument everything.
    //
    // If CARGO_LLVM_COV_DEP_COVERAGE is set, we're using RUSTC_WRAPPER and
    // should instrument everything. Otherwise, when using RUSTC_WORKSPACE_WRAPPER,
    // instrument everything (Cargo already filtered for workspace members).
    true
}

/// Add coverage instrumentation flags to the argument list
fn add_coverage_flags(flags: &mut Vec<OsString>) -> Result<()> {
    let Some(cov_flags) = env::var_os("CARGO_LLVM_COV_FLAGS") else {
        return Ok(());
    };

    // Parse space-separated flags
    let cov_flags_str =
        cov_flags.to_str().context("CARGO_LLVM_COV_FLAGS contains invalid UTF-8")?;

    flags.extend(cov_flags_str.split_whitespace().map(OsString::from));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_instrument_no_env() {
        env::remove_var("CARGO_LLVM_COV");
        assert!(!should_instrument());
    }

    #[test]
    fn test_should_instrument_with_env() {
        env::set_var("CARGO_LLVM_COV", "1");
        env::remove_var("CARGO_LLVM_COV_TARGET_ONLY");
        assert!(should_instrument());
        env::remove_var("CARGO_LLVM_COV");
    }
}
