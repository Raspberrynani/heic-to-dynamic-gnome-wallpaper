// heic-to-dynamic-gnome-wallpaper
// Copyright (C) 2022 Johannes Wünsche
// Copyright (C) 2026 Raspberrynani
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
use anyhow::{Context, Result};
use colored::*;
use libheif_rs::HeifContext;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};

use clap::{Arg, ArgAction, Command};

mod image;
mod metadata;
mod schema;
mod serializer;
mod solar;
mod timebased;
mod util;

const INPUT: &str = "IMAGE";
const DIR: &str = "DIR";
const APPLY: &str = "APPLY";
const ZIP: &str = "ZIP";
const DAY_SECS: f32 = 86400.0;

fn main() -> Result<()> {
    let matches = Command::new("heic-to-dynamic-gnome-wallpaper")
        .arg(Arg::new(INPUT)
             .help("Image which should be transformed")
             .num_args(1)
             .value_name(INPUT)
             .required(true)
        )
        .arg(Arg::new(DIR)
             .help("Base directory where a wallpaper-specific output folder should be created.")
             .long_help("Specifies the base directory for generated wallpapers. A folder named after the input image will be created inside it. Default is $XDG_DATA_HOME/backgrounds or ~/.local/share/backgrounds.")
             .short('d')
             .long("dir")
             .num_args(1)
             .value_name(DIR)
        )
        .arg(Arg::new(APPLY)
             .help("Apply the generated wallpaper through GNOME settings after conversion.")
             .long("apply")
             .action(ArgAction::SetTrue)
        )
        .arg(Arg::new(ZIP)
             .help("Export the generated wallpaper folder as a zip file next to the folder.")
             .long("zip")
             .action(ArgAction::SetTrue)
        )
        .get_matches();
    let path = matches
        .get_one::<String>(INPUT)
        .map(String::as_str)
        .ok_or_else(|| anyhow::Error::msg("Could not read INPUT"))?;

    let image_name = Path::new(path)
        .file_stem()
        .ok_or_else(|| anyhow::Error::msg(format!("Could not get file name of path: {}", path)))?
        .to_string_lossy();
    let parent_directory = output_directory(
        matches.get_one::<String>(DIR).map(String::as_str),
        &image_name,
    )?;
    cleanup_generated_files(&parent_directory, &image_name)?;

    println!(
        "{}: Installing wallpaper files into {}",
        "Preparation".bright_blue(),
        parent_directory.display(),
    );

    let image_ctx = HeifContext::read_from_file(path)?;

    // FETCH file wide metadata
    println!(
        "{}: Fetch metadata from image...",
        "Preparation".bright_blue(),
    );
    let base64plist = metadata::get_wallpaper_metadata(&image_ctx)?
        .ok_or_else(|| anyhow::Error::msg("No valid metadata found describing wallpaper! Please check if the mime field is available and carries an apple_desktop:h24 and/or apple_desktop:solar value"))?;

    println!(
        "{}: Detecting wallpaper description type...",
        "Preparation".bright_blue(),
    );
    let xml_path = match base64plist {
        metadata::WallPaperMode::H24(content) => {
            println!(
                "{}: Detected time-based wallpaper.",
                "Preparation".bright_blue(),
            );
            timebased::compute_time_based_wallpaper(
                Path::new(path),
                image_ctx,
                content,
                &parent_directory,
                &image_name,
            )
        }
        metadata::WallPaperMode::Solar(content) => {
            println!(
                "{}: Detected solar-based wallpaper.",
                "Preparation".bright_blue(),
            );
            solar::compute_solar_based_wallpaper(
                Path::new(path),
                image_ctx,
                content,
                &parent_directory,
                &image_name,
            )
        }
    }?;

    if matches.get_flag(ZIP) {
        export_zip(&parent_directory)?;
    }

    if matches.get_flag(APPLY) {
        apply_gnome_wallpaper(&xml_path)?;
    }

    Ok(())
}

fn output_directory(base_dir: Option<&str>, image_name: &str) -> Result<PathBuf> {
    let base = match base_dir {
        Some(dir) => PathBuf::from(dir),
        None => user_backgrounds_directory()?,
    };
    let output = base.join(sanitize_filename(image_name));
    std::fs::create_dir_all(&output)
        .with_context(|| format!("Could not create output directory {}", output.display()))?;
    output.canonicalize().with_context(|| {
        format!(
            "Could not canonicalize output directory {}",
            output.display()
        )
    })
}

