use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::cli::OutputOptions;

pub fn validate_output_options(inputs: &[PathBuf], output: &OutputOptions) -> Result<()> {
    if output.output.is_some() && output.output_dir.is_some() {
        bail!("use either --output or --output-dir, not both");
    }

    if output.output.is_some() && inputs.len() > 1 {
        bail!("--output can only be used with a single input file");
    }

    Ok(())
}

pub fn build_output_path(
    input: &Path,
    output: &OutputOptions,
    suffix: &str,
    extension: &str,
) -> Result<PathBuf> {
    if let Some(path) = &output.output {
        return Ok(path.clone());
    }

    let filename = format!("{}{suffix}.{extension}", file_stem(input));
    Ok(if let Some(dir) = &output.output_dir {
        dir.join(filename)
    } else {
        input.with_file_name(filename)
    })
}

pub fn build_segment_output_path(
    input: &Path,
    output: &OutputOptions,
    index: usize,
    extension: &str,
) -> Result<PathBuf> {
    if let Some(path) = &output.output {
        if index == 0 {
            return Ok(path.clone());
        }
        bail!("--output can only be used with a single cut segment");
    }

    let filename = format!("{}.part{:02}.{extension}", file_stem(input), index + 1);
    Ok(if let Some(dir) = &output.output_dir {
        dir.join(filename)
    } else {
        input.with_file_name(filename)
    })
}

pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    Ok(())
}

fn file_stem(path: &Path) -> &str {
    path.file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("output")
}
