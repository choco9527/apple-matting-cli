use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::matting::perform_matting;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchResult {
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchSummary {
    pub total: usize,
    pub succeeded: usize,
    pub failed: usize,
    pub results: Vec<BatchResult>,
}

pub fn perform_batch(
    input_dir: &str,
    output_dir: &str,
    crop_to_subject: bool,
    recursive: bool,
    jobs: usize,
) -> Result<BatchSummary, String> {
    let items = plan_batch(Path::new(input_dir), Path::new(output_dir), recursive)?;
    let results = execute_items(items, jobs, |item| {
        let parent = item
            .output_path
            .parent()
            .ok_or_else(|| "Could not resolve output directory".to_string())?;
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Could not create output directory {}: {error}",
                parent.display()
            )
        })?;

        let input = path_string(&item.input_path)?;
        let output = path_string(&item.output_path)?;
        perform_matting(&input, Some(&output), crop_to_subject)
            .map(|_| ())
            .map_err(String::from)
    });

    let failed = results
        .iter()
        .filter(|result| result.error.is_some())
        .count();
    Ok(BatchSummary {
        total: results.len(),
        succeeded: results.len() - failed,
        failed,
        results,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BatchItem {
    input_path: PathBuf,
    output_path: PathBuf,
}

fn plan_batch(
    input_dir: &Path,
    output_dir: &Path,
    recursive: bool,
) -> Result<Vec<BatchItem>, String> {
    let input_root = canonical_directory(input_dir, "input")?;
    fs::create_dir_all(output_dir).map_err(|error| {
        format!(
            "Could not create output directory {}: {error}",
            output_dir.display()
        )
    })?;
    let output_root = canonical_directory(output_dir, "output")?;

    if output_root == input_root || output_root.starts_with(&input_root) {
        return Err("Output directory must be outside the input directory".to_string());
    }

    let mut files = Vec::new();
    collect_images(&input_root, recursive, &mut files)?;
    files.sort();
    if files.is_empty() {
        return Err("No supported images found in input directory".to_string());
    }

    build_items(&input_root, &output_root, files)
}

fn canonical_directory(path: &Path, label: &str) -> Result<PathBuf, String> {
    let canonical = fs::canonicalize(path).map_err(|error| {
        format!(
            "Could not open {label} directory {}: {error}",
            path.display()
        )
    })?;
    if !canonical.is_dir() {
        return Err(format!(
            "{label} path is not a directory: {}",
            path.display()
        ));
    }
    Ok(canonical)
}

fn collect_images(
    directory: &Path,
    recursive: bool,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("Could not read directory {}: {error}", directory.display()))?;

    for entry in entries {
        let entry = entry.map_err(|error| format!("Could not read directory entry: {error}"))?;
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Could not inspect {}: {error}", entry.path().display()))?;
        if file_type.is_file() && is_supported_image(&entry.path()) {
            files.push(entry.path());
        } else if recursive && file_type.is_dir() {
            collect_images(&entry.path(), true, files)?;
        }
    }
    Ok(())
}

fn is_supported_image(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("jpg" | "jpeg" | "png" | "webp" | "bmp")
    )
}

fn build_items(
    input_root: &Path,
    output_root: &Path,
    files: Vec<PathBuf>,
) -> Result<Vec<BatchItem>, String> {
    let mut outputs = HashMap::new();
    let mut items = Vec::with_capacity(files.len());

    for input_path in files {
        let relative = input_path
            .strip_prefix(input_root)
            .map_err(|_| format!("Image is outside input directory: {}", input_path.display()))?;
        let mut output_relative = relative.to_path_buf();
        output_relative.set_extension("png");
        let output_path = output_root.join(output_relative);

        if let Some(existing) = outputs.insert(output_path.clone(), input_path.clone()) {
            return Err(format!(
                "Output collision: {} and {} both map to {}",
                existing.display(),
                input_path.display(),
                output_path.display()
            ));
        }
        items.push(BatchItem {
            input_path,
            output_path,
        });
    }
    Ok(items)
}

