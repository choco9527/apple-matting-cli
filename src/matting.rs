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
    ) -> i32;
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
/// * `output_path` – where the transparent-background PNG should be saved;
///   if `None`, an `_nobg.png` sibling of `input_path` is used.
/// * `crop_to_subject` – if `true`, crop the output to the subject bounding box.
///
/// # Returns
/// The absolute path of the written output file.
pub fn perform_matting(
    input_path: &str,
    output_path: Option<&str>,
    crop_to_subject: bool,
) -> Result<String, MattingError> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (input_path, output_path, crop_to_subject);
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

        // SAFETY: Swift function is thread-safe (it creates its own Vision handler
        // and CIContext per call). Pointers are valid for the duration of the call.
        let ret =
            unsafe { matting_process_image(c_input.as_ptr(), c_output.as_ptr(), crop_to_subject) };

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
