use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

fn is_scannable(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    if name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts") {
        return false;
    }
    let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
        return false;
    };
    matches!(extension, "ts" | "tsx" | "mts" | "cts")
}

fn is_ignored_dir(name: &str) -> bool {
    matches!(
        name,
        "node_modules" | ".git" | "dist" | "build" | "coverage" | ".next" | ".turbo"
    )
}

/// Collect scannable TypeScript files under `root`, respecting .gitignore.
pub fn collect_files(root: &Path) -> Vec<PathBuf> {
    WalkBuilder::new(root)
        .hidden(true)
        .git_ignore(true)
        .git_global(false)
        .filter_entry(|entry| {
            let Some(name) = entry.file_name().to_str() else {
                return false;
            };
            !is_ignored_dir(name)
        })
        .build()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .is_some_and(|file_type| file_type.is_file())
        })
        .map(|entry| entry.into_path())
        .filter(|path| is_scannable(path))
        .collect()
}
