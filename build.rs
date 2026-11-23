use std::env;
use std::process::Command;

fn main() {
    // Embed the commit and build profile into the WASM `contractmetav0`
    // section so explorers and `stellar contract info meta` can map a deployed
    // contract back to its source. Falls back to "unknown" when building from
    // a source tree without git metadata (e.g. a Docker context without .git).
    let git_sha = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|sha| sha.trim().to_string())
        .filter(|sha| !sha.is_empty())
        .unwrap_or_else(|| "unknown".to_string());

    let profile = env::var("PROFILE").unwrap_or_else(|_| "unknown".to_string());

    println!("cargo:rustc-env=GIT_SHA={git_sha}");
    println!("cargo:rustc-env=BUILD_PROFILE={profile}");
    println!("cargo:rerun-if-changed=.git/HEAD");
    println!("cargo:rerun-if-changed=.git/refs");
}
