//! How big to draw the NES picture: every NES row a whole number of physical pixel rows, and the
//! width whatever the region's pixel shape makes it. NES pixels aren't square (8:7 on NTSC), so
//! whole pixels in both directions would only keep their shape at 8×7 physical pixels each;
//! instead the columns are drawn with sharp bilinear filtering (see `nes_picture.wgsl`), which
//! blends only where a column edge falls inside a physical pixel.

use crate::{
    Size,
    emulation::{NES_HEIGHT, NES_WIDTH},
};

/// Points per NES row in a window at its default size. From this size up, the picture gets whole
/// physical rows per NES row; a smaller window is filled instead, rather than left with wide
/// borders.
const DEFAULT_SCALE: f32 = 3.0;

/// The picture's width at 1x: 256 pixels of `pixel_aspect` each.
fn picture_width(pixel_aspect: f32) -> f32 {
    NES_WIDTH as f32 * pixel_aspect
}

/// The window's size at its default, in points.
pub fn default_window_size(pixel_aspect: f32) -> Size {
    Size::new(
        (picture_width(pixel_aspect) * DEFAULT_SCALE).round() as u32,
        (NES_HEIGHT as f32 * DEFAULT_SCALE) as u32,
    )
}

/// The smallest the window may be, in points: the picture at 1x.
pub fn min_window_size(pixel_aspect: f32) -> Size {
    Size::new(picture_width(pixel_aspect).round() as u32, NES_HEIGHT)
}

/// The size to draw the picture at, in whole physical pixels, for a window `available` points
/// big at `pixels_per_point` (2 on a Retina display, 1.25 at 125% scaling).
pub fn picture_size(available: (f32, f32), pixels_per_point: f32, pixel_aspect: f32) -> (f32, f32) {
    let width = picture_width(pixel_aspect);
    let height = NES_HEIGHT as f32;
    let (available_width, available_height) = (
        available.0 * pixels_per_point,
        available.1 * pixels_per_point,
    );
    let fits = (available_width / width).min(available_height / height);

    let default = default_window_size(pixel_aspect);
    // Within half a point, so a window at exactly its default size counts.
    let big_enough =
        available.0 + 0.5 >= default.width as f32 && available.1 + 0.5 >= default.height as f32;
    let scale = if big_enough {
        fits.floor().max(1.0)
    } else {
        fits
    };
    ((width * scale).round(), (height * scale).round())
}

#[cfg(test)]
mod tests {
    use super::*;

    const NTSC: f32 = 8.0 / 7.0;
    const PAL: f32 = 2_950_000.0 / 2_128_137.0;

    #[test]
    fn whole_rows_and_the_regions_shape_from_the_default_size_up() {
        for pixel_aspect in [NTSC, PAL] {
            let default = default_window_size(pixel_aspect);
            for pixels_per_point in [1.0, 1.25, 1.5, 2.0, 3.0] {
                for grow in [0.0, 37.0, 250.0, 800.0] {
                    let available = (default.width as f32 + grow, default.height as f32 + grow);
                    let (width, height) = picture_size(available, pixels_per_point, pixel_aspect);
                    let rows = height / NES_HEIGHT as f32;
                    assert_eq!(rows, rows.round(), "{rows} rows per NES row");
                    // Off by at most the rounding of the width to a whole pixel.
                    let exact_width = NES_WIDTH as f32 * pixel_aspect * rows;
                    assert!(
                        (width - exact_width).abs() <= 0.5 + 1e-3,
                        "{width} wide, {exact_width} exactly"
                    );
                    assert!(width <= available.0 * pixels_per_point + 0.5);
                    assert!(height <= available.1 * pixels_per_point);
                }
            }
        }
    }

    #[test]
    fn the_default_window_is_filled_exactly() {
        for pixel_aspect in [NTSC, PAL] {
            let default = default_window_size(pixel_aspect);
            let (width, height) = picture_size(
                (default.width as f32, default.height as f32),
                2.0,
                pixel_aspect,
            );
            assert_eq!(height, default.height as f32 * 2.0);
            assert!((width - default.width as f32 * 2.0).abs() <= 1.0);
        }
    }

    #[test]
    fn smaller_windows_are_filled_in_the_regions_shape() {
        let (width, height) = picture_size((500.0, 600.0), 1.0, NTSC);
        assert_eq!(width, 500.0);
        assert!((width / height - 256.0 * NTSC / 240.0).abs() < 0.01);
    }
}
