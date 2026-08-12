//! Which profile the crate being compiled is using.
//!
//! A build script reads `PROFILE`, but a proc macro is not a build script and
//! cargo does not set it for a rustc invocation. What it does set is the
//! command line, and a proc macro is a dynamic library loaded into the rustc
//! process, so [`std::env::args`] is rustc's own argv.

use std::env;

use exos_build::Mode;

/// The mode to build assets in, when it can be established.
///
/// [`None`] means this is not a recognisable rustc invocation, which in
/// practice means a proc-macro server such as rust-analyzer's. Nothing it
/// produces is ever served, so the caller may pick whichever mode is cheapest.
pub(crate) fn mode() -> Option<Mode> {
    debug_assertions().map(|debug| if debug { Mode::Debug } else { Mode::Release })
}

/// Identifies the compilation unit, when there is one to identify.
///
/// Rustc's `-C metadata` is unique per crate build, which is exactly the scope
/// anything cached across expansions may be reused within.
pub(crate) fn unit() -> Option<String> {
    let args: Vec<String> = env::args().collect();

    if !args.iter().any(|arg| arg == "--crate-name") {
        return None;
    }

    codegen(&args, "metadata")
}

/// Reproduces `cfg!(debug_assertions)` for the crate being compiled.
///
/// Cargo passes `-C debug-assertions` only when the profile disagrees with
/// rustc's default, which is on exactly at optimisation level zero. Verified
/// against the dev and release profiles, custom profiles inheriting from
/// either, and profiles that set `debug-assertions` against the grain.
///
/// The expansion asserts this against the real `cfg!(debug_assertions)`, so a
/// wrong answer here is a compile error rather than an asset that quietly
/// ships unminified.
pub(crate) fn debug_assertions() -> Option<bool> {
    let args: Vec<String> = env::args().collect();

    // Every rustc invocation has this. Refusing to guess without it keeps the
    // answer honest when something other than cargo is driving.
    if !args.iter().any(|arg| arg == "--crate-name") {
        return None;
    }

    if let Some(explicit) = codegen(&args, "debug-assertions") {
        return Some(matches!(
            explicit.as_str(),
            "on" | "yes" | "true" | "y" | ""
        ));
    }

    Some(codegen(&args, "opt-level").is_none_or(|level| level == "0"))
}

/// The value of a `-C name=value` flag, however it was spelled.
fn codegen(args: &[String], name: &str) -> Option<String> {
    let prefix = format!("{name}=");

    let separate = args
        .windows(2)
        .filter(|pair| pair[0] == "-C")
        .find_map(|pair| pair[1].strip_prefix(&prefix));

    separate
        .or_else(|| {
            args.iter()
                .find_map(|arg| arg.strip_prefix("-C")?.strip_prefix(&prefix))
        })
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(extra: &[&str]) -> Vec<String> {
        core::iter::once("rustc")
            .chain(["--crate-name", "app"])
            .chain(extra.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn reads_a_separated_flag() {
        assert_eq!(
            codegen(&args(&["-C", "opt-level=3"]), "opt-level").as_deref(),
            Some("3")
        );
    }

    #[test]
    fn reads_a_joined_flag() {
        assert_eq!(
            codegen(&args(&["-Copt-level=2"]), "opt-level").as_deref(),
            Some("2")
        );
    }

    #[test]
    fn a_missing_flag_is_absent_rather_than_empty() {
        assert_eq!(codegen(&args(&[]), "opt-level"), None);
    }
}
