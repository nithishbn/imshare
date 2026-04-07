#![allow(dead_code)]

use anyhow::Result;
use image::Luma;
use qrcode::QrCode;

pub fn generate_qr_code_terminal(url: &str) -> Result<String> {
    let code = QrCode::new(url.as_bytes())?;
    let string = code
        .render::<char>()
        .quiet_zone(false)
        .module_dimensions(2, 1)
        .build();

    Ok(string)
}

pub fn generate_qr_code_png(url: &str) -> Result<Vec<u8>> {
    let code = QrCode::new(url.as_bytes())?;
    let image = code.render::<Luma<u8>>().build();

    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    image::DynamicImage::ImageLuma8(image).write_to(&mut cursor, image::ImageFormat::Png)?;

    Ok(buf)
}
