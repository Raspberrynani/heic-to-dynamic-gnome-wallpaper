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
use std::path::{Path, PathBuf};

use crate::image::{process_img, save_xml, ImagePoint};
use crate::metadata;
use crate::schema::plist::TimeSlice;
use crate::schema::xml::{Background, StartTime};
use crate::DAY_SECS;

use crate::util::time;
use anyhow::{Context, Result};
use chrono::Datelike;
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use libheif_rs::HeifContext;
use rayon::prelude::*;

pub fn compute_time_based_wallpaper(
    source_path: &Path,
    image_ctx: HeifContext,
    content: String,
    parent_directory: &Path,
    image_name: &str,
) -> Result<PathBuf> {
    let mut plist = metadata::get_time_plist_from_base64(&content)?;
    //println!("Found plist {:?}", plist);

    plist.time_slices.sort_by(|a, b| a.time.total_cmp(&b.time));
    let start_time = plist
        .time_slices
        .first()
        .context("No time slices were found in the wallpaper metadata")?
        .time;
    let start_seconds = (start_time * DAY_SECS) as u32;
    let date = chrono::Local::now();
    let number_of_images = image_ctx.number_of_top_level_images();
    let mut xml_background = Background {
        images: Vec::with_capacity(number_of_images * 2),
        starttime: StartTime {
            year: date.year(),
            month: date.month(),
            day: date.day(),
            hour: time::to_rem_hours(start_seconds),
            minute: time::to_rem_min(start_seconds),
            second: time::to_rem_sec(start_seconds),
        },
    };

    println!(
        "{}: Found {} images",
        "Preparation".bright_blue(),
        number_of_images,
    );
    let mut image_ids = vec![0u32; number_of_images];
    image_ctx.top_level_image_ids(&mut image_ids);
    println!(
        "{}: Converting embedded images to png format",
        "Conversion".green(),
    );
    println!("{}:", "Conversion".green());
    let pb = ProgressBar::new(number_of_images as u64).with_style(
        ProgressStyle::default_bar()
            .template("[{wide_bar}] {pos}/{len} [ETA: {eta_precise}]")
            .map_err(anyhow::Error::from)?
            .progress_chars("## "),
    );
    let mut points = Vec::with_capacity(plist.time_slices.len());
    for (time_idx, TimeSlice { time, idx }) in plist.time_slices.iter().enumerate() {
        let img_id = *image_ids
            .get(*idx)
            .with_context(|| format!("Could not fetch image id described in metadata: {}", idx))?;
        points.push(ImagePoint {
            source_path: source_path.to_path_buf(),
            img_id,
            index: time_idx,
            image_count: plist.time_slices.len(),
            parent_directory: parent_directory.to_path_buf(),
            start_time,
            time: *time,
            next_time: plist
                .time_slices
                .get(time_idx + 1)
                .map(|elem| elem.time)
                .unwrap_or(0f32),
        });
    }

    let mut image_entries: Vec<_> = points
        .par_iter()
        .map(|point| {
            let result = process_img(point);
            pb.inc(1);
            result
        })
        .collect::<Result<Vec<_>>>()?;
    pb.finish_and_clear();
    image_entries.sort_by_key(|(idx, _)| *idx);
    xml_background
        .images
        .extend(image_entries.into_iter().flat_map(|(_, images)| images));

    // Valify time range
    let total_time = xml_background
        .images
        .iter()
        .fold(0f32, |acc, image| match image {
            crate::schema::xml::Image::Static { duration, .. } => acc + duration,
            crate::schema::xml::Image::Transition { duration, .. } => acc + duration,
        });

    if total_time < DAY_SECS as f32 {
        if let Some(img) = xml_background.images.last_mut() {
            match img {
                crate::schema::xml::Image::Static {
                    ref mut duration, ..
                } => {
                    *duration = (*duration + (DAY_SECS as f32 - total_time)).ceil();
                }
                crate::schema::xml::Image::Transition {
                    ref mut duration, ..
                } => {
                    *duration = (*duration + (DAY_SECS as f32 - total_time)).ceil();
                }
            }
        }
    }

    save_xml(&mut xml_background, parent_directory, image_name)
}
