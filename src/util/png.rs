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
use colored::*;
use std::io::{BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result};
use libheif_rs::{ColorSpace, ImageHandle, LibHeif, RgbChroma};

pub fn write_png(path: &Path, lib_heif: &LibHeif, handle: &ImageHandle) -> Result<()> {
    let width = handle.width();
    let height = handle.height();

    match lib_heif.decode(handle, ColorSpace::Rgb(RgbChroma::Rgb), None) {
        Ok(decoded) => {
            let planes = decoded.planes();
            let interleaved = planes
                .interleaved
                .context("Decoded image did not contain an interleaved RGB plane")?;

            if interleaved.width != width || interleaved.height != height {
                return Err(anyhow::Error::msg("Interleaved RGB plane dimensions do not match the image dimensions. The image data is probably invalid, please check the used image in another application."));
            }

            let file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(path)?;
            let writer = BufWriter::new(file);

            let mut pngencoder = png::Encoder::new(writer, width, height);
            pngencoder.set_color(png::ColorType::Rgb);
            pngencoder.set_depth(png::BitDepth::Eight);
            let image_writer = pngencoder.write_header()?;
            let mut w = image_writer.into_stream_writer()?;

            let row_len = width as usize * 3;
            for y in 0..height as usize {
                let start = y * interleaved.stride;
                w.write_all(&interleaved.data[start..start + row_len])?;
            }

            Ok(())
        }
        Err(_) => write_png_from_planar_rgb(path, lib_heif, handle, width, height),
    }
}

fn write_png_from_planar_rgb(
    path: &Path,
    lib_heif: &LibHeif,
    handle: &ImageHandle,
    width: u32,
    height: u32,
) -> Result<()> {
    match lib_heif.decode(handle, ColorSpace::Rgb(RgbChroma::C444), None) {
        Ok(decoded) => {
            let planes = decoded.planes();

            let red = planes
                .r
                .context("Decoded image did not contain a red plane")?;
            let green = planes
                .g
                .context("Decoded image did not contain a green plane")?;
            let blue = planes
                .b
                .context("Decoded image did not contain a blue plane")?;

            let file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(path)?;
            let writer = BufWriter::new(file);

            if red.width != width
                || green.width != width
                || blue.width != width
                || red.height != height
                || green.height != height
                || blue.height != height
            {
                return Err(anyhow::Error::msg("Color plane dimensions do not match the image dimensions. The image data is probably invalid, please check the used image in another application."));
            }

            let mut pngencoder = png::Encoder::new(writer, width, height);
            pngencoder.set_color(png::ColorType::Rgb);
            pngencoder.set_depth(png::BitDepth::Eight);
            let image_writer = pngencoder.write_header()?;
            let mut w = image_writer.into_stream_writer()?;

            let width = width as usize;
            let height = height as usize;
            let mut row = vec![0; width * 3];

            for y in 0..height {
                let red_row = &red.data[y * red.stride..y * red.stride + width];
                let green_row = &green.data[y * green.stride..y * green.stride + width];
                let blue_row = &blue.data[y * blue.stride..y * blue.stride + width];

                for (pixel, ((r, g), b)) in row
                    .chunks_exact_mut(3)
                    .zip(red_row.iter().zip(green_row.iter()).zip(blue_row.iter()))
                {
                    pixel[0] = *r;
                    pixel[1] = *g;
                    pixel[2] = *b;
                }

                w.write_all(&row)?;
            }
            Ok(())
        }
        Err(err) => {
            println!(
                "{}: Could not determine color space. Colorspace RGB C444 could not be applied",
                "Error".red(),
            );
            Err(anyhow::Error::msg(format!(
                "Could not decode the image data in RGB C444 colorspace: {:?}",
                err
            )))
        }
    }
}
