//! Locating and running the external helper programs.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::{FetchError, Result};

/// Find a helper by explicit path first, then next to the app, then on PATH.
/// GUI apps on macOS do not inherit the shell PATH, so common package manager
/// locations are searched as well.
pub fn find_tool(explicit: Option<&Path>, name: &str) -> Option<PathBuf> {
    if let Some(path) = explicit
        && path.is_file()
    {
        return Some(path.to_path_buf());
    }

    let mut candidates = vec![name.to_string()];
    if cfg!(windows) {
        candidates.push(format!("{name}.exe"));
    }

    let bundled = videleum_core::tools_dir();
    for candidate in &candidates {
        let path = bundled.join(candidate);
        if path.is_file() {
            return Some(path);
        }
    }

    for candidate in &candidates {
        if let Some(path) = search_path(candidate) {
            return Some(path);
        }
    }

    #[cfg(target_os = "macos")]
    {
        for directory in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
            for candidate in &candidates {
                let path = Path::new(directory).join(candidate);
                if path.is_file() {
                    return Some(path);
                }
            }
        }
    }
    #[cfg(target_os = "linux")]
    {
        for directory in ["/usr/local/bin", "/usr/bin", "/bin", "/snap/bin"] {
            for candidate in &candidates {
                let path = Path::new(directory).join(candidate);
                if path.is_file() {
                    return Some(path);
                }
            }
        }
    }
    None
}

fn search_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// A command that never flashes a console window on Windows.
pub fn command(path: &Path) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut command = Command::new(path);
        command.creation_flags(CREATE_NO_WINDOW);
        command
    }
    #[cfg(not(windows))]
    {
        Command::new(path)
    }
}

pub fn run_capture(path: &Path, arguments: &[&str]) -> Result<String> {
    let output = command(path)
        .args(arguments)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| {
            FetchError::Tool(format!(
                "Couldn't run {}: {error}",
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("the helper")
            ))
        })?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        return Err(FetchError::Tool(last_lines(detail, 6)));
    }
    Ok(stdout)
}

pub fn last_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    let start = lines.len().saturating_sub(count);
    lines[start..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_the_tail_of_tool_output() {
        assert_eq!(last_lines("one\ntwo\nthree", 2), "two\nthree");
        assert_eq!(last_lines("  \n one \n", 3), "one");
    }
}
