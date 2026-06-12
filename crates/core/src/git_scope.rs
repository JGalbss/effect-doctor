use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanScope {
    /// Whole repository (default).
    Full,
    /// All issues in files changed vs the base ref (plus untracked files).
    ChangedFiles,
    /// Only issues on lines changed vs the base ref.
    ChangedLines,
}

/// Changed files relative to the repo root. `None` ranges = the whole file
/// counts as changed (untracked / files scope).
pub struct DiffInfo {
    pub files: HashMap<String, Option<Vec<(u32, u32)>>>,
}

impl DiffInfo {
    pub fn contains_file(&self, relative_path: &str) -> bool {
        self.files.contains_key(relative_path)
    }

    pub fn line_is_changed(&self, relative_path: &str, line: u32) -> bool {
        match self.files.get(relative_path) {
            None => false,
            Some(None) => true,
            Some(Some(ranges)) => ranges
                .iter()
                .any(|(start, end)| line >= *start && line <= *end),
        }
    }
}

fn git(root: &Path, arguments: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()
        .map_err(|error| format!("failed to run git: {error}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn ref_exists(root: &Path, reference: &str) -> bool {
    git(root, &["rev-parse", "--verify", "--quiet", reference]).is_ok()
}

/// Resolve the diff base: an explicit ref, or the merge-base with the first
/// of origin/main, origin/master, main, master that exists.
pub fn resolve_base(root: &Path, explicit: Option<&str>) -> Result<String, String> {
    if let Some(reference) = explicit {
        let merge_base = git(root, &["merge-base", "HEAD", reference])?;
        return Ok(merge_base.trim().to_string());
    }
    let candidate = ["origin/main", "origin/master", "main", "master"]
        .into_iter()
        .find(|reference| ref_exists(root, reference))
        .ok_or_else(|| {
            "no base branch found (tried origin/main, origin/master, main, master) — pass --base <ref>".to_string()
        })?;
    let merge_base = git(root, &["merge-base", "HEAD", candidate])?;
    Ok(merge_base.trim().to_string())
}

fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
    // @@ -a,b +start,count @@ — count omitted means 1, 0 means pure deletion.
    let plus_section = line.split("+").nth(1)?.split(' ').next()?;
    let mut parts = plus_section.split(',');
    let start: u32 = parts.next()?.parse().ok()?;
    let count: u32 = match parts.next() {
        Some(count) => count.parse().ok()?,
        None => 1,
    };
    if count == 0 {
        return None;
    }
    Some((start, start + count - 1))
}

fn untracked_files(root: &Path) -> Vec<String> {
    git(root, &["ls-files", "--others", "--exclude-standard"])
        .map(|stdout| stdout.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// Collect the change set vs `base`. With `with_lines`, parse -U0 hunks into
/// per-file changed line ranges.
pub fn collect_diff(root: &Path, base: &str, with_lines: bool) -> Result<DiffInfo, String> {
    let mut files: HashMap<String, Option<Vec<(u32, u32)>>> = HashMap::new();

    if with_lines {
        let stdout = git(root, &["diff", "-U0", "--diff-filter=ACMR", base])?;
        let mut current: Option<String> = None;
        for line in stdout.lines() {
            if let Some(path) = line.strip_prefix("+++ b/") {
                current = Some(path.to_string());
                files.entry(path.to_string()).or_insert_with(|| Some(Vec::new()));
                continue;
            }
            if !line.starts_with("@@") {
                continue;
            }
            let (Some(path), Some(range)) = (&current, parse_hunk_header(line)) else {
                continue;
            };
            if let Some(Some(ranges)) = files.get_mut(path) {
                ranges.push(range);
            }
        }
    } else {
        let stdout = git(root, &["diff", "--name-only", "--diff-filter=ACMR", base])?;
        for path in stdout.lines().filter(|path| !path.is_empty()) {
            files.insert(path.to_string(), None);
        }
    }

    for path in untracked_files(root) {
        files.insert(path, None);
    }

    Ok(DiffInfo { files })
}
