# files-ingest

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
Concatenate a directory full of files into a single prompt for use with LLMs. This is a Rust implementation inspired by Simon Willison's Python tool [files-to-prompt](https://github.com/simonw/files-to-prompt).

## Installation / Building

You need Rust installed (`rustup` is recommended). You can build the tool using Cargo:

1.  **Clone the repository (or download the source):**

    ```bash
    git clone https://github.com/jdambron/files-ingest.git # Replace with your repo URL
    cd files-ingest
    ```

2.  **Build the project:**
    ```bash
    cargo build
    ```
    For an optimized release build:
    ```bash
    cargo build --release
    ```

The executable will be located at `target/debug/files-ingest` (for debug builds) or `target/release/files-ingest` (for release builds). You can copy this executable to a location in your `$PATH` for easier access.

## Usage

Run the tool by providing paths to files or directories:

```bash
# Using the debug build:
./target/debug/files-ingest path/to/file_or_directory [path/to/another ...]

# Using the release build:
./target/release/files-ingest path/to/file_or_directory [path/to/another ...]

# If you copied it to your PATH:
files-ingest path/to/file_or_directory [path/to/another ...]
```

This will output the contents of every file found recursively, with each file preceded by its relative path and separated by `---` (by default).

### Options

- `-e, --extension <EXT>`: Only include files with the specified extension. Can be used multiple times (e.g., `-e rs -e toml`).
- `--include-hidden`: Include files and folders starting with `.` (hidden files and directories). By default, they are ignored.
- `--ignore <PATTERN>`: Specify one or more gitignore-style patterns to ignore files or directories. Can be used multiple times (e.g., `--ignore "*.log"` `--ignore "temp/"`).
- `--ignore-files-only`: When set, `--ignore` patterns only match against filenames, not directory names during traversal.
- `--ignore-gitignore`: Ignore rules found in `.gitignore` files. By default, `.gitignore` files are respected.
- `-c, --cxml`: Output in Claude XML format.
- `-m, --markdown`: Output as Markdown with fenced code blocks (language guessed from extension).
- `-n, --line-numbers`: Include line numbers in the output.
- `-o, --output <FILE>`: Write the output to the specified file instead of printing to the console (stdout).
- `-0, --null`: Use NUL character (`\0`) as separator when reading paths from stdin (useful for filenames with spaces/newlines piped from `find ... -print0`).
- `--help`: Show help message and exit.
- `--version`: Show version information and exit.

### Reading from stdin

The tool can read paths from standard input if no paths are provided as arguments. This allows piping from other commands like `find`:

```bash
# Find Rust files modified in the last day
find . -name "*.rs" -mtime -1 -print | ./target/debug/files-ingest

# Use NUL separator with find -print0 and the -0 flag
find . -name "*.txt" -print0 | ./target/debug/files-ingest -0

# Mix arguments and stdin (processes README.md and files from find)
find . -name "*.toml" -print | ./target/debug/files-ingest README.md
```

### Output Formats

**Default:**

```
path/to/file1.txt
---
Contents of file1.txt
---
path/to/subdir/file2.rs
---
Contents of file2.rs
---

```

**Claude XML (`--cxml`):**

```xml
<documents>
<document index="1">
<source>path/to/file1.txt</source>
<document_content>
Contents of file1.txt
</document_content>
</document>
<document index="2">
<source>path/to/subdir/file2.rs</source>
<document_content>
Contents of file2.rs
</document_content>
</document>
</documents>
```

**Markdown (`--markdown`):**

`````
path/to/file1.txt
```
Contents of file1.txt
```

path/to/subdir/file2.rs
```rust
Contents of file2.rs
```

path/to/other.md
````markdown
File with ``` backticks
````

`````

**Line Numbers (`-n`):**

Prepends line numbers to the content in any format:

```
path/to/file.rs
---
1  use std::io;
2
3  fn main() {
4      println!("Hello");
5  }
---

```

## Development

Build the project using `cargo build`. Run tests (if any are added) with `cargo test`. Format the code with `cargo fmt`. Lint with `cargo clippy`.

## License

This project is licensed under the Apache License, Version 2.0 - see the [LICENSE](LICENSE) file for details.
