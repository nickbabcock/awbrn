use std::cmp::Ordering;
use std::env;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

fn main() {
    let args: Vec<String> = env::args().collect();

    // Check if directory argument is provided
    if args.len() < 2 {
        eprintln!("Usage: {} <directory>", args[0]);
        std::process::exit(1);
    }

    let directory = PathBuf::from(&args[1]);

    // Collect all files by category: clear, snow, rain
    let mut clear_files = Vec::new();
    let mut snow_files = Vec::new();
    let mut rain_files = Vec::new();

    for entry in WalkDir::new(&directory)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path().to_owned();
        let filename = path.file_name().unwrap_or_default().to_string_lossy();

        // Categorize the file based on its name
        if filename.contains("_Snow") {
            snow_files.push(path);
        } else if filename.contains("_Rain") {
            rain_files.push(path);
        } else {
            clear_files.push(path);
        }
    }

    // Sort each category using path-aware natural sort logic
    clear_files.sort_by(|a, b| path_natural_sort(a, b));
    snow_files.sort_by(|a, b| path_natural_sort(a, b));
    rain_files.sort_by(|a, b| path_natural_sort(a, b));

    // Print files in the desired order: clear, snow, rain
    let stubby = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/textures/stubby.png");
    println!("{}", stubby.canonicalize().unwrap().display());
    print_files(&directory, &clear_files);

    let stubby_snow =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/textures/stubby-snow.png");
    println!("{}", stubby_snow.canonicalize().unwrap().display());
    print_files(&directory, &snow_files);

    print_files(&directory, &rain_files);
}

fn print_files(base_dir: &Path, files: &[PathBuf]) {
    for file in files {
        if let Ok(relative_path) = file.strip_prefix(base_dir) {
            println!("{}", relative_path.display());
        } else {
            // Fallback to just the filename if stripping the prefix fails
            if let Some(filename) = file.file_name() {
                println!("{}", filename.to_string_lossy());
            }
        }
    }
}

fn path_natural_sort(a: &Path, b: &Path) -> Ordering {
    // Count path components to determine nesting depth
    let a_components: Vec<_> = a.components().collect();
    let b_components: Vec<_> = b.components().collect();

    // First sort by directory depth - shallower directories come first
    match a_components.len().cmp(&b_components.len()) {
        Ordering::Equal => {
            // If paths have the same depth, compare parent directories
            match a.parent().cmp(&b.parent()) {
                Ordering::Equal => {
                    // If directories are identical, compare filenames
                    natural_sort(a, b)
                }
                ordering => ordering,
            }
        }
        ordering => ordering,
    }
}

fn natural_sort(a: &Path, b: &Path) -> Ordering {
    let a_name = a.file_name().unwrap_or_default().to_string_lossy();
    let b_name = b.file_name().unwrap_or_default().to_string_lossy();

    natural_sort_str(&a_name, &b_name)
}

