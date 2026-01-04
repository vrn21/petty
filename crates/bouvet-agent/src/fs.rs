//! File system operations for bouvet-agent.
//!
//! Provides functions to read, write, and list files/directories.

use crate::protocol::FileEntry;
use std::fs;
use std::path::Path;

/// Maximum file size for read_file (10 MB).
/// Prevents memory exhaustion from reading huge files.
const MAX_READ_SIZE: u64 = 10 * 1024 * 1024;

/// Read the contents of a file.
///
/// # Arguments
/// * `path` - Path to the file to read.
///
/// # Returns
/// The file contents as a string, or an error message.
/// Files larger than 10MB will be rejected.
pub fn read_file(path: &str) -> Result<String, String> {
    // Check file size first
    let metadata = fs::metadata(path).map_err(|e| format!("failed to stat '{}': {}", path, e))?;

    if metadata.len() > MAX_READ_SIZE {
        return Err(format!(
            "file '{}' is too large ({} bytes, max {} bytes)",
            path,
            metadata.len(),
            MAX_READ_SIZE
        ));
    }

    fs::read_to_string(path).map_err(|e| format!("failed to read '{}': {}", path, e))
}

/// Write content to a file.
///
/// Creates parent directories if they don't exist.
///
/// # Arguments
/// * `path` - Path to the file to write.
/// * `content` - Content to write.
///
/// # Returns
/// `true` on success, or an error message.
pub fn write_file(path: &str, content: &str) -> Result<bool, String> {
    // Create parent directories if needed
    if let Some(parent) = Path::new(path).parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create directories for '{}': {}", path, e))?;
        }
    }

    fs::write(path, content)
        .map(|_| true)
        .map_err(|e| format!("failed to write '{}': {}", path, e))
}

/// List contents of a directory.
///
/// # Arguments
/// * `path` - Path to the directory to list.
///
/// # Returns
/// A vector of `FileEntry` items, or an error message.
pub fn list_dir(path: &str) -> Result<Vec<FileEntry>, String> {
    let entries =
        fs::read_dir(path).map_err(|e| format!("failed to read directory '{}': {}", path, e))?;

    let mut result = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("failed to read entry: {}", e))?;

        let metadata = entry
            .metadata()
            .map_err(|e| format!("failed to get metadata: {}", e))?;

        let name = entry.file_name().to_string_lossy().into_owned();

        result.push(FileEntry {
            name,
            is_dir: metadata.is_dir(),
            size: if metadata.is_file() {
                metadata.len()
            } else {
                0
            },
        });
    }

    // Sort by name for consistent output
    result.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_dir() -> PathBuf {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("bouvet-agent-test-{}-{}", std::process::id(), id));
        // Clean up any existing directory first
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_write_and_read_file() {
        let dir = temp_dir();
        let path = dir.join("test.txt");
        let path_str = path.to_str().unwrap();

        let content = "Hello, bouvet-agent!";
        assert!(write_file(path_str, content).is_ok());
        assert_eq!(read_file(path_str).unwrap(), content);

        // Cleanup
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_write_creates_parent_dirs() {
        let dir = temp_dir();
        let path = dir.join("nested/dirs/test.txt");
        let path_str = path.to_str().unwrap();

        assert!(write_file(path_str, "content").is_ok());
        assert!(path.exists());

        // Cleanup
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn test_read_nonexistent_file() {
        let result = read_file("/nonexistent/path/file.txt");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed to"));
    }

    #[test]
    fn test_list_dir() {
        let dir = temp_dir();
        fs::write(dir.join("file1.txt"), "content").unwrap();
        fs::write(dir.join("file2.txt"), "content").unwrap();
        fs::create_dir(dir.join("subdir")).unwrap();

        let entries = list_dir(dir.to_str().unwrap()).unwrap();
        assert_eq!(entries.len(), 3);

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"file1.txt"));
        assert!(names.contains(&"file2.txt"));
        assert!(names.contains(&"subdir"));

        // Check is_dir flag
        let subdir = entries.iter().find(|e| e.name == "subdir").unwrap();
        assert!(subdir.is_dir);

        // Cleanup
        fs::remove_dir_all(dir).ok();
    }
}
