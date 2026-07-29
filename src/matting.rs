use std::ffi::CString;
use std::path::{Path, PathBuf};

// ─── FFI declaration ────────────────────────────────────────────────────────
//
// Links to the `matting_process_image` symbol compiled from MattingBridge.swift.
// Only compiled on macOS; on other platforms the stub below is used instead.

#[cfg(target_os = "macos")]
extern "C" {
    /// See MattingBridge.swift for full documentation.
    /// Returns 0 on success, or a negative error code.
    fn matting_process_image(
        input_path: *const std::os::raw::c_char,
        output_path: *const std::os::raw::c_char,
        crop_to_subject: bool,
        background_red: f32,
        background_green: f32,
        background_blue: f32,
        background_alpha: f32,
    ) -> i32;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Background {
    #[default]
    Transparent,
    Solid {
        red: u8,
        green: u8,
        blue: u8,
    },
}

impl Background {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "transparent" => Ok(Self::Transparent),
            "white" => Ok(Self::Solid {
                red: 255,
                green: 255,
                blue: 255,
            }),
            "black" => Ok(Self::Solid {
                red: 0,
                green: 0,
                blue: 0,
            }),
            value => parse_hex_background(value).ok_or_else(|| {
                format!(
                    "Invalid background color: {value}. Use transparent, white, black, or #RRGGBB"
                )
            }),
        }
    }

    fn rgba(self) -> [f32; 4] {
        match self {
            Self::Transparent => [0.0, 0.0, 0.0, 0.0],
            Self::Solid { red, green, blue } => [
                f32::from(red) / 255.0,
                f32::from(green) / 255.0,
                f32::from(blue) / 255.0,
                1.0,
            ],
        }
    }
}

fn parse_hex_background(value: &str) -> Option<Background> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }

    Some(Background::Solid {
        red: u8::from_str_radix(&hex[0..2], 16).ok()?,
        green: u8::from_str_radix(&hex[2..4], 16).ok()?,
        blue: u8::from_str_radix(&hex[4..6], 16).ok()?,
    })
}

// ─── Error type ─────────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum MattingError {
    #[error("Image could not be loaded: {0}")]
    LoadFailed(String),

    #[error("Vision request failed")]
    VisionRequestFailed,

    #[error("No foreground found in image")]
    NoForeground,

    #[error("Mask generation failed")]
    MaskFailed,

    #[error("Blend filter failed")]
    BlendFailed,

    #[error("Could not write output PNG")]
    WriteFailed,

    #[error("Could not calculate crop bounds")]
    CropBoundsFailed,

    #[error("macOS 12.0 or later is required")]
    UnsupportedOs,

    #[error("Swift bridge returned unknown error code: {0}")]
    Unknown(i32),

    #[error("Invalid path string")]
    InvalidPath,

    #[error("Unsupported platform (macOS only)")]
    UnsupportedPlatform,
}

impl From<MattingError> for String {
    fn from(e: MattingError) -> Self {
        e.to_string()
    }
}

// ─── Output path helper ─────────────────────────────────────────────────────

/// Given `/some/dir/photo.jpg`, returns `/some/dir/photo_nobg.png`.
pub fn derive_output_path(input_path: &str) -> Option<PathBuf> {
    let p = Path::new(input_path);
    let stem = p.file_stem()?.to_str()?;
    let parent = p.parent().unwrap_or(Path::new("."));
    Some(parent.join(format!("{}_nobg.png", stem)))
}

// ─── Core matting function ─────────────────────────────────────────────────

/// Perform background removal on a single image.
///
/// # Arguments
/// * `input_path`  – absolute path to the source image
/// * `output_path` – where the PNG should be saved; if `None`, an `_nobg.png`
///   sibling of `input_path` is used.
/// * `crop_to_subject` – if `true`, crop the output to the subject bounding box.
/// * `background` – transparent by default, or one solid RGB color.
///
/// # Returns
/// The absolute path of the written output file.
pub fn perform_matting(
    input_path: &str,
    output_path: Option<&str>,
    crop_to_subject: bool,
    background: Background,
) -> Result<String, MattingError> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (input_path, output_path, crop_to_subject, background);
        return Err(MattingError::UnsupportedPlatform);
    }

    #[cfg(target_os = "macos")]
    {
        let out_path: PathBuf = match output_path {
            Some(p) => PathBuf::from(p),
            None => derive_output_path(input_path).ok_or(MattingError::InvalidPath)?,
        };

        let out_str = out_path
            .to_str()
            .ok_or(MattingError::InvalidPath)?
            .to_string();

        let c_input = CString::new(input_path).map_err(|_| MattingError::InvalidPath)?;
        let c_output = CString::new(out_str.as_str()).map_err(|_| MattingError::InvalidPath)?;
        let [red, green, blue, alpha] = background.rgba();

        // SAFETY: Swift creates a Vision handler per call and reuses a thread-safe
        // CIContext. Pointers are valid for the duration of the call.
        let ret = unsafe {
            matting_process_image(
                c_input.as_ptr(),
                c_output.as_ptr(),
                crop_to_subject,
                red,
                green,
                blue,
                alpha,
            )
        };

        match ret {
            0 => Ok(out_str),
            -1 => Err(MattingError::LoadFailed(input_path.to_string())),
            -2 => Err(MattingError::VisionRequestFailed),
            -3 => Err(MattingError::NoForeground),
            -4 => Err(MattingError::MaskFailed),
            -5 => Err(MattingError::BlendFailed),
            -6 => Err(MattingError::WriteFailed),
            -7 => Err(MattingError::CropBoundsFailed),
            -99 => Err(MattingError::UnsupportedOs),
            code => Err(MattingError::Unknown(code)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Background;

    #[test]
    fn parses_named_backgrounds() {
        assert_eq!(
            Background::parse("transparent"),
            Ok(Background::Transparent)
        );
        assert_eq!(
            Background::parse("WHITE"),
            Ok(Background::Solid {
                red: 255,
                green: 255,
                blue: 255,
            })
        );
        assert_eq!(
            Background::parse("black"),
            Ok(Background::Solid {
                red: 0,
                green: 0,
                blue: 0,
            })
        );
    }

    #[test]
    fn parses_hex_background() {
        assert_eq!(
            Background::parse("#12aBcF"),
            Ok(Background::Solid {
                red: 0x12,
                green: 0xab,
                blue: 0xcf,
            })
        );
    }

    #[test]
    fn rejects_invalid_background() {
        assert!(Background::parse("red").is_err());
        assert!(Background::parse("#fff").is_err());
        assert!(Background::parse("112233").is_err());
    }
}
