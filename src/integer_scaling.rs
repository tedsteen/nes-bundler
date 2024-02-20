pub struct Ratios {
    pub x: u32,
    pub y: u32,
}

pub struct Size {
    pub width: u32,
    pub height: u32,
}

/// Calculates an integer scaling ratio common for X/Y axes (square pixels).
pub fn calculate_ratio(
    area_width: u32,
    area_height: u32,
    image_width: u32,
    image_height: u32,
) -> u32 {
    let (area_size, image_size);

    if area_height * image_width < area_width * image_height {
        area_size = area_height;
        image_size = image_height;
    } else {
        area_size = area_width;
        image_size = image_width;
    }

    let mut ratio = area_size / image_size;

    if ratio < 1 {
        ratio = 1;
    }

    ratio
}

/// Calculates integer scaling ratios potentially different for X/Y axes
/// as a result of aspect-ratio correction (rectangular pixels).
pub fn calculate_ratios(
    area_width: u32,
    area_height: u32,
    image_width: u32,
    image_height: u32,
    aspect_x: f64,
    aspect_y: f64,
) -> Ratios {
    if image_width as f64 * aspect_y == image_height as f64 * aspect_x {
        let ratio = calculate_ratio(area_width, area_height, image_width, image_height);

        return Ratios { x: ratio, y: ratio };
    }

    let max_ratio_x = area_width / image_width;
    let max_ratio_y = area_height / image_height;
    let max_width = image_width * max_ratio_x;
    let max_height = image_height * max_ratio_y;
    let max_width_aspect_y = max_width as f64 * aspect_y;
    let max_height_aspect_x = max_height as f64 * aspect_x;

    let mut ratio_x: u32;
    let mut ratio_y: u32;

    if max_width_aspect_y == max_height_aspect_x {
        ratio_x = max_ratio_x;
        ratio_y = max_ratio_y;
    } else {
        let max_aspect_less_than_target = max_width_aspect_y < max_height_aspect_x;

        let (ratio_a, max_size_a, image_size_b, aspect_a, aspect_b): (u32, u32, u32, f64, f64);

        if max_aspect_less_than_target {
            ratio_a = max_ratio_x;
            max_size_a = max_width;
            image_size_b = image_height;
            aspect_a = aspect_x;
            aspect_b = aspect_y;
        } else {
            ratio_a = max_ratio_y;
            max_size_a = max_height;
            image_size_b = image_width;
            aspect_a = aspect_y;
            aspect_b = aspect_x;
        }

        let ratio_bfract = max_size_a as f64 * aspect_b / aspect_a / image_size_b as f64;
        let ratio_bfloor = ratio_bfract.floor();
        let ratio_bceil = ratio_bfract.ceil();

        let mut par_floor = ratio_bfloor / ratio_a as f64;
        let mut par_ceil = ratio_bceil / ratio_a as f64;

        if max_aspect_less_than_target {
            par_floor = 1.0 / par_floor;
            par_ceil = 1.0 / par_ceil;
        }

        let common_factor = image_width as f64 * aspect_y / aspect_x / image_height as f64;
        let error_floor = (1.0 - common_factor * par_floor).abs();
        let error_ceil = (1.0 - common_factor * par_ceil).abs();

        let ratio_b = if (error_floor - error_ceil).abs() < 0.001 {
            if (ratio_a as f64 - ratio_bfloor).abs() < (ratio_a as f64 - ratio_bceil).abs() {
                ratio_bfloor as u32
            } else {
                ratio_bceil as u32
            }
        } else if error_floor < error_ceil {
            ratio_bfloor as u32
        } else {
            ratio_bceil as u32
        };

        if max_aspect_less_than_target {
            ratio_x = ratio_a;
            ratio_y = ratio_b;
        } else {
            ratio_x = ratio_b;
            ratio_y = ratio_a;
        }
    }

    if ratio_x < 1 {
        ratio_x = 1;
    }

    if ratio_y < 1 {
        ratio_y = 1;
    }

    Ratios {
        x: ratio_x,
        y: ratio_y,
    }
}

/// Calculates size (width and height) of scaled image
/// with aspect-ratio correction (rectangular pixels).
pub fn calculate_size_corrected(
    area_width: u32,
    area_height: u32,
    image_width: u32,
    image_height: u32,
    aspect_x: f64,
    aspect_y: f64,
) -> Size {
    let ratios = calculate_ratios(
        area_width,
        area_height,
        image_width,
        image_height,
        aspect_x,
        aspect_y,
    );

    Size {
        width: image_width * ratios.x,
        height: image_height * ratios.y,
    }
}
