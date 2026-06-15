# heic-to-dynamic-gnome-wallpaper

Convert macOS dynamic wallpaper `.heic` files into GNOME dynamic wallpaper XML plus extracted PNG frames.

The converter supports both Apple `h24` time-based wallpapers and Apple `solar` wallpapers. Solar wallpapers are converted into a time-based GNOME schedule using the azimuth data embedded in the HEIC metadata.

## Features

- Extracts every dynamic wallpaper frame from the HEIC container.
- Preserves irregular `h24` timing intervals from Apple metadata.
- Writes a GNOME-compatible dynamic wallpaper XML file.
- Uses parallel frame conversion for faster processing.
- Installs output by default under `~/.local/share/backgrounds/<wallpaper-name>/`.
- Can apply the generated wallpaper to GNOME with `--apply`.
- Can export the generated folder as a ZIP archive with `--zip`.

## Requirements

Install Rust and the native HEIC build dependencies.

Ubuntu/Debian:

```sh
sudo apt install pkg-config zlib1g-dev libheif-dev
```

You also need a Rust toolchain:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Make sure Cargo-installed binaries are on your `PATH`:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
```

Add that line to your shell config if needed.

## Install Or Reinstall

From this checkout:

```sh
cd $yourdownloadedpath
cargo install --path . --force
```

Verify:

```sh
heic-to-dynamic-gnome-wallpaper --help
```

If the command is not found, your shell is not seeing `~/.cargo/bin`.

## Usage

Basic conversion:

```sh
heic-to-dynamic-gnome-wallpaper /path/to/wallpaper.heic
```

This creates:

```text
~/.local/share/backgrounds/<wallpaper-name>/
  0.png
  1.png
  ...
  <wallpaper-name>.xml
```

Use a custom base directory:

```sh
heic-to-dynamic-gnome-wallpaper /path/to/wallpaper.heic --dir /tmp/wallpapers
```

Apply to GNOME after conversion:

```sh
heic-to-dynamic-gnome-wallpaper /path/to/wallpaper.heic --apply
```

Export a shareable ZIP:

```sh
heic-to-dynamic-gnome-wallpaper /path/to/wallpaper.heic --zip
```

Combine options:

```sh
heic-to-dynamic-gnome-wallpaper /path/to/wallpaper.heic --dir /tmp/wallpapers --zip --apply
```

## CLI

```text
Usage: heic-to-dynamic-gnome-wallpaper [OPTIONS] <IMAGE>

Arguments:
  <IMAGE>
          Image which should be transformed

Options:
  -d, --dir <DIR>
          Specifies the base directory for generated wallpapers. A folder named after the input image will be created inside it. Default is $XDG_DATA_HOME/backgrounds or ~/.local/share/backgrounds.

      --apply
          Apply the generated wallpaper through GNOME settings after conversion.

      --zip
          Export the generated wallpaper folder as a zip file next to the folder.

  -h, --help
          Print help
```

## Notes

- Re-running conversion for the same wallpaper name cleans previously generated numbered PNG files and the matching XML before writing new output.
- ZIP export stores PNG files without recompressing them, so archive creation is fast and avoids wasting CPU on already-compressed images.
- On openSUSE, the default `libheif` package may not include the HEIC/H.265 codec. Use the Packman repository if decoding HEIC files fails.
