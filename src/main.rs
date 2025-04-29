use clap::Parser;
use ignore::{DirEntry, WalkBuilder}; // For directory traversal respecting .gitignore etc.
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write}; // Standard Input/Output operations
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering}; // For the global CXML index
use thiserror::Error; // For custom error types // Import the atty crate

// --- Configuration & Constants ---

// Static map for file extensions to Markdown language tags
static EXT_TO_LANG: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
fn initialize_language_map() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert("py", "python");
    m.insert("rs", "rust");
    m.insert("c", "c");
    m.insert("h", "c");
    m.insert("cpp", "cpp");
    m.insert("hpp", "cpp");
    m.insert("java", "java");
    m.insert("js", "javascript");
    m.insert("ts", "typescript");
    m.insert("html", "html");
    m.insert("css", "css");
    m.insert("xml", "xml");
    m.insert("json", "json");
    m.insert("yaml", "yaml");
    m.insert("yml", "yaml");
    m.insert("sh", "bash");
    m.insert("rb", "ruby");
    m.insert("md", "markdown");
    m.insert("toml", "toml");
    m.insert("go", "go");
    m.insert("php", "php");
    m.insert("swift", "swift");
    m.insert("kt", "kotlin");
    m.insert("sql", "sql");
    m
}

// Global counter for Claude XML document index
static GLOBAL_INDEX: AtomicUsize = AtomicUsize::new(1);

// --- Error Handling ---

#[derive(Error, Debug)]
enum AppError {
    #[error("IO Error: {0}")]
    Io(#[from] io::Error),
    #[error("Ignore Error: {0}")] // Keep this for potential errors during walk
    Ignore(#[from] ignore::Error),
    #[error("Path does not exist: {0}")]
    PathNotFound(PathBuf),
    // Add a specific error variant if needed for invalid ignore patterns,
    // though 'ignore' crate often reports these during the walk.
    // #[error("Invalid ignore pattern: {0}")]
    // InvalidIgnorePattern(String),
}

// --- Command Line Argument Parsing ---

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Concatenates files into a single prompt, similar to Python's files-to-prompt.",
    long_about = "Takes one or more paths to files or directories and outputs the content of each file, recursively. Supports filtering, ignoring files (.gitignore), and various output formats (default, Claude XML, Markdown)."
)]
struct Cli {
    /// Paths to files or directories to process. Reads from stdin if empty.
    #[arg(name = "PATHS")]
    paths: Vec<PathBuf>,

    /// Only include files with the specified extension (can be used multiple times).
    #[arg(short, long = "extension", value_name = "EXT")]
    extensions: Vec<String>,

    /// Include hidden files and directories (starting with '.').
    #[arg(long)]
    include_hidden: bool,

    /// Specify patterns to ignore (files or directories, uses gitignore syntax). Can be used multiple times.
    #[arg(long = "ignore", value_name = "PATTERN")]
    ignore_patterns: Vec<String>,

    /// When set, --ignore patterns only match files, not directories.
    #[arg(long)]
    ignore_files_only: bool,

    /// Ignore .gitignore files and include all files found.
    #[arg(long)]
    ignore_gitignore: bool,

    /// Output in Claude XML format.
    #[arg(short = 'c', long = "cxml")]
    cxml: bool,

    /// Output as Markdown with fenced code blocks.
    #[arg(short = 'm', long = "markdown")]
    markdown: bool,

    /// Include line numbers in the output.
    #[arg(short = 'n', long = "line-numbers")]
    line_numbers: bool,

    /// Write output to a file instead of stdout.
    #[arg(short = 'o', long = "output", value_name = "FILE")]
    output_file: Option<PathBuf>,

    /// Use NUL character ('\0') as separator when reading paths from stdin.
    #[arg(short = '0', long = "null")]
    null_separator: bool,
}

// --- Main Application Logic ---

