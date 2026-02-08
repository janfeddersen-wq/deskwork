//! Attachment handling for drag-and-drop images.

use std::io::Cursor;
use std::path::Path;

use eframe::egui;
use image::{imageops::FilterType, DynamicImage, ImageFormat, ImageReader};

/// Maximum number of attachments allowed
pub const MAX_ATTACHMENTS: usize = 5;

/// Thumbnail size for preview
const THUMBNAIL_SIZE: u32 = 80;

/// Maximum image dimension before resizing for sending
const MAX_IMAGE_DIMENSION: u32 = 1024;

/// Supported image extensions
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp"];

/// Supported text-like document extensions for prompt intake
const TEXT_EXTENSIONS: &[&str] = &[
    "txt", "md", "markdown", "rst", "csv", "json", "yaml", "yml", "toml", "log", "xml", "html",
    "htm", "rs", "py", "js", "ts", "java", "c", "h", "cpp", "go", "sh", "sql",
];

/// A pending image attachment
#[derive(Clone)]
pub struct PendingImage {
    /// Original filename
    pub filename: String,
    /// Thumbnail texture for preview
    pub thumbnail: egui::TextureHandle,
    /// PNG bytes for sending (resized)
    pub data: Vec<u8>,
}

/// Check if a path is a supported image file
pub fn is_image_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Check if a path is a supported text file for prompt intake
pub fn is_text_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| TEXT_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
        .unwrap_or(false)
}

/// Read a text file and truncate to max_chars for prompt inclusion
pub fn read_text_file_for_prompt(path: &Path, max_chars: usize) -> anyhow::Result<String> {
    let bytes = std::fs::read(path)?;
    let mut content = String::from_utf8_lossy(&bytes).into_owned();

    if content.chars().count() > max_chars {
        content = content.chars().take(max_chars).collect();
    }

    Ok(content)
}

/// Process an image from a file path
pub fn process_image_from_path(path: &Path, ctx: &egui::Context) -> anyhow::Result<PendingImage> {
    let img = ImageReader::open(path)?.with_guessed_format()?.decode()?;

    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("image")
        .to_string();

    process_image_internal(img, filename, ctx)
}

/// Process an image from raw bytes
pub fn process_image_from_bytes(
    data: &[u8],
    filename: Option<String>,
    ctx: &egui::Context,
) -> anyhow::Result<PendingImage> {
    let img = ImageReader::new(Cursor::new(data))
        .with_guessed_format()?
        .decode()?;

    let filename = filename.unwrap_or_else(|| "pasted_image.png".to_string());
    process_image_internal(img, filename, ctx)
}

/// Internal image processing
fn process_image_internal(
    img: DynamicImage,
    filename: String,
    ctx: &egui::Context,
) -> anyhow::Result<PendingImage> {
    // Resize for sending if needed
    let processed = resize_to_fit(&img, MAX_IMAGE_DIMENSION);
    // Encode as PNG for sending
    let data = encode_as_png(&processed)?;

    // Create thumbnail for preview
    let thumbnail_img = img.thumbnail(THUMBNAIL_SIZE, THUMBNAIL_SIZE);
    let thumbnail = create_texture(ctx, &thumbnail_img, &filename);

    Ok(PendingImage {
        filename,
        thumbnail,
        data,
    })
}

/// Resize image if either dimension exceeds max_pixels
fn resize_to_fit(img: &DynamicImage, max_pixels: u32) -> DynamicImage {
    let (w, h) = (img.width(), img.height());

    if w <= max_pixels && h <= max_pixels {
        return img.clone();
    }

    let ratio = (max_pixels as f64) / (w.max(h) as f64);
    let new_w = ((w as f64) * ratio).round() as u32;
    let new_h = ((h as f64) * ratio).round() as u32;

    img.resize(new_w, new_h, FilterType::Triangle)
}

/// Encode image as PNG bytes
fn encode_as_png(img: &DynamicImage) -> anyhow::Result<Vec<u8>> {
    let mut buffer = Vec::new();
    let mut cursor = Cursor::new(&mut buffer);
    img.write_to(&mut cursor, ImageFormat::Png)?;
    Ok(buffer)
}

/// Create an egui texture from an image
fn create_texture(ctx: &egui::Context, img: &DynamicImage, name: &str) -> egui::TextureHandle {
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let pixels = rgba.into_raw();

    let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);

    ctx.load_texture(
        format!("attachment-{}", name),
        color_image,
        egui::TextureOptions::LINEAR,
    )
}

/// Render attachment previews above the input
pub fn render_attachments(attachments: &mut Vec<PendingImage>, ui: &mut egui::Ui) {
    if attachments.is_empty() {
        return;
    }

    ui.horizontal(|ui| {
        let mut to_remove = None;

        for (idx, attachment) in attachments.iter().enumerate() {
            ui.vertical(|ui| {
                // Thumbnail
                let img_size = egui::vec2(
                    attachment.thumbnail.size()[0] as f32,
                    attachment.thumbnail.size()[1] as f32,
                );
                ui.add(
                    egui::Image::new(&attachment.thumbnail)
                        .fit_to_exact_size(img_size)
                        .rounding(8.0),
                );

                // Filename + remove button
                ui.horizontal(|ui| {
                    let name = if attachment.filename.len() > 10 {
                        format!("{}...", &attachment.filename[..8])
                    } else {
                        attachment.filename.clone()
                    };
                    ui.label(egui::RichText::new(name).size(10.0));

                    if ui.small_button("×").clicked() {
                        to_remove = Some(idx);
                    }
                });
            });

            ui.add_space(8.0);
        }

        if let Some(idx) = to_remove {
            attachments.remove(idx);
        }
    });

    ui.add_space(8.0);

    // Attachment count
    let muted = crate::ui::colors::muted(ui.visuals());
    ui.label(
        egui::RichText::new(format!(
            "{}/{} attachments",
            attachments.len(),
            MAX_ATTACHMENTS
        ))
        .size(10.0)
        .color(muted),
    );

    ui.add_space(8.0);
}
