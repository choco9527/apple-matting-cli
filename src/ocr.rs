use std::ffi::CString;
use std::path::{Path, PathBuf};

pub const DEFAULT_LANGUAGES: &str = "zh-Hans,en-US";

#[cfg(target_os = "macos")]
extern "C" {
    fn ocr_process_image(
        input_path: *const std::os::raw::c_char,
        output_path: *const std::os::raw::c_char,
        languages: *const std::os::raw::c_char,
        json_output: bool,
    ) -> i32;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OcrFormat {
    #[default]
    Text,
    Json,
}

impl OcrFormat {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value.trim().to_ascii_lowercase().as_str() {
            "text" | "txt" => Ok(Self::Text),
            "json" => Ok(Self::Json),
            _ => Err(format!("Invalid OCR format: {value}. Use text or json")),
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Text => "txt",
            Self::Json => "json",
        }
    }

    fn is_json(self) -> bool {
        matches!(self, Self::Json)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    #[error("Image could not be loaded: {0}")]
    LoadFailed(String),

    #[error("OCR request failed")]
    RecognitionFailed,

    #[error("Could not write OCR output")]
    WriteFailed,

    #[error("PDF exceeds OCR limits (maximum 200 pages and 32 megapixels per page)")]
    PdfExceedsLimits,

    #[error("macOS 10.15 or later is required for OCR")]
    UnsupportedOs,

    #[error("Invalid path or language string")]
    InvalidString,

    #[error("Unsupported platform (macOS only)")]
    UnsupportedPlatform,

    #[error("Swift OCR bridge returned unknown error code: {0}")]
    Unknown(i32),
}

pub fn normalize_languages(value: &str) -> Result<String, String> {
    let languages = value
        .split(',')
        .map(str::trim)
        .filter(|language| !language.is_empty())
        .collect::<Vec<_>>();

    if languages.is_empty() {
        return Err("OCR languages cannot be empty".to_string());
    }

    Ok(languages.join(","))
}

pub fn derive_output_path(input_path: &str, format: OcrFormat) -> Option<PathBuf> {
    let path = Path::new(input_path);
    let stem = path.file_stem()?.to_str()?;
    let parent = path.parent().unwrap_or(Path::new("."));
    Some(parent.join(format!("{stem}_ocr.{}", format.extension())))
}

pub fn perform_ocr(
    input_path: &str,
    output_path: Option<&str>,
    languages: &str,
    format: OcrFormat,
) -> Result<String, OcrError> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (input_path, output_path, languages, format);
        return Err(OcrError::UnsupportedPlatform);
    }

    #[cfg(target_os = "macos")]
    {
        let output_path = output_path
            .map(PathBuf::from)
            .or_else(|| derive_output_path(input_path, format))
            .ok_or(OcrError::InvalidString)?;
        let output = output_path.to_str().ok_or(OcrError::InvalidString)?;
        let input = CString::new(input_path).map_err(|_| OcrError::InvalidString)?;
        let output_c = CString::new(output).map_err(|_| OcrError::InvalidString)?;
        let languages = CString::new(languages).map_err(|_| OcrError::InvalidString)?;

        // SAFETY: Swift performs the request synchronously and the C strings remain valid.
        let result = unsafe {
            ocr_process_image(
                input.as_ptr(),
                output_c.as_ptr(),
                languages.as_ptr(),
                format.is_json(),
            )
        };

        match result {
            0 => Ok(output.to_string()),
            -1 => Err(OcrError::LoadFailed(input_path.to_string())),
            -10 => Err(OcrError::RecognitionFailed),
            -11 => Err(OcrError::WriteFailed),
            -12 => Err(OcrError::PdfExceedsLimits),
            -99 => Err(OcrError::UnsupportedOs),
            code => Err(OcrError::Unknown(code)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{derive_output_path, normalize_languages, OcrFormat};

    #[test]
    fn normalizes_languages() {
        assert_eq!(
            normalize_languages(" zh-Hans, en-US "),
            Ok("zh-Hans,en-US".to_string())
        );
        assert!(normalize_languages(" , ").is_err());
    }

    #[test]
    fn derives_output_extension_from_format() {
        assert_eq!(
            derive_output_path("photo.png", OcrFormat::Json),
            Some("photo_ocr.json".into())
        );
    }
}