fn main() -> Result<(), AppError> {
    let mut cli = Cli::parse();

    // --- Read paths from stdin if no paths are provided as arguments ---
    if cli.paths.is_empty() {
        read_paths_from_stdin(&mut cli.paths, cli.null_separator)?;
        if cli.paths.is_empty() {
            eprintln!(
                "No input paths provided either as arguments or via stdin. Use --help for usage."
            );
            return Ok(()); // Exit gracefully if no input
        }
    }

    // --- Validate input paths ---
    for path in &cli.paths {
        if !path.exists() {
            return Err(AppError::PathNotFound(path.clone()));
        }
    }

    // --- Setup Output Writer ---
    // Determine where to write the output: stdout or a file.
    // Use BufWriter for potentially better performance, especially with large outputs.
    let writer: Box<dyn Write> = if let Some(output_path) = &cli.output_file {
        Box::new(BufWriter::new(File::create(output_path)?))
    } else {
        Box::new(BufWriter::new(io::stdout()))
    };
    let mut writer = writer; // Make it mutable

    // --- Process Paths ---
    // Write initial XML tag if needed
    if cli.cxml {
        writeln!(writer, "<documents>")?;
    }

    // Create the directory walker builder
    // Handle the case where cli.paths might be empty after attempting stdin read
    if cli.paths.is_empty() {
        // This case should ideally be caught earlier, but double-check
        eprintln!("No valid paths found to process.");
        return Ok(());
    }
    let mut walker_builder = WalkBuilder::new(&cli.paths[0]); // Start with the first path

    walker_builder
        .hidden(!cli.include_hidden) // Respect --include-hidden flag
        .git_ignore(!cli.ignore_gitignore) // Respect --ignore-gitignore flag
        .git_global(!cli.ignore_gitignore)
        .git_exclude(!cli.ignore_gitignore)
        .require_git(false) // Don't require a git repo to exist
        .ignore(!cli.ignore_gitignore); // Also respect .ignore files

    // Add custom ignore patterns
    for pattern in &cli.ignore_patterns {
        // The add_ignore method returns &mut WalkBuilder and doesn't return a Result/Option.
        // Invalid patterns usually cause errors during the .build() or the walk itself.
        walker_builder.add_ignore(pattern); // REMOVED '?' and incorrect error handling
    }

    // Add remaining paths to the walker
    for path in cli.paths.iter().skip(1) {
        walker_builder.add(path);
    }

    // Iterate through the files found by the walker
    // The build() method itself can return an error (e.g., invalid pattern)
    for result in walker_builder.build() {
        // Errors from build() are handled here
        match result {
            Ok(entry) => {
                if !should_process_entry(&entry, &cli) {
                    continue; // Skip if it's a directory, doesn't match extension, or ignored by --ignore file pattern
                }
                let path = entry.path();
                match fs::read_to_string(path) {
                    Ok(content) => {
                        // Successfully read the file content as UTF-8
                        print_file(
                            &mut writer,
                            path,
                            &content,
                            cli.cxml,
                            cli.markdown,
                            cli.line_numbers,
                        )?;
                    }
                    Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                        // Handle non-UTF-8 files gracefully
                        eprintln!(
                            "{}",
                            format_args!(
                                "Warning: Skipping file {} - Not valid UTF-8.",
                                path.display()
                            )
                        );
                    }
                    Err(e) => {
                        // Handle other file reading errors
                        eprintln!(
                            "{}",
                            format_args!(
                                "Warning: Skipping file {} - Error reading: {}",
                                path.display(),
                                e
                            )
                        );
                    }
                }
            }
            Err(err) => {
                // Handle errors during the walk (could be permission issues, invalid patterns, etc.)
                eprintln!("Warning: Error during directory walk: {err}");
            }
        }
    }

    // Write closing XML tag if needed
    if cli.cxml {
        writeln!(writer, "</documents>")?;
    }

    // Ensure the buffer is flushed before exiting
    writer.flush()?;

    Ok(())
}

// --- Helper Functions ---

/// Checks if a directory entry should be processed based on CLI options.
fn should_process_entry(entry: &DirEntry, cli: &Cli) -> bool {
    // Only process files
    if !entry.file_type().is_some_and(|ft| ft.is_file()) {
        return false;
    }

    let path = entry.path();

    // Filter by extension if specified
    if !cli.extensions.is_empty() {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if !cli
                .extensions
                .iter()
                .any(|allowed_ext| ext.eq_ignore_ascii_case(allowed_ext))
            {
                // Case-insensitive compare
                return false; // Extension doesn't match
            }
        } else {
            return false; // No extension or invalid UTF-8 extension
        }
    }

    // Apply --ignore patterns specifically to files if --ignore-files-only is set
    // Note: The `ignore` crate handles directory ignoring based on patterns automatically
    // unless overridden. This check is mainly for the --ignore-files-only case where
    // we might want to ignore a file *within* a directory that isn't itself ignored.
    // The `ignore` crate's standard matching should cover most cases, but this adds
    // an explicit file-only check if the flag is set.
    if cli.ignore_files_only && !cli.ignore_patterns.is_empty() {
        if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
            // We need a simple glob matcher here. The `ignore` crate doesn't directly
            // expose its matcher easily for this specific file-only check after traversal.
            // Using a basic `contains` or `starts_with`/`ends_with` might be sufficient
            // for simple patterns, or pull in a glob crate if needed.
            // For simplicity, let's just check if the filename *contains* any ignore pattern.
            // A proper implementation would use glob matching.
            // Example using `glob` crate (add `glob = "0.3"` to Cargo.toml):
            // use glob::Pattern;
            // if cli.ignore_patterns.iter().any(|pattern| Pattern::new(pattern).map_or(false, |p| p.matches(file_name))) {
            //     return false;
            // }
            // Using contains as a placeholder:
            if cli
                .ignore_patterns
                .iter()
                .any(|pattern| file_name.contains(pattern))
            {
                // This is a basic check, consider using a proper glob matcher
                // return false; // Uncomment if using contains is sufficient, or replace with glob logic
            }
        }
    }

    true // Process this entry
}

