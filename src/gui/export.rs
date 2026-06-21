//! Raster export of the graph canvas — PNG and a minimal one-page PDF (with the
//! image embedded as a JPEG XObject, so no extra PDF dependency is needed).

use std::io::{self, Write};

fn to_io<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::new(io::ErrorKind::Other, e.to_string())
}

/// Save RGBA pixels (`w`×`h`) as a PNG file.
pub fn save_png(path: &str, rgba: &[u8], w: u32, h: u32) -> io::Result<()> {
    image::save_buffer(path, rgba, w, h, image::ExtendedColorType::Rgba8).map_err(to_io)
}

/// Fast, low-compression PNG — for throwaway video frames where encode speed
/// matters far more than file size (the default encoder is ~5× slower).
pub fn save_png_fast(path: &str, rgba: &[u8], w: u32, h: u32) -> io::Result<()> {
    use image::codecs::png::{PngEncoder, CompressionType, FilterType};
    use image::ImageEncoder;
    let file = std::io::BufWriter::new(std::fs::File::create(path)?);
    PngEncoder::new_with_quality(file, CompressionType::Fast, FilterType::NoFilter)
        .write_image(rgba, w, h, image::ExtendedColorType::Rgba8)
        .map_err(to_io)
}

/// Save RGBA pixels as a single-page PDF (image embedded as JPEG / DCTDecode).
pub fn save_pdf(path: &str, rgba: &[u8], w: u32, h: u32) -> io::Result<()> {
    // RGBA → RGB
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for px in rgba.chunks_exact(4) { rgb.extend_from_slice(&px[..3]); }

    // RGB → JPEG
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 88)
        .encode(&rgb, w, h, image::ExtendedColorType::Rgb8)
        .map_err(to_io)?;

    // Assemble the PDF, tracking byte offsets for the xref table.
    let mut buf: Vec<u8> = Vec::new();
    let mut offsets = [0usize; 6]; // index 1..=5 used

    let push = |buf: &mut Vec<u8>, s: &str| buf.extend_from_slice(s.as_bytes());
    push(&mut buf, "%PDF-1.4\n");

    offsets[1] = buf.len();
    push(&mut buf, "1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");

    offsets[2] = buf.len();
    push(&mut buf, "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");

    offsets[3] = buf.len();
    push(&mut buf, &format!(
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] \
         /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>\nendobj\n"));

    offsets[4] = buf.len();
    push(&mut buf, &format!(
        "4 0 obj\n<< /Type /XObject /Subtype /Image /Width {w} /Height {h} \
         /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n",
        jpeg.len()));
    buf.extend_from_slice(&jpeg);
    push(&mut buf, "\nendstream\nendobj\n");

    let content = format!("q {w} 0 0 {h} 0 0 cm /Im0 Do Q\n");
    offsets[5] = buf.len();
    push(&mut buf, &format!("5 0 obj\n<< /Length {} >>\nstream\n{content}endstream\nendobj\n",
        content.len()));

    let xref_pos = buf.len();
    push(&mut buf, "xref\n0 6\n0000000000 65535 f \n");
    for i in 1..=5 {
        push(&mut buf, &format!("{:010} 00000 n \n", offsets[i]));
    }
    push(&mut buf, &format!(
        "trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF\n"));

    let mut f = std::fs::File::create(path)?;
    f.write_all(&buf)
}