fn user_backgrounds_directory() -> Result<PathBuf> {
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        return Ok(PathBuf::from(data_home).join("backgrounds"));
    }

    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".local/share/backgrounds"))
        .ok_or_else(|| anyhow::Error::msg("Could not determine HOME for default install path"))
}

fn sanitize_filename(name: &str) -> String {
    let mut sanitized = String::with_capacity(name.len());
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
            sanitized.push(ch);
        } else {
            sanitized.push('_');
        }
    }

    let sanitized = sanitized.trim_matches('_');
    if sanitized.is_empty() {
        "dynamic-wallpaper".to_string()
    } else {
        sanitized.to_string()
    }
}

fn cleanup_generated_files(directory: &Path, image_name: &str) -> Result<()> {
    let xml_name = format!("{}.xml", image_name);
    for entry in std::fs::read_dir(directory)
        .with_context(|| format!("Could not read output directory {}", directory.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        let should_remove = file_name == xml_name
            || path.extension().and_then(|ext| ext.to_str()) == Some("png")
                && path
                    .file_stem()
                    .and_then(|stem| stem.to_str())
                    .is_some_and(|stem| stem.chars().all(|ch| ch.is_ascii_digit()));

        if should_remove {
            std::fs::remove_file(&path).with_context(|| {
                format!("Could not remove stale generated file {}", path.display())
            })?;
        }
    }

    Ok(())
}

fn export_zip(directory: &Path) -> Result<PathBuf> {
    let zip_path = directory.with_extension("zip");
    println!(
        "{}: Exporting wallpaper archive {}",
        "Installation".green(),
        zip_path.display()
    );

    let file = File::create(&zip_path)
        .with_context(|| format!("Could not create zip file {}", zip_path.display()))?;
    let writer = BufWriter::new(file);
    let mut zip = ZipWriter::new(writer);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

    let mut entries = std::fs::read_dir(directory)
        .with_context(|| format!("Could not read output directory {}", directory.display()))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow::Error::msg(format!("Invalid file name {}", path.display())))?;
        zip.start_file(name, options)?;
        let mut input = File::open(&path)
            .with_context(|| format!("Could not open file for zip {}", path.display()))?;
        std::io::copy(&mut input, &mut zip)?;
    }

    zip.finish()?.flush()?;
    Ok(zip_path)
}

fn apply_gnome_wallpaper(xml_path: &Path) -> Result<()> {
    let uri = file_uri(xml_path)?;
    println!(
        "{}: Applying GNOME wallpaper {}",
        "Installation".green(),
        uri
    );

    set_gnome_background_key("picture-uri", &uri)?;
    set_gnome_background_key("picture-uri-dark", &uri)?;
    set_gnome_screensaver_key("picture-uri", &uri)?;
    Ok(())
}

fn set_gnome_background_key(key: &str, value: &str) -> Result<()> {
    run_gsettings(&["set", "org.gnome.desktop.background", key, value])
}

fn set_gnome_screensaver_key(key: &str, value: &str) -> Result<()> {
    run_gsettings(&["set", "org.gnome.desktop.screensaver", key, value])
}

fn run_gsettings(args: &[&str]) -> Result<()> {
    let status = ProcessCommand::new("gsettings")
        .args(args)
        .status()
        .context("Could not run gsettings. Omit --apply to only install generated files.")?;

    if status.success() {
        Ok(())
    } else {
        Err(anyhow::Error::msg(format!(
            "gsettings failed with status {}. Omit --apply to only install generated files.",
            status
        )))
    }
}

fn file_uri(path: &Path) -> Result<String> {
    let absolute = path
        .canonicalize()
        .with_context(|| format!("Could not canonicalize XML path {}", path.display()))?;
    Ok(format!(
        "file://{}",
        percent_encode_path(&absolute.to_string_lossy())
    ))
}

fn percent_encode_path(path: &str) -> String {
    let mut encoded = String::with_capacity(path.len());
    for byte in path.bytes() {
        if byte == b'/' || byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~')
        {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{:02X}", byte));
        }
    }
    encoded
}
