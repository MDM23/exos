//! A sleeve, and the sixteen pixels a side that stand in for it.
//!
//! The art is an embedded asset, so the placeholder is not a second thing to
//! keep in step with it: both come out of the same bytes, once, while the room
//! is being seeded. Replacing the file replaces the blur, and there is nothing
//! written down beside the image that can go stale.
//!
//! **The placeholder travels in the page rather than as anything to decode.**
//! A format like [thumbhash](https://evanw.github.io/thumbhash/) squeezes this
//! into twenty-five bytes, which is worth a vendored decoder and a plugin when
//! a page carries hundreds of them and the hashes are rows in a database. This
//! page carries one, so the same picture goes in as a `data:` URL the browser
//! already knows how to read, and the whole client half of it disappears. It
//! also stops being something a patch can strip: a `style` the server rendered
//! is markup, and the morph keeps markup.

use base64::Engine as _;
use exos::Asset;

/// The longest edge of the placeholder.
///
/// It is scaled up to fill the sleeve, so this is the blur's resolution and
/// most of its weight. Sixteen is a few hundred bytes in the page and enough
/// to make out where a sleeve is light and where it is dark, which is all a
/// blur is for.
const SAMPLE: usize = 16;

/// A cover, and the blur that stands in for it until it arrives.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Sleeve {
    /// The image itself, as the browser asks for it.
    pub(crate) art: Asset,
    /// The same image, small enough to inline, as a `data:` URL.
    pub(crate) blur: String,
}

impl Sleeve {
    /// Reads the art back out of the binary and shrinks it.
    ///
    /// # Panics
    ///
    /// If the asset is not a PNG this crate embedded. That is a build fault
    /// rather than anything a request can cause, and this runs at startup,
    /// which is when it should be heard.
    pub(crate) fn new(art: Asset) -> Self {
        let small = encode(&sample(&decode(art.bytes())));

        Self {
            art,
            blur: format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(small)
            ),
        }
    }
}

/// A decoded image: 8-bit RGBA, row by row.
struct Pixels {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}

fn decode(png: &[u8]) -> Pixels {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(png));
    decoder.set_transformations(png::Transformations::normalize_to_color8());

    let mut reader = decoder.read_info().expect("a sleeve is a PNG");
    let mut buffer = vec![
        0;
        reader
            .output_buffer_size()
            .expect("a sleeve fits in memory twice over")
    ];

    let frame = reader
        .next_frame(&mut buffer)
        .expect("a sleeve has a frame");

    let channels = match frame.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        other => panic!("a sleeve is RGB or RGBA, not {other:?}"),
    };

    let width = frame.width as usize;
    let height = frame.height as usize;
    let mut rgba = Vec::with_capacity(width * height * 4);

    for pixel in buffer.chunks_exact(channels).take(width * height) {
        rgba.extend([
            pixel[0],
            pixel[1],
            pixel[2],
            *pixel.get(3).unwrap_or(&u8::MAX),
        ]);
    }

    Pixels {
        width,
        height,
        rgba,
    }
}

/// A box average down to at most [`SAMPLE`] on the long edge.
fn sample(image: &Pixels) -> Pixels {
    let long = image.width.max(image.height);

    if long <= SAMPLE {
        return Pixels {
            width: image.width,
            height: image.height,
            rgba: image.rgba.clone(),
        };
    }

    let width = (image.width * SAMPLE / long).max(1);
    let height = (image.height * SAMPLE / long).max(1);
    let mut rgba = Vec::with_capacity(width * height * 4);

    for y in 0..height {
        let top = y * image.height / height;
        let bottom = ((y + 1) * image.height / height).max(top + 1);

        for x in 0..width {
            let left = x * image.width / width;
            let right = ((x + 1) * image.width / width).max(left + 1);

            let mut totals = [0_usize; 4];

            for row in top..bottom {
                for column in left..right {
                    let at = (row * image.width + column) * 4;

                    for (total, value) in totals.iter_mut().zip(&image.rgba[at..at + 4]) {
                        *total += usize::from(*value);
                    }
                }
            }

            let count = (bottom - top) * (right - left);
            rgba.extend(totals.map(|total| u8::try_from(total / count).unwrap_or(u8::MAX)));
        }
    }

    Pixels {
        width,
        height,
        rgba,
    }
}

/// The placeholder, as the PNG that goes into the page.
fn encode(image: &Pixels) -> Vec<u8> {
    let mut out = Vec::new();

    let mut encoder = png::Encoder::new(&mut out, image.width as u32, image.height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);

    let mut writer = encoder.write_header().expect("a header fits in a Vec");

    writer
        .write_image_data(&image.rgba)
        .expect("the rows are the size the header claims");

    writer.finish().expect("a Vec does not fail to flush");

    out
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "a failing assertion is the point of a test"
)]
mod tests {
    use super::*;

    /// The whole claim of this module: the placeholder comes out of the file
    /// the browser is served, so the two cannot disagree.
    #[test]
    fn a_sleeve_shrinks_the_art_it_points_at() {
        let sleeve = Sleeve::new(exos::asset!("img/coffee.png"));
        let png = sleeve
            .blur
            .strip_prefix("data:image/png;base64,")
            .expect("the blur is an inline PNG");

        let bytes = base64::engine::general_purpose::STANDARD
            .decode(png)
            .expect("the blur is base64");

        // Small enough that inlining it beats a request for it, which is the
        // whole reason it is in the page rather than beside it.
        assert!(bytes.len() < 1024, "{} bytes", bytes.len());

        // And it is that sleeve rather than any sleeve: coffee.png is a dark
        // warm square, so what it shrinks to is warm and dark.
        let pixels = decode(&bytes);
        let (red, green, blue) = average(&pixels);

        assert_eq!((pixels.width, pixels.height), (SAMPLE, SAMPLE));
        assert!(red > blue, "warm: {red} against {blue}");
        assert!(green < 140, "dark: {green}");
    }

    /// Two call sites, one file, one set of bytes. The second `asset!` embeds
    /// nothing and still finds them.
    #[test]
    fn the_same_art_shrinks_the_same_from_anywhere() {
        assert_eq!(
            Sleeve::new(exos::asset!("img/coffee.png")).blur,
            Sleeve::new(exos::asset!("img/coffee.png")).blur
        );
    }

    fn average(image: &Pixels) -> (usize, usize, usize) {
        let mut totals = [0_usize; 3];

        for pixel in image.rgba.chunks_exact(4) {
            for (total, value) in totals.iter_mut().zip(pixel) {
                *total += usize::from(*value);
            }
        }

        let count = image.rgba.len() / 4;
        (totals[0] / count, totals[1] / count, totals[2] / count)
    }
}
