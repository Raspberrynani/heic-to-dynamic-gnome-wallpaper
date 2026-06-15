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
use anyhow::Result;
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use libheif_rs::HeifContext;
use quick_xml::{events::Event, name::QName, Reader};

use crate::schema::plist::{WallpaperMetaSun, WallpaperMetaTime};

pub enum WallPaperMode {
    H24(String),
    Solar(String),
}

pub fn get_wallpaper_metadata(image_ctx: &HeifContext) -> Result<Option<WallPaperMode>> {
    // Fetch META information about all images (These are by standard stored in the first images meta information tags)
    let primary_image = image_ctx.primary_image_handle()?;
    let metadata_count = primary_image.number_of_metadata_blocks(b"mime");
    if metadata_count == 0 {
        return Ok(None);
    }

    let mut metadatas = vec![0; metadata_count as usize];
    primary_image.metadata_block_ids(&mut metadatas, b"mime");

    for metadata_id in metadatas {
        let tmp = primary_image.metadata(metadata_id)?;
        let content = String::from_utf8_lossy(&tmp);
        //println!("{:?}", content);
        let mut reader = Reader::from_str(&content);
        reader.config_mut().trim_text(true);

        let mut h24 = None;

        loop {
            match reader.read_event() {
                Ok(quick_xml::events::Event::Empty(ref e)) => {
                    e.attributes()
                        .flatten()
                        .filter(|att| {
                            att.key == QName(b"apple_desktop:h24")
                                || att.key == QName(b"apple_desktop:solar")
                        })
                        .for_each(|att| match att.key {
                            QName(b"apple_desktop:h24") => {
                                h24 = Some(WallPaperMode::H24(
                                    String::from_utf8_lossy(att.value.as_ref()).to_string(),
                                ))
                            }
                            QName(b"apple_desktop:solar") => {
                                h24 = Some(WallPaperMode::Solar(
                                    String::from_utf8_lossy(att.value.as_ref()).to_string(),
                                ))
                            }
                            _ => panic!("Invalid Branch"),
                        });
                    break;
                }
                Ok(Event::Eof) => break,
                Err(_) => break,
                _ => {}
            }
        }
        if h24.is_some() {
            return Ok(h24);
        }
    }

    Ok(None)
}

pub fn get_time_plist_from_base64(input: &str) -> Result<WallpaperMetaTime> {
    let decoded = STANDARD.decode(input)?;
    let plist = plist::from_bytes(&decoded)?;
    Ok(plist)
}

pub fn get_solar_plist_from_base64(input: &str) -> Result<WallpaperMetaSun> {
    let decoded = STANDARD.decode(input)?;
    let plist = plist::from_bytes(&decoded)?;
    Ok(plist)
}
