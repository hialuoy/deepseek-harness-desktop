//! First-launch bootstrap: download Node.js and install @deepseek-ai/dsh
//! into a user-local toolchain (~/.dsh/toolchain).

use std::path::{Path, PathBuf};

use semver::Version;

#[allow(dead_code)]
pub const NODE_VERSION: &str = "24.19.0";
#[allow(dead_code)]
pub const MIN_NODE_VERSION: &str = "22.0.0";
#[allow(dead_code)]
pub const DSH_PACKAGE: &str = "@deepseek-ai/dsh";

/// HOME on Unix, USERPROFILE on Windows — mirrors main.rs state-file lookup.
#[allow(dead_code)]
pub fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_default()
}

#[allow(dead_code)]
pub fn toolchain_dir() -> PathBuf {
    home_dir().join(".dsh").join("toolchain")
}

#[allow(dead_code)]
pub fn node_dir(toolchain: &Path, version: &str) -> PathBuf {
    toolchain.join(format!("node-{}", version))
}

#[allow(dead_code)]
pub fn node_bin(toolchain: &Path, version: &str) -> PathBuf {
    node_dir(toolchain, version)
        .join("bin")
        .join(if cfg!(windows) { "node.exe" } else { "node" })
}

#[allow(dead_code)]
pub fn npm_cli_js(toolchain: &Path, version: &str) -> PathBuf {
    node_dir(toolchain, version)
        .join("lib")
        .join("node_modules")
        .join("npm")
        .join("bin")
        .join("npm-cli.js")
}

/// Derive the bundled npm-cli.js path from the node binary location,
/// so it works for whatever version dir actually exists.
#[allow(dead_code)]
pub fn npm_cli_from_node(node: &Path) -> PathBuf {
    node.parent()
        .unwrap_or(Path::new(""))
        .parent()
        .unwrap_or(Path::new(""))
        .join("lib")
        .join("node_modules")
        .join("npm")
        .join("bin")
        .join("npm-cli.js")
}

/// (os, arch) naming used by nodejs.org release files; None on unsupported platforms.
#[allow(dead_code)]
pub fn platform_arch() -> Option<(&'static str, &'static str)> {
    let os = if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "win"
    } else {
        return None;
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        _ => return None,
    };
    Some((os, arch))
}

#[allow(dead_code)]
pub fn archive_name(version: &str, os: &str, arch: &str) -> String {
    let ext = if os == "darwin" { "tar.xz" } else { "zip" };
    format!("node-v{}-{}-{}.{}", version, os, arch, ext)
}

#[allow(dead_code)]
pub fn archive_url(version: &str, os: &str, arch: &str) -> String {
    format!(
        "https://nodejs.org/dist/v{}/{}",
        version,
        archive_name(version, os, arch)
    )
}

/// Top-level directory name inside the downloaded archive.
#[allow(dead_code)]
pub fn extracted_top_dir(version: &str, os: &str, arch: &str) -> String {
    format!("node-v{}-{}-{}", version, os, arch)
}

/// True when `node -v` output parses as >= MIN_NODE_VERSION.
#[allow(dead_code)]
pub fn parse_node_version_ok(output: &str) -> bool {
    let v = output.trim().strip_prefix('v').unwrap_or(output.trim());
    match Version::parse(v) {
        Ok(parsed) => parsed >= Version::parse(MIN_NODE_VERSION).unwrap(),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archive_name_for_macos_arm64_is_tar_xz() {
        assert_eq!(
            archive_name("24.19.0", "darwin", "arm64"),
            "node-v24.19.0-darwin-arm64.tar.xz"
        );
    }

    #[test]
    fn archive_name_for_windows_x64_is_zip() {
        assert_eq!(
            archive_name("24.19.0", "win", "x64"),
            "node-v24.19.0-win-x64.zip"
        );
    }

    #[test]
    fn archive_url_points_at_official_dist() {
        assert_eq!(
            archive_url("24.19.0", "darwin", "arm64"),
            "https://nodejs.org/dist/v24.19.0/node-v24.19.0-darwin-arm64.tar.xz"
        );
    }

    #[test]
    fn extracted_top_dir_matches_archive_stem() {
        assert_eq!(
            extracted_top_dir("24.19.0", "darwin", "arm64"),
            "node-v24.19.0-darwin-arm64"
        );
        assert_eq!(
            extracted_top_dir("24.19.0", "win", "x64"),
            "node-v24.19.0-win-x64"
        );
    }

    #[test]
    fn node_bin_points_into_version_dir() {
        let tc = PathBuf::from("/home/u/.dsh/toolchain");
        let name = if cfg!(windows) { "node.exe" } else { "node" };
        assert_eq!(
            node_bin(&tc, "24.19.0"),
            tc.join("node-24.19.0").join("bin").join(name)
        );
    }

    #[test]
    fn npm_cli_from_node_derives_lib_path() {
        let node = PathBuf::from("/x/toolchain/node-24.19.0/bin/node");
        assert_eq!(
            npm_cli_from_node(&node),
            PathBuf::from("/x/toolchain/node-24.19.0/lib/node_modules/npm/bin/npm-cli.js")
        );
    }

    #[test]
    fn platform_arch_maps_native_values() {
        let (os, arch) = platform_arch().expect("supported platform");
        assert!(os == "darwin" || os == "win");
        assert!(arch == "x64" || arch == "arm64");
    }

    #[test]
    fn node_version_parse_accepts_22_or_newer() {
        assert!(parse_node_version_ok("v22.9.0\n"));
        assert!(parse_node_version_ok("v24.19.0\n"));
    }

    #[test]
    fn node_version_parse_rejects_older_or_garbage() {
        assert!(!parse_node_version_ok("v20.3.1\n"));
        assert!(!parse_node_version_ok("v18.0.0\n"));
        assert!(!parse_node_version_ok("garbage"));
        assert!(!parse_node_version_ok(""));
    }
}
