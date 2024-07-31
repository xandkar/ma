use std::{
    fs,
    path::{Path, PathBuf},
};

// TODO Switch from iterator to async stream?
pub fn find_files(path: &Path) -> impl Iterator<Item = PathBuf> {
    FilePaths::find(path)
}

struct FilePaths {
    frontier: Vec<PathBuf>,
}

impl FilePaths {
    fn find(root: &Path) -> Self {
        let mut frontier = Vec::new();
        frontier.push(root.to_path_buf());
        Self { frontier }
    }
}

impl Iterator for FilePaths {
    type Item = PathBuf;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(path) = self.frontier.pop() {
            match fs::metadata(&path) {
                Ok(meta) if meta.is_file() => {
                    return Some(path);
                }
                Ok(meta) if meta.is_dir() => match fs::read_dir(&path) {
                    Err(error) => {
                        tracing::error!(
                            ?path,
                            ?error,
                            "Failed to read directory",
                        );
                    }
                    Ok(entries) => {
                        for entry_result in entries {
                            match entry_result {
                                Ok(entry) => {
                                    self.frontier.push(entry.path());
                                }
                                Err(error) => {
                                    tracing::error!(
                                        from = ?path, ?error,
                                        "Failed to read an entry",
                                    );
                                }
                            }
                        }
                    }
                },
                Ok(meta) => {
                    tracing::debug!(
                        ?path,
                        ?meta,
                        "Neither file nor directory"
                    );
                }
                Err(error) => {
                    tracing::error!(
                        from = ?path, ?error,
                        "Failed to read metadata",
                    );
                }
            }
        }
        None
    }
}
