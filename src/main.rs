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
             .help("Base directory where GNOME wallpaper files should be installed.")
             .long_help("Specifies the base directory for generated wallpapers. A folder named after the input image and a matching XML file will be created inside it. Default is $XDG_DATA_HOME/backgrounds/gnome or ~/.local/share/backgrounds/gnome.")
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
             .help("Export the generated wallpaper XML and image folder as a zip file next to the source image.")
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
        .to_string_lossy()
        .into_owned();
    let wallpaper_name = sanitize_filename(&image_name);
    let parent_directory = output_directory(
        matches.get_one::<String>(DIR).map(String::as_str),
        &wallpaper_name,
    )?;
    cleanup_generated_files(&parent_directory, &wallpaper_name)?;

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
                &wallpaper_name,
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
                &wallpaper_name,
            )
        }
    }?;
    let properties_path =
        save_gnome_background_properties(&parent_directory, &xml_path, &wallpaper_name)?;

    if matches.get_flag(ZIP) {
        export_zip(
            &parent_directory,
            &xml_path,
            &properties_path,
            Path::new(path),
            &wallpaper_name,
        )?;
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
        return Ok(PathBuf::from(data_home).join("backgrounds/gnome"));
    }

    std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join(".local/share/backgrounds/gnome"))
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
    for entry in std::fs::read_dir(directory)
        .with_context(|| format!("Could not read output directory {}", directory.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let should_remove = path.extension().and_then(|ext| ext.to_str()) == Some("png")
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

    let xml_path = directory
        .parent()
        .ok_or_else(|| {
            anyhow::Error::msg(format!(
                "Could not determine XML directory for {}",
                directory.display()
            ))
        })?
        .join(format!("{}.xml", image_name));
    if xml_path.exists() {
        std::fs::remove_file(&xml_path).with_context(|| {
            format!(
                "Could not remove stale generated file {}",
                xml_path.display()
            )
        })?;
    }

    let properties_path = gnome_background_properties_path(directory, image_name)?;
    if properties_path.exists() {
        std::fs::remove_file(&properties_path).with_context(|| {
            format!(
                "Could not remove stale GNOME background properties file {}",
                properties_path.display()
            )
        })?;
    }

    Ok(())
}

fn save_gnome_background_properties(
    image_directory: &Path,
    timed_xml_path: &Path,
    wallpaper_name: &str,
) -> Result<PathBuf> {
    let properties_path = gnome_background_properties_path(image_directory, wallpaper_name)?;
    if let Some(directory) = properties_path.parent() {
        std::fs::create_dir_all(directory).with_context(|| {
            format!(
                "Could not create GNOME background properties directory {}",
                directory.display()
            )
        })?;
    }

    let timed_xml_path = timed_xml_path.canonicalize().with_context(|| {
        format!(
            "Could not canonicalize wallpaper XML path {}",
            timed_xml_path.display()
        )
    })?;
    let mut file = BufWriter::new(File::create(&properties_path).with_context(|| {
        format!(
            "Could not create GNOME background properties file {}",
            properties_path.display()
        )
    })?);

    writeln!(file, "<?xml version=\"1.0\"?>")?;
    writeln!(file, "<!DOCTYPE wallpapers SYSTEM \"gnome-wp-list.dtd\">")?;
    writeln!(file, "<wallpapers>")?;
    writeln!(file, "  <wallpaper deleted=\"false\">")?;
    writeln!(file, "    <name>{}</name>", xml_escape(wallpaper_name))?;
    writeln!(
        file,
        "    <filename>{}</filename>",
        xml_escape(&timed_xml_path.to_string_lossy())
    )?;
    writeln!(file, "    <options>zoom</options>")?;
    writeln!(file, "  </wallpaper>")?;
    writeln!(file, "</wallpapers>")?;
    file.flush()?;

    Ok(properties_path)
}

fn gnome_background_properties_path(
    image_directory: &Path,
    wallpaper_name: &str,
) -> Result<PathBuf> {
    let gnome_directory = image_directory.parent().ok_or_else(|| {
        anyhow::Error::msg(format!(
            "Could not determine GNOME backgrounds directory for {}",
            image_directory.display()
        ))
    })?;
    let properties_directory =
        if gnome_directory.file_name().and_then(|name| name.to_str()) == Some("gnome") {
            if let Some(backgrounds_directory) = gnome_directory.parent() {
                if backgrounds_directory
                    .file_name()
                    .and_then(|name| name.to_str())
                    == Some("backgrounds")
                {
                    if let Some(share_directory) = backgrounds_directory.parent() {
                        share_directory.join("gnome-background-properties")
                    } else {
                        gnome_directory.join("gnome-background-properties")
                    }
                } else {
                    gnome_directory.join("gnome-background-properties")
                }
            } else {
                gnome_directory.join("gnome-background-properties")
            }
        } else {
            gnome_directory.join("gnome-background-properties")
        };

    Ok(properties_directory.join(format!("{}.xml", wallpaper_name)))
}

fn export_zip(
    image_directory: &Path,
    xml_path: &Path,
    properties_path: &Path,
    source_path: &Path,
    wallpaper_name: &str,
) -> Result<PathBuf> {
    let zip_path = source_path.with_extension("zip");
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

    let xml_name = xml_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow::Error::msg(format!("Invalid file name {}", xml_path.display())))?;
    zip.start_file(xml_name, options)?;
    let mut xml = File::open(xml_path)
        .with_context(|| format!("Could not open XML file for zip {}", xml_path.display()))?;
    std::io::copy(&mut xml, &mut zip)?;

    let properties_name = properties_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            anyhow::Error::msg(format!("Invalid file name {}", properties_path.display()))
        })?;
    zip.start_file(
        format!("gnome-background-properties/{}", properties_name),
        options,
    )?;
    let mut properties = File::open(properties_path).with_context(|| {
        format!(
            "Could not open GNOME background properties file for zip {}",
            properties_path.display()
        )
    })?;
    std::io::copy(&mut properties, &mut zip)?;

    let mut entries = std::fs::read_dir(image_directory)
        .with_context(|| {
            format!(
                "Could not read output directory {}",
                image_directory.display()
            )
        })?
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
        zip.start_file(format!("{}/{}", wallpaper_name, name), options)?;
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
    set_gnome_background_key("picture-options", "zoom")?;
    set_gnome_screensaver_key("picture-uri", &uri)?;
    set_gnome_screensaver_key("picture-options", "zoom")?;
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

fn xml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}
