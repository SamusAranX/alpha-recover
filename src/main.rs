use std::io::{Error, ErrorKind};
use std::path::PathBuf;
use std::time::Instant;

use clap::Parser;
use image::{ColorType, DynamicImage, GenericImageView, ImageBuffer};
use image::io::Reader as ImageReader;

#[derive(clap::ValueEnum, Clone, Default)]
enum Blend {
    White,
    #[default]
    Black,
    Mix,
}

#[derive(Parser)]
#[clap(version, about="Derives an image with alpha channel from two alpha-less images")]
#[command(version, about)]
struct Args {
    #[clap(short, long, value_enum, help="Which image to take the color values from (mix is experimental)", default_value_t=Blend::default())]
    blend: Blend,

    #[clap(help="An image with a solid black background")]
    black: PathBuf,
    #[clap(help="An image with a solid white background")]
    white: PathBuf,
    #[clap(help="The output image")]
    out: PathBuf,
}

fn preflight_checks(black: &DynamicImage, white: &DynamicImage) -> Result<(), Error> {
    let unsupported_color_types = vec![ColorType::Rgb32F, ColorType::Rgba32F];
    let black_color = black.color();
    let white_color = white.color();

    if black.dimensions() != white.dimensions() {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "Both input images must be the same size",
        ));
    }

    if unsupported_color_types.contains(&black_color) || unsupported_color_types.contains(&white_color) {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "32-bit color is not supported",
        ));
    }

    if black_color != white_color {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            "Both input images must use the same color format",
        ));
    }

    Ok(())
}

/// Does Math™ on two input pixels from images with black and white backgrounds
/// respectively to obtain a "fixed" pixel that includes an alpha channel.
/// The input pixels are expected to be three-item f32 arrays,
/// the output pixel is a four-item f64 array.
/// Based on the method explained here: https://www.interact-sw.co.uk/iangblog/2007/01/30/recoveralpha
fn magic(black_pixel: [f32; 3], white_pixel: [f32; 3], blend: &Blend) -> [f64; 4] {
    let (rb, gb, bb, rw, gw, bw) = (
        black_pixel[0] as f64,
        black_pixel[1] as f64,
        black_pixel[2] as f64,
        white_pixel[0] as f64,
        white_pixel[1] as f64,
        white_pixel[2] as f64,
    );

    let (alpha, mut rs, mut gs, mut bs) = (
        rb - rw + 1.0, // this can occasionally exceed 1.0 but it seems saving as non-32-bit automatically clips this to [0.0, 1.0]
        0.0, 0.0, 0.0
    );

    if alpha > 0.0 {
        match blend {
            Blend::White => {
                rs = rw / alpha;
                gs = gw / alpha;
                bs = bw / alpha;
            }
            Blend::Black => {
                rs = rb / alpha;
                gs = gb / alpha;
                bs = bb / alpha;
            }
            Blend::Mix => {
                // not actually all that accurate, just in here as an experiment
                rs = (rb + rw) / 2.0 / alpha;
                gs = (gb + gw) / 2.0 / alpha;
                bs = (bb + bw) / 2.0 / alpha;
            }
        }
    }

    return [rs, gs, bs, alpha];
}

const SCALAR8: f64 = 255.0;
const SCALAR16: f64 = 65535.0;

fn main() -> Result<(), Error> {
    let args = Args::parse();

    // println!("black path: {}", args.black.display());
    // println!("white path: {}", args.white.display());
    // println!("out path: {}", args.out.display());

    println!("Loading images…");

    let start = Instant::now();

    let black_reader = ImageReader::open(args.black).expect("Can't open file");
    let white_reader = ImageReader::open(args.white).expect("Can't open file");

    let black_image = black_reader.decode().expect("Can't decode image");
    let white_image = white_reader.decode().expect("Can't decode image");

    preflight_checks(&black_image, &white_image).unwrap();

    let image_dim = black_image.dimensions();

    let color_type = black_image.color();
    let black_rgb = black_image.into_rgb32f();
    let white_rgb = white_image.into_rgb32f();

    let format_name = if color_type.has_color() { "RGB" } else { "grayscale" };
    let bits_per_channel = color_type.bits_per_pixel() / color_type.channel_count() as u16;
    println!("Generating {format_name} output at {}×{} with {bits_per_channel} bits per channel…", image_dim.0, image_dim.1);

    // TODO: please let there be a way to reduce the amount of code in this match block 😭
    match color_type {
        ColorType::L8 | ColorType::La8 => {
            let mut luma_image = ImageBuffer::new(image_dim.0, image_dim.1);
            for (x, y, pixel) in luma_image.enumerate_pixels_mut() {
                let bp = black_rgb.get_pixel(x, y).0;
                let wp = white_rgb.get_pixel(x, y).0;
                let new = magic(bp, wp, &args.blend);

                *pixel = image::LumaA([
                    (new[0] * SCALAR8) as u8,
                    (new[3] * SCALAR8) as u8,
                ]);
            }

            luma_image.save(args.out.as_path()).unwrap();
        }
        ColorType::L16 | ColorType::La16 => {
            let mut luma_image = ImageBuffer::new(image_dim.0, image_dim.1);
            for (x, y, pixel) in luma_image.enumerate_pixels_mut() {
                let bp = black_rgb.get_pixel(x, y).0;
                let wp = white_rgb.get_pixel(x, y).0;
                let new = magic(bp, wp, &args.blend);

                *pixel = image::LumaA([
                    (new[0] * SCALAR16) as u16,
                    (new[3] * SCALAR16) as u16,
                ]);
            }

            luma_image.save(args.out.as_path()).unwrap();
        }
        ColorType::Rgb8 | ColorType::Rgba8 => {
            let mut rgb_image = ImageBuffer::new(image_dim.0, image_dim.1);
            for (x, y, pixel) in rgb_image.enumerate_pixels_mut() {
                let bp = black_rgb.get_pixel(x, y).0;
                let wp = white_rgb.get_pixel(x, y).0;
                let new = magic(bp, wp, &args.blend);

                *pixel = image::Rgba([
                    (new[0] * SCALAR8) as u8,
                    (new[1] * SCALAR8) as u8,
                    (new[2] * SCALAR8) as u8,
                    (new[3] * SCALAR8) as u8,
                ]);
            }

            rgb_image.save(args.out.as_path()).unwrap();
        }
        ColorType::Rgb16 | ColorType::Rgba16 => {
            let mut rgb_image = ImageBuffer::new(image_dim.0, image_dim.1);
            for (x, y, pixel) in rgb_image.enumerate_pixels_mut() {
                let bp = black_rgb.get_pixel(x, y).0;
                let wp = white_rgb.get_pixel(x, y).0;
                let new = magic(bp, wp, &args.blend);

                *pixel = image::Rgba([
                    (new[0] * SCALAR16) as u16,
                    (new[1] * SCALAR16) as u16,
                    (new[2] * SCALAR16) as u16,
                    (new[3] * SCALAR16) as u16,
                ]);
            }

            rgb_image.save(args.out.as_path()).unwrap();
        }
        _ => {}
    }

    println!("{} saved in {:.02}s!", args.out.file_name().unwrap().to_str().unwrap(), start.elapsed().as_secs_f64());

    Ok(())
}
