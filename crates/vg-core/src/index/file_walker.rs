use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use ignore::WalkBuilder;

pub fn collect_files(roots: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = HashSet::new();
    for root in roots {
        walk_root(root, &mut files)?;
    }
    let mut output = files.into_iter().collect::<Vec<_>>();
    output.sort();
    Ok(output)
}

fn walk_root(root: &Path, files: &mut HashSet<PathBuf>) -> Result<()> {
    if root.is_file() {
        files.insert(
            root.canonicalize()
                .with_context(|| format!("解析文件路径失败: {}", root.display()))?,
        );
        return Ok(());
    }

    let walker = WalkBuilder::new(root)
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .build();

    for entry in walker {
        let entry = entry.with_context(|| format!("遍历目录失败: {}", root.display()))?;
        if entry
            .file_type()
            .map(|file_type| file_type.is_file())
            .unwrap_or(false)
        {
            files.insert(
                entry
                    .path()
                    .canonicalize()
                    .with_context(|| format!("解析文件路径失败: {}", entry.path().display()))?,
            );
        }
    }

    Ok(())
}
