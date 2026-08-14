//! First-launch bootstrap: download Node.js and install @deepseek-ai/dsh
//! into a user-local toolchain (~/.dsh/toolchain).

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

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

/// Run `node -v` and check it meets MIN_NODE_VERSION.
#[allow(dead_code)]
pub fn node_version_ok(node: &Path) -> bool {
    Command::new(node)
        .arg("-v")
        .output()
        .map(|o| parse_node_version_ok(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or(false)
}

/// Locate a complete private toolchain: newest node-<ver>/bin/node plus the
/// installed dsh shim, if both exist.
#[allow(dead_code)]
pub fn private_node_and_dsh(toolchain: &Path) -> Option<(PathBuf, PathBuf)> {
    let mut versions: Vec<(Version, PathBuf)> = Vec::new();
    let entries = std::fs::read_dir(toolchain).ok()?;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if let Some(ver) = name
            .strip_prefix("node-")
            .and_then(|v| Version::parse(v).ok())
        {
            versions.push((ver, e.path()));
        }
    }
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    let (_ver, dir) = versions.into_iter().next()?;
    let node = dir
        .join("bin")
        .join(if cfg!(windows) { "node.exe" } else { "node" });
    let dsh = toolchain.join("node_modules").join(".bin").join("dsh");
    if node.is_file() && dsh.exists() {
        Some((node, dsh))
    } else {
        None
    }
}

/// Args for `node npm-cli.js <args...>` installing dsh into the private prefix.
#[allow(dead_code)]
pub fn npm_install_args(prefix: &Path) -> Vec<String> {
    vec![
        "install".to_string(),
        "--prefix".to_string(),
        prefix.to_string_lossy().into_owned(),
        "--no-fund".to_string(),
        "--no-audit".to_string(),
        DSH_PACKAGE.to_string(),
    ]
}

/// Run `<node> <npm-cli.js> install ...`; Ok(output) on success, Err(tail) on failure.
#[allow(dead_code)]
pub fn install_dsh(node: &Path, npm_cli: &Path, prefix: &Path) -> Result<String, String> {
    let output = Command::new(node)
        .arg(npm_cli)
        .args(npm_install_args(prefix))
        .output()
        .map_err(|e| format!("failed to run npm: {}", e))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(format!(
            "npm install failed (exit {:?}):\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// Stream-download `url` to `dest`, calling `on_progress(bytes, total)` at most
/// once per MiB (and once at completion).
#[allow(dead_code)]
pub fn download_node(
    url: &str,
    dest: &Path,
    mut on_progress: impl FnMut(u64, Option<u64>),
) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(600))
        .build()
        .map_err(|e| format!("http client: {}", e))?;
    let mut resp = client
        .get(url)
        .send()
        .map_err(|e| format!("download failed: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("download failed: HTTP {}", resp.status()));
    }
    let total = resp.content_length();
    let mut file =
        std::fs::File::create(dest).map_err(|e| format!("create {}: {}", dest.display(), e))?;
    let mut downloaded: u64 = 0;
    let mut since_emit: u64 = 0;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = resp.read(&mut buf).map_err(|e| format!("read: {}", e))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .map_err(|e| format!("write: {}", e))?;
        downloaded += n as u64;
        since_emit += n as u64;
        let done = total.map(|t| downloaded >= t).unwrap_or(false);
        if since_emit >= 1024 * 1024 || done {
            since_emit = 0;
            on_progress(downloaded, total);
        }
    }
    file.flush().map_err(|e| format!("flush: {}", e))?;
    Ok(())
}

/// Extract the downloaded archive into `dest_dir` using system tools:
/// `tar -xJf` on macOS, PowerShell `Expand-Archive` on Windows.
#[allow(dead_code)]
pub fn extract_archive(archive: &Path, dest_dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dest_dir)
        .map_err(|e| format!("mkdir {}: {}", dest_dir.display(), e))?;
    let status = if cfg!(target_os = "macos") {
        Command::new("tar")
            .arg("-xJf")
            .arg(archive)
            .arg("-C")
            .arg(dest_dir)
            .status()
    } else if cfg!(target_os = "windows") {
        Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg(format!(
                "Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force",
                archive.display(),
                dest_dir.display()
            ))
            .status()
    } else {
        return Err("unsupported platform".to_string());
    };
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("extract failed with status: {}", s)),
        Err(e) => Err(format!("failed to run extractor: {}", e)),
    }
}