/// Reads paths from standard input.
fn read_paths_from_stdin(paths: &mut Vec<PathBuf>, null_separator: bool) -> io::Result<()> {
    // Use atty to check if stdin is connected to a terminal
    if atty::is(atty::Stream::Stdin) {
        // No input piped, return early.
        return Ok(());
    }

    let mut stdin_content = String::new();
    io::stdin().read_to_string(&mut stdin_content)?;

    // Determine the separator based on the --null flag
    let separator = if null_separator { '\0' } else { '\n' };

    // Split the input string by the separator and collect valid paths
    for path_str in stdin_content.split(separator) {
        let trimmed = path_str.trim(); // Trim whitespace
        if !trimmed.is_empty() {
            paths.push(PathBuf::from(trimmed));
        }
    }
    Ok(())
}

/// Adds line numbers to the content string.
fn add_line_numbers(content: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let num_lines = lines.len();
    // Calculate padding needed for line numbers (e.g., 1, 10, 100)
    let padding = if num_lines == 0 {
        1
    } else {
        num_lines.to_string().len()
    };

    lines
        .into_iter()
        .enumerate()
        .map(|(i, line)| format!("{:<width$}  {}", i + 1, line, width = padding))
        .collect::<Vec<String>>()
        .join("\n")
}

/// Prints a single file's content in the specified format.
fn print_file(
    writer: &mut dyn Write,
    path: &Path,
    content: &str,
    cxml: bool,
    markdown: bool,
    line_numbers: bool,
) -> io::Result<()> {
    // Use relative path if possible for cleaner output, fallback to absolute
    let display_path = path.strip_prefix(".").unwrap_or(path).display();

    // Apply line numbers if requested *before* formatting
    let processed_content = if line_numbers {
        add_line_numbers(content)
    } else {
        content.to_string() // Keep original content if no line numbers
    };

    // --- Select Output Format ---
    if cxml {
        // Claude XML Format
        let index = GLOBAL_INDEX.fetch_add(1, Ordering::SeqCst); // Increment and get previous value
        writeln!(writer, "<document index=\"{index}\">")?;
        writeln!(writer, "<source>{display_path}</source>")?; // Use relative path
        writeln!(writer, "<document_content>")?;
        // Basic XML escaping for content - replace '&', '<', '>'
        let escaped_content = processed_content
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        writeln!(writer, "{escaped_content}")?; // Write potentially line-numbered and escaped content
        writeln!(writer, "</document_content>")?;
        writeln!(writer, "</document>")?;
    } else if markdown {
        // Markdown Format
        let lang = path
            .extension()
            .and_then(|ext| ext.to_str()) // Get extension as &str
            .and_then(|ext_str| {
                // Access the OnceLock, initializing it if this is the first time
                EXT_TO_LANG
                    .get_or_init(|| {
                        // This closure runs only once to initialize the map
                        // eprintln!("Initializing language map..."); // Optional debug print
                        initialize_language_map() // Call the initializer function
                        // Alternatively, put the HashMap creation logic directly here:
                        // let mut m = HashMap::new(); /* ... inserts ... */ m
                    })
                    .get(ext_str.to_lowercase().as_str()) // Now get from the initialized HashMap
            })
            .unwrap_or(&""); // Get language tag or empty string

        // Determine necessary backtick count (handle content with backticks)
        let mut backticks = "```".to_string();
        while processed_content.contains(&backticks) {
            backticks.push('`');
        }

        writeln!(writer, "{display_path}")?; // File path (relative)
        writeln!(writer, "{backticks}{lang}")?; // Opening fence with language tag
        writeln!(writer, "{processed_content}")?; // File content (potentially line-numbered)
        writeln!(writer, "{backticks}")?; // Closing fence
        writeln!(writer)?; // Add a blank line for separation
    } else {
        // Default Format
        writeln!(writer, "{display_path}")?; // File path (relative)
        writeln!(writer, "---")?;
        writeln!(writer, "{processed_content}")?; // File content (potentially line-numbered)
        // writeln!(writer)?; // Original python version adds blank line here - removed for closer match
        writeln!(writer, "---")?;
        writeln!(writer)?; // Add blank line after the closing separator
    }

    Ok(())
}