fn natural_sort_str(a: &str, b: &str) -> Ordering {
    // Extract filenames without extensions for easier comparison
    let Some((a_filename, _)) = a.rsplit_once('.') else {
        return a.cmp(b);
    };

    let Some((b_filename, _)) = b.rsplit_once('.') else {
        return a.cmp(b);
    };

    if let (Some(a_prefix), Some(b_prefix)) =
        (a_filename.split('_').next(), b_filename.split('_').next())
    {
        let cmp = a_prefix.cmp(b_prefix);
        if cmp != Ordering::Equal {
            return cmp;
        }
    }

    // Handle number sorting for filenames with numbers after hyphens
    // Example: "HQ_Rain-10.png" vs "HQ_Rain-2.png"
    let Some((_, a_num)) = a_filename.rsplit_once('-') else {
        return a.cmp(b);
    };

    let Some((_, b_num)) = b_filename.rsplit_once('-') else {
        return a.cmp(b);
    };

    if let (Ok(a_num), Ok(b_num)) = (a_num.parse::<u64>(), b_num.parse::<u64>()) {
        return a_num.cmp(&b_num);
    }

    // Fall back to standard text comparison
    a.cmp(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_natural_sort_numbers() {
        // Test that numeric sorting works correctly
        assert_eq!(
            natural_sort_str("HQ_Rain-1.png", "HQ_Rain-2.png"),
            Ordering::Less
        );

        assert_eq!(
            natural_sort_str("HQ_Rain-2.png", "HQ_Rain-10.png"),
            Ordering::Less
        );

        assert_eq!(
            natural_sort_str("HQ_Rain-10.png", "HQ_Rain-2.png"),
            Ordering::Greater
        );
    }

    #[test]
    fn test_sort_mixed_content() {
        // Test the specific examples from the requirements
        assert_eq!(natural_sort_str("ES_Snow.png", "ESW.png"), Ordering::Less);

        assert_eq!(
            natural_sort_str("ESW.png", "ES_Snow.png"),
            Ordering::Greater
        );
    }

    #[test]
    fn test_full_sort_order() {
        // Group 1: Clear files (without "_Snow" or "_Rain")
        let mut clear_files = vec!["ESW.png", "ES.png", "NEnd.png", "NE.png"];

        // Group 2: Snow files (with "_Snow")
        let mut snow_files = vec![
            "NEnd_Snow.png",
            "ES_Snow.png",
            "NE_Snow.png",
            "ESW_Snow.png",
        ];

        // Group 3: Rain files (with "_Rain")
        let mut rain_files = vec![
            "HQ_Rain-1.png",
            "ES_Rain.png",
            "HQ_Rain-0.png",
            "HQ_Rain-2.png",
            "ESW_Rain.png",
            "HQ_Rain-10.png",
            "HQ_Rain-11.png",
        ];

        // Sort each group using our natural sort function
        clear_files.sort_by(|a, b| natural_sort_str(a, b));
        snow_files.sort_by(|a, b| natural_sort_str(a, b));
        rain_files.sort_by(|a, b| natural_sort_str(a, b));

        // Define the expected order for each group
        let expected_clear = vec!["ES.png", "ESW.png", "NE.png", "NEnd.png"];

        let expected_snow = vec![
            "ES_Snow.png",
            "ESW_Snow.png",
            "NE_Snow.png",
            "NEnd_Snow.png",
        ];

        let expected_rain = vec![
            "ES_Rain.png",
            "ESW_Rain.png",
            "HQ_Rain-0.png",
            "HQ_Rain-1.png",
            "HQ_Rain-2.png",
            "HQ_Rain-10.png",
            "HQ_Rain-11.png",
        ];

        // Check that each group is sorted correctly
        assert_eq!(
            clear_files, expected_clear,
            "Clear files not sorted correctly"
        );
        assert_eq!(snow_files, expected_snow, "Snow files not sorted correctly");
        assert_eq!(rain_files, expected_rain, "Rain files not sorted correctly");
    }

    #[test]
    fn test_underscore_pattern() {
        // Test that files with underscores come before files with same prefix but no underscore
        assert_eq!(natural_sort_str("WN_Snow.png", "WNEnd.png"), Ordering::Less);

        assert_eq!(natural_sort_str("ES_Rain.png", "ESW.png"), Ordering::Less);

        assert_eq!(natural_sort_str("NE_Snow.png", "NEnd.png"), Ordering::Less);
    }

    #[test]
    fn test_path_natural_sort() {
        // Create paths with different directories
        let path1 = Path::new("/dir1/ES.png");
        let path2 = Path::new("/dir1/ES_Rain.png");
        let path3 = Path::new("/dir2/ES.png");

        // Test that directory sorting comes before filename sorting
        assert_eq!(path_natural_sort(path1, path3), Ordering::Less);

        assert_eq!(path_natural_sort(path3, path1), Ordering::Greater);

        // Test that within the same directory, filename sorting applies
        assert_eq!(path_natural_sort(path1, path2), Ordering::Less);

        assert_eq!(path_natural_sort(path2, path1), Ordering::Greater);

        // Test that directory sorting takes precedence over filename sorting
        // Even if the second filename would sort before the first filename in a natural sort,
        // the directory order is what matters
        assert_eq!(path_natural_sort(path2, path3), Ordering::Less);

        // Test that files in parent directories come before files in child directories
        let parent_file = Path::new("/parent/file.png");
        let child_file = Path::new("/parent/child/file.png");

        assert_eq!(path_natural_sort(parent_file, child_file), Ordering::Less);

        assert_eq!(
            path_natural_sort(child_file, parent_file),
            Ordering::Greater
        );
    }
}