/// Rename the archive's top-level dir to the canonical `node-<version>` name.
#[allow(dead_code)]
pub fn move_into_node_dir(
    toolchain: &Path,
    version: &str,
    os: &str,
    arch: &str,
) -> Result<(), String> {
    let from = toolchain.join(extracted_top_dir(version, os, arch));
    let to = node_dir(toolchain, version);
    std::fs::rename(&from, &to)
        .map_err(|e| format!("rename {} -> {}: {}", from.display(), to.display(), e))
}

/// Bootstrap progress states, surfaced to the UI (localized by the caller).
#[allow(dead_code)]
pub enum Step {
    Download,
    Extract,
    Install,
}

/// Full bootstrap: clean any half-installed toolchain, download, extract,
/// install dsh, verify. `on_progress(step, percent)` — percent is None while
/// the step is indeterminate.
#[allow(dead_code)]
pub fn install(mut on_progress: impl FnMut(Step, Option<f64>)) -> Result<(), String> {
    let (os, arch) = platform_arch().ok_or("unsupported platform/arch")?;
    let toolchain = toolchain_dir();
    if toolchain.exists() {
        std::fs::remove_dir_all(&toolchain)
            .map_err(|e| format!("cleanup {}: {}", toolchain.display(), e))?;
    }
    std::fs::create_dir_all(&toolchain)
        .map_err(|e| format!("mkdir {}: {}", toolchain.display(), e))?;

    let archive = toolchain.join(archive_name(NODE_VERSION, os, arch));
    let url = archive_url(NODE_VERSION, os, arch);
    println!("[bootstrap] downloading {}", url);
    download_node(&url, &archive, |downloaded, total| {
        let pct = match total {
            Some(t) if t > 0 => downloaded as f64 / t as f64,
            _ => 0.0,
        };
        on_progress(Step::Download, Some(pct));
    })?;

    on_progress(Step::Extract, None);
    extract_archive(&archive, &toolchain)?;
    move_into_node_dir(&toolchain, NODE_VERSION, os, arch)?;
    let _ = std::fs::remove_file(&archive);

    on_progress(Step::Install, None);
    let node = node_bin(&toolchain, NODE_VERSION);
    let npm_cli = npm_cli_js(&toolchain, NODE_VERSION);
    println!(
        "[bootstrap] installing {} via {}",
        DSH_PACKAGE,
        npm_cli.display()
    );
    install_dsh(&node, &npm_cli, &toolchain)?;

    if private_node_and_dsh(&toolchain).is_none() {
        return Err("toolchain incomplete after install".to_string());
    }
    println!("[bootstrap] toolchain ready at {}", toolchain.display());
    Ok(())
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

    #[test]
    fn private_node_and_dsh_requires_both_parts() {
        let root = std::env::temp_dir().join(format!("dsh-bootstrap-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let tc = root.join("toolchain");
        std::fs::create_dir_all(tc.join("node-24.19.0/bin")).unwrap();
        std::fs::write(tc.join("node-24.19.0/bin/node"), b"").unwrap();
        assert!(private_node_and_dsh(&tc).is_none());
        std::fs::create_dir_all(tc.join("node_modules/.bin")).unwrap();
        std::fs::write(tc.join("node_modules/.bin/dsh"), b"").unwrap();
        assert!(private_node_and_dsh(&tc).is_some());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn private_node_and_dsh_picks_newest_version() {
        let root =
            std::env::temp_dir().join(format!("dsh-bootstrap-newest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let tc = root.join("toolchain");
        for v in ["node-22.9.0", "node-24.19.0"] {
            std::fs::create_dir_all(tc.join(v).join("bin")).unwrap();
            std::fs::write(tc.join(v).join("bin/node"), b"").unwrap();
        }
        std::fs::create_dir_all(tc.join("node_modules/.bin")).unwrap();
        std::fs::write(tc.join("node_modules/.bin/dsh"), b"").unwrap();
        let (node, _dsh) = private_node_and_dsh(&tc).expect("both parts present");
        assert!(node.to_string_lossy().contains("node-24.19.0"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn npm_install_args_target_private_prefix() {
        let args = npm_install_args(Path::new("/home/u/.dsh/toolchain"));
        assert_eq!(
            args,
            vec![
                "install".to_string(),
                "--prefix".to_string(),
                "/home/u/.dsh/toolchain".to_string(),
                "--no-fund".to_string(),
                "--no-audit".to_string(),
                "@deepseek-ai/dsh".to_string(),
            ]
        );
    }

    #[test]
    fn install_dsh_errors_when_node_missing() {
        let result = install_dsh(
            Path::new("/nonexistent/node"),
            Path::new("/nonexistent/npm-cli.js"),
            Path::new("/tmp"),
        );
        assert!(result.is_err());
    }
}
