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
use std::path::Path;

use crate::schema::xml::{
    Background,
    Image::{Static, Transition},
};
use crate::serializer::GnomeXMLBackgroundSerializer;
use crate::util::png;
use crate::DAY_SECS;
use anyhow::{Context, Result};
use colored::*;
use libheif_rs::{HeifContext, LibHeif};
use std::io::BufWriter;
use std::path::PathBuf;

pub struct ImagePoint {
    pub source_path: PathBuf,
    pub img_id: u32,
    pub index: usize,
    pub image_count: usize,
    pub parent_directory: PathBuf,
    pub start_time: f32,
    pub time: f32,
    pub next_time: f32,
}

pub fn process_img(pt: &ImagePoint) -> Result<(usize, Vec<crate::schema::xml::Image>)> {
    let lib_heif = LibHeif::new();
    let image_ctx = HeifContext::read_from_file(&pt.source_path.to_string_lossy())
        .with_context(|| format!("Could not read HEIC file {}", pt.source_path.display()))?;
    let prim_image = image_ctx
        .image_handle(pt.img_id)
        .with_context(|| format!("Could not fetch image handle {}", pt.img_id))?;
    let file_path = image_path(&pt.parent_directory, pt.index);
    let file = file_path.to_string_lossy().into_owned();
    png::write_png(&file_path, &lib_heif, &prim_image)?;

    let mut images = Vec::with_capacity(2);
    images.push(Static {
        duration: 1f32,
        file: file.clone(),
    });

    let next_index = if pt.index < pt.image_count - 1 {
        pt.index + 1
    } else {
        0
    };
    let next_file = image_path(&pt.parent_directory, next_index)
        .to_string_lossy()
        .into_owned();

    images.push(Transition {
        kind: "overlay".to_string(),
        duration: {
            if pt.index < pt.image_count - 1 {
                (pt.time - pt.next_time).abs() * DAY_SECS - 1.0
            } else {
                (((pt.time - 1.0).abs() + pt.start_time) * DAY_SECS - 1.0).ceil()
            }
        },
        from: file,
        to: next_file,
    });

    Ok((pt.index, images))
}

fn image_path(parent_directory: &Path, index: usize) -> PathBuf {
    parent_directory.join(format!("{}.png", index))
}

pub fn save_xml(
    xml: &mut Background,
    parent_directory: &Path,
    image_name: &str,
) -> Result<PathBuf> {
    println!(
        "{}: Creating xml description for new wallpaper...",
        "Conversion".green(),
    );
    println!("{}: Writing wallpaper description...", "Conversion".green(),);
    let xml_directory = parent_directory.parent().ok_or_else(|| {
        anyhow::Error::msg(format!(
            "Could not determine XML directory for {}",
            parent_directory.display()
        ))
    })?;
    let xml_path = xml_directory.join(format!("{}.xml", image_name));
    let result_file = std::fs::OpenOptions::new()
        .write(true)
        .truncate(true)
        .create(true)
        .open(&xml_path)?;
    let mut result = BufWriter::new(result_file);
    let mut ser = GnomeXMLBackgroundSerializer::new(&mut result);
    ser.serialize(xml)?;
    println!("{}: {}", "Conversion".green(), "Done!".green());
    Ok(xml_path)
}