fn execute_items<F>(items: Vec<BatchItem>, jobs: usize, processor: F) -> Vec<BatchResult>
where
    F: Fn(&BatchItem) -> Result<(), String> + Sync,
{
    let worker_count = jobs.clamp(1, items.len());
    let queue = Arc::new(Mutex::new(VecDeque::from(items)));
    let results = Arc::new(Mutex::new(Vec::new()));

    thread::scope(|scope| {
        for _ in 0..worker_count {
            let queue = Arc::clone(&queue);
            let results = Arc::clone(&results);
            let processor = &processor;
            scope.spawn(move || loop {
                let item = queue.lock().expect("batch queue poisoned").pop_front();
                let Some(item) = item else { break };
                let error = processor(&item).err();
                results
                    .lock()
                    .expect("batch results poisoned")
                    .push(BatchResult {
                        input_path: item.input_path,
                        output_path: item.output_path,
                        error,
                    });
            });
        }
    });

    let mut results = Arc::try_unwrap(results)
        .expect("batch workers still hold results")
        .into_inner()
        .expect("batch results poisoned");
    results.sort_by(|left, right| left.input_path.cmp(&right.input_path));
    results
}

fn path_string(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("Path is not valid UTF-8: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tempfile::tempdir;

    use super::{execute_items, plan_batch, BatchItem};

    #[test]
    fn plans_only_top_level_supported_images_by_default() {
        let temp = tempdir().unwrap();
        let input = temp.path().join("input");
        let output = temp.path().join("output");
        fs::create_dir_all(input.join("nested")).unwrap();
        fs::write(input.join("one.jpg"), b"image").unwrap();
        fs::write(input.join("ignore.txt"), b"text").unwrap();
        fs::write(input.join("nested/two.png"), b"image").unwrap();

        let items = plan_batch(&input, &output, false).unwrap();

        assert_eq!(items.len(), 1);
        assert!(items[0].output_path.ends_with("output/one.png"));
    }

    #[test]
    fn recursive_plan_preserves_relative_directories() {
        let temp = tempdir().unwrap();
        let input = temp.path().join("input");
        let output = temp.path().join("output");
        fs::create_dir_all(input.join("nested")).unwrap();
        fs::write(input.join("nested/two.webp"), b"image").unwrap();

        let items = plan_batch(&input, &output, true).unwrap();

        assert_eq!(items.len(), 1);
        assert!(items[0].output_path.ends_with("output/nested/two.png"));
    }

    #[test]
    fn rejects_output_directory_inside_input_directory() {
        let temp = tempdir().unwrap();
        let input = temp.path().join("input");
        let output = input.join("output");
        fs::create_dir_all(&input).unwrap();
        fs::write(input.join("one.jpg"), b"image").unwrap();

        let error = plan_batch(&input, &output, true).unwrap_err();

        assert_eq!(
            error,
            "Output directory must be outside the input directory"
        );
    }

    #[test]
    fn rejects_output_name_collisions() {
        let temp = tempdir().unwrap();
        let input = temp.path().join("input");
        let output = temp.path().join("output");
        fs::create_dir_all(&input).unwrap();
        fs::write(input.join("same.jpg"), b"image").unwrap();
        fs::write(input.join("same.png"), b"image").unwrap();

        let error = plan_batch(&input, &output, false).unwrap_err();

        assert!(error.starts_with("Output collision:"));
    }

    #[test]
    fn workers_continue_after_an_item_fails() {
        let calls = AtomicUsize::new(0);
        let items = (0..4)
            .map(|index| BatchItem {
                input_path: format!("input-{index}.jpg").into(),
                output_path: format!("output-{index}.png").into(),
            })
            .collect();

        let results = execute_items(items, 3, |item| {
            calls.fetch_add(1, Ordering::SeqCst);
            if item.input_path.ends_with("input-2.jpg") {
                Err("failed".to_string())
            } else {
                Ok(())
            }
        });

        assert_eq!(calls.load(Ordering::SeqCst), 4);
        assert_eq!(
            results
                .iter()
                .filter(|result| result.error.is_some())
                .count(),
            1
        );
    }
}
