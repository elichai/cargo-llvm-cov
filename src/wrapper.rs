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
    let mut args: Vec<OsString> = env::args_os().collect();

    // First arg is our binary name, second is rustc path, rest are rustc args
    if args.len() < 2 {
        anyhow::bail!("rustc wrapper called with insufficient arguments");
    }

    // Remove our binary name
    args.remove(0);

    // First remaining arg is the rustc path
    let rustc = args.remove(0);

    // Add coverage instrumentation flags before other arguments
    // These are read from environment variables set by cargo-llvm-cov
    let mut coverage_flags = Vec::new();

    // Check if we should add instrumentation for this invocation
    if should_instrument()? {
        add_coverage_flags(&mut coverage_flags)?;
    }

    // Build the final argument list: coverage flags + original args
    let mut final_args = coverage_flags;
    final_args.extend(args);

    // Execute rustc
    let status = Command::new(&rustc)
        .args(&final_args)
        .status()
        .with_context(|| format!("failed to execute rustc: {}", rustc.to_string_lossy()))?;

    Ok(if status.success() { ExitCode::SUCCESS } else { ExitCode::FAILURE })
}

/// Determine if we should instrument this rustc invocation
fn should_instrument() -> Result<bool> {
    // Check if cargo-llvm-cov environment is active
    if env::var_os("CARGO_LLVM_COV").is_none() {
        return Ok(false);
    }

    // Check if this is a coverage_target_only build and we're not on the target
    if let Some(coverage_target) = env::var_os("CARGO_LLVM_COV_TARGET_ONLY") {
        if let Some(target) = env::var_os("TARGET") {
            if target != coverage_target {
                return Ok(false);
            }
        }
    }

    Ok(true)
}

/// Add coverage instrumentation flags to the argument list
fn add_coverage_flags(flags: &mut Vec<OsString>) -> Result<()> {
    // Read the coverage flags from environment variable set by cargo-llvm-cov
    if let Some(cov_flags) = env::var_os("CARGO_LLVM_COV_FLAGS") {
        // Parse space-separated flags
        let cov_flags_str = cov_flags
            .to_str()
            .context("CARGO_LLVM_COV_FLAGS contains invalid UTF-8")?;

        for flag in cov_flags_str.split_whitespace() {
            flags.push(OsString::from(flag));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_instrument_no_env() {
        env::remove_var("CARGO_LLVM_COV");
        assert!(!should_instrument().unwrap());
    }

    #[test]
    fn test_should_instrument_with_env() {
        env::set_var("CARGO_LLVM_COV", "1");
        env::remove_var("CARGO_LLVM_COV_TARGET_ONLY");
        assert!(should_instrument().unwrap());
        env::remove_var("CARGO_LLVM_COV");
    }
}
