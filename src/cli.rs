use std::net::SocketAddr;
use std::path::Path;

use axum::extract::Multipart;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use axum::{Json, Router};
use serde::Serialize;
use tower_http::cors::CorsLayer;

use crate::batch::{perform_batch, BatchSummary};
use crate::matting::perform_matting;

const DEFAULT_SERVER_PORT: u16 = 8080;
const DEFAULT_BATCH_JOBS: usize = 3;
const MAX_BATCH_JOBS: usize = 64;
const USAGE: &str = "Usage:
  apple-matting-cli <input-image> [-o|--output <output-png>] [--crop]
  apple-matting-cli --batch <input-dir> -o <output-dir> [--crop] [--recursive] [--jobs <count>]
  apple-matting-cli --server [--port <port>]
  apple-matting-cli --version";

#[derive(Debug, PartialEq, Eq)]
pub enum CliArgs {
    Help,
    Version,
    Batch {
        input_dir: String,
        output_dir: String,
        crop_to_subject: bool,
        recursive: bool,
        jobs: usize,
    },
    Run {
        input_path: String,
        output_path: Option<String>,
        crop_to_subject: bool,
    },
    Server {
        port: u16,
    },
}

pub fn parse_args(args: &[String]) -> Result<CliArgs, String> {
    if args.len() == 2 && matches!(args[1].as_str(), "-h" | "--help") {
        return Ok(CliArgs::Help);
    }

    if args.len() == 2 && matches!(args[1].as_str(), "-V" | "--version") {
        return Ok(CliArgs::Version);
    }

    let has_server = args.iter().skip(1).any(|arg| arg == "--server");
    let has_batch = args.iter().skip(1).any(|arg| arg == "--batch");
    if has_server && has_batch {
        return Err(USAGE.to_string());
    }
    if has_server {
        return parse_server_args(args);
    }
    if has_batch {
        return parse_batch_args(args);
    }

    parse_run_args(args)
}

fn parse_run_args(args: &[String]) -> Result<CliArgs, String> {
    let mut crop_to_subject = false;
    let mut input_path: Option<String> = None;
    let mut output_path: Option<String> = None;
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--crop" => {
                crop_to_subject = true;
                index += 1;
            }
            "-o" | "--output" => {
                if input_path.is_none() {
                    return Err(USAGE.to_string());
                }

                if output_path.is_some() {
                    return Err(USAGE.to_string());
                }

                let value = args.get(index + 1).ok_or_else(|| USAGE.to_string())?;
                if value.starts_with('-') {
                    return Err(USAGE.to_string());
                }

                output_path = Some(value.clone());
                index += 2;
            }
            arg if arg.starts_with('-') => return Err(USAGE.to_string()),
            value => {
                if input_path.is_none() {
                    input_path = Some(value.to_string());
                } else if output_path.is_none() {
                    output_path = Some(value.to_string());
                } else {
                    return Err(USAGE.to_string());
                }

                index += 1;
            }
        }
    }

    let input_path = input_path.ok_or_else(|| USAGE.to_string())?;

    Ok(CliArgs::Run {
        input_path,
        output_path,
        crop_to_subject,
    })
}

fn parse_server_args(args: &[String]) -> Result<CliArgs, String> {
    let mut port = DEFAULT_SERVER_PORT;
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--server" => {
                index += 1;
            }
            "-p" | "--port" => {
                let value = args.get(index + 1).ok_or_else(|| USAGE.to_string())?;
                port = value
                    .parse::<u16>()
                    .map_err(|_| format!("Invalid port: {value}"))?;
                index += 2;
            }
            _ => return Err(USAGE.to_string()),
        }
    }

    Ok(CliArgs::Server { port })
}

fn parse_batch_args(args: &[String]) -> Result<CliArgs, String> {
    let mut input_dir = None;
    let mut output_dir = None;
    let mut crop_to_subject = false;
    let mut recursive = false;
    let mut jobs = DEFAULT_BATCH_JOBS;
    let mut index = 1;

    while index < args.len() {
        match args[index].as_str() {
            "--batch" => {
                let value = option_value(args, index)?;
                if input_dir.replace(value).is_some() {
                    return Err(USAGE.to_string());
                }
                index += 2;
            }
            "-o" | "--output" => {
                let value = option_value(args, index)?;
                if output_dir.replace(value).is_some() {
                    return Err(USAGE.to_string());
                }
                index += 2;
            }
            "--crop" => {
                crop_to_subject = true;
                index += 1;
            }
            "-r" | "--recursive" => {
                recursive = true;
                index += 1;
            }
            "-j" | "--jobs" => {
                let value = option_value(args, index)?;
                jobs = parse_jobs(&value)?;
                index += 2;
            }
            _ => return Err(USAGE.to_string()),
        }
    }

    Ok(CliArgs::Batch {
        input_dir: input_dir.ok_or_else(|| USAGE.to_string())?,
        output_dir: output_dir.ok_or_else(|| USAGE.to_string())?,
        crop_to_subject,
        recursive,
        jobs,
    })
}

fn option_value(args: &[String], index: usize) -> Result<String, String> {
    let value = args.get(index + 1).ok_or_else(|| USAGE.to_string())?;
    if value.starts_with('-') {
        return Err(USAGE.to_string());
    }
    Ok(value.clone())
}

fn parse_jobs(value: &str) -> Result<usize, String> {
    let jobs = value
        .parse::<usize>()
        .map_err(|_| format!("Invalid jobs count: {value}"))?;
    if !(1..=MAX_BATCH_JOBS).contains(&jobs) {
        return Err(format!("Jobs count must be between 1 and {MAX_BATCH_JOBS}"));
    }
    Ok(jobs)
}

pub async fn run(args: Vec<String>) -> i32 {
    match parse_args(&args) {
        Ok(CliArgs::Help) => {
            println!("{USAGE}");
            0
        }
        Ok(CliArgs::Version) => {
            println!("apple-matting-cli {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Ok(CliArgs::Batch {
            input_dir,
            output_dir,
            crop_to_subject,
            recursive,
            jobs,
        }) => match perform_batch(&input_dir, &output_dir, crop_to_subject, recursive, jobs) {
            Ok(summary) => report_batch(summary),
            Err(error) => {
                eprintln!("{error}");
                1
            }
        },
        Ok(CliArgs::Run {
            input_path,
            output_path,
            crop_to_subject,
        }) => match perform_matting(&input_path, output_path.as_deref(), crop_to_subject) {
            Ok(path) => {
                println!("{path}");
                0
            }
            Err(error) => {
                eprintln!("{error}");
                1
            }
        },
        Ok(CliArgs::Server { port }) => match run_server(port).await {
            Ok(()) => 0,
            Err(error) => {
                eprintln!("{error}");
                1
            }
        },
        Err(message) => {
            eprintln!("{message}");
            2
        }
    }
}

fn report_batch(summary: BatchSummary) -> i32 {
    for result in &summary.results {
        if let Some(error) = &result.error {
            eprintln!("{}: {error}", result.input_path.display());
        } else {
            println!("{}", result.output_path.display());
        }
    }
    eprintln!(
        "Processed {} image(s): {} succeeded, {} failed",
        summary.total, summary.succeeded, summary.failed
    );
    i32::from(summary.failed > 0)
}

async fn run_server(port: u16) -> Result<(), String> {
    let address = SocketAddr::from(([0, 0, 0, 0], port));
    let app = Router::new()
        .route("/matting", post(handle_matting))
        .layer(CorsLayer::permissive());
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .map_err(|error| format!("Could not bind server to {address}: {error}"))?;

    eprintln!("apple-matting-cli server listening on http://{address}");
    eprintln!("POST multipart/form-data to /matting with field name `file`");

    axum::serve(listener, app)
        .await
        .map_err(|error| format!("Server error: {error}"))
}

async fn handle_matting(mut multipart: Multipart) -> Result<Response, ApiError> {
    let mut crop_to_subject = false;
    let mut uploaded_file: Option<UploadedFile> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(ApiError::bad_request)?
    {
        let name = field.name().unwrap_or("");

        if name == "crop" {
            let value = field.text().await.map_err(ApiError::bad_request)?;
            crop_to_subject = matches!(value.trim().to_lowercase().as_str(), "true" | "1" | "yes");
            continue;
        }

        if name != "file" {
            continue;
        }

        let extension = input_extension(field.file_name());
        let bytes = field.bytes().await.map_err(ApiError::bad_request)?;
        uploaded_file = Some(UploadedFile {
            extension,
            bytes: bytes.to_vec(),
        });
    }

    let uploaded_file = uploaded_file
        .ok_or_else(|| ApiError::new(StatusCode::BAD_REQUEST, "Missing multipart field `file`"))?;

    let temp_dir = tempfile::tempdir().map_err(ApiError::internal)?;
    let input_path = temp_dir
        .path()
        .join(format!("input.{}", uploaded_file.extension));
    let output_path = temp_dir.path().join("output.png");

    tokio::fs::write(&input_path, uploaded_file.bytes)
        .await
        .map_err(ApiError::internal)?;

    let input_string = input_path.to_string_lossy().to_string();
    let output_string = output_path.to_string_lossy().to_string();

    let matting_result = tokio::task::spawn_blocking(move || {
        perform_matting(&input_string, Some(&output_string), crop_to_subject)
    })
    .await
    .map_err(ApiError::internal)?;

    matting_result.map_err(ApiError::matting)?;

    let output_bytes = tokio::fs::read(&output_path)
        .await
        .map_err(ApiError::internal)?;

    Ok(([(header::CONTENT_TYPE, "image/png")], output_bytes).into_response())
}

struct UploadedFile {
    extension: &'static str,
    bytes: Vec<u8>,
}

fn input_extension(file_name: Option<&str>) -> &'static str {
    let Some(file_name) = file_name else {
        return "jpg";
    };

    match Path::new(file_name)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "png",
        Some("webp") => "webp",
        Some("bmp") => "bmp",
        _ => "jpg",
    }
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

#[derive(Serialize)]
struct ErrorBody {
    error: String,
}

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    fn bad_request(error: impl std::fmt::Display) -> Self {
        Self::new(StatusCode::BAD_REQUEST, error.to_string())
    }

    fn internal(error: impl std::fmt::Display) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
    }

    fn matting(error: impl std::fmt::Display) -> Self {
        Self::new(StatusCode::UNPROCESSABLE_ENTITY, error.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                error: self.message,
            }),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_args, CliArgs, DEFAULT_BATCH_JOBS, DEFAULT_SERVER_PORT, USAGE};

    #[test]
    fn parses_input_only() {
        let args = vec!["apple-matting-cli".to_string(), "input.jpg".to_string()];

        assert_eq!(
            parse_args(&args),
            Ok(CliArgs::Run {
                input_path: "input.jpg".to_string(),
                output_path: None,
                crop_to_subject: false,
            })
        );
    }

    #[test]
    fn parses_input_and_output() {
        let args = vec![
            "apple-matting-cli".to_string(),
            "input.jpg".to_string(),
            "output.png".to_string(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(CliArgs::Run {
                input_path: "input.jpg".to_string(),
                output_path: Some("output.png".to_string()),
                crop_to_subject: false,
            })
        );
    }

    #[test]
    fn parses_short_output_flag() {
        let args = vec![
            "apple-matting-cli".to_string(),
            "input.jpg".to_string(),
            "-o".to_string(),
            "output.png".to_string(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(CliArgs::Run {
                input_path: "input.jpg".to_string(),
                output_path: Some("output.png".to_string()),
                crop_to_subject: false,
            })
        );
    }

    #[test]
    fn parses_long_output_flag() {
        let args = vec![
            "apple-matting-cli".to_string(),
            "input.jpg".to_string(),
            "--output".to_string(),
            "output.png".to_string(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(CliArgs::Run {
                input_path: "input.jpg".to_string(),
                output_path: Some("output.png".to_string()),
                crop_to_subject: false,
            })
        );
    }

    #[test]
    fn parses_crop_flag() {
        let args = vec![
            "apple-matting-cli".to_string(),
            "input.jpg".to_string(),
            "--crop".to_string(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(CliArgs::Run {
                input_path: "input.jpg".to_string(),
                output_path: None,
                crop_to_subject: true,
            })
        );
    }

    #[test]
    fn parses_crop_with_output() {
        let args = vec![
            "apple-matting-cli".to_string(),
            "input.jpg".to_string(),
            "--crop".to_string(),
            "-o".to_string(),
            "output.png".to_string(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(CliArgs::Run {
                input_path: "input.jpg".to_string(),
                output_path: Some("output.png".to_string()),
                crop_to_subject: true,
            })
        );
    }

    #[test]
    fn parses_server_with_default_port() {
        let args = vec!["apple-matting-cli".to_string(), "--server".to_string()];

        assert_eq!(
            parse_args(&args),
            Ok(CliArgs::Server {
                port: DEFAULT_SERVER_PORT,
            })
        );
    }

    #[test]
    fn parses_server_with_port() {
        let args = vec![
            "apple-matting-cli".to_string(),
            "--server".to_string(),
            "--port".to_string(),
            "9090".to_string(),
        ];

        assert_eq!(parse_args(&args), Ok(CliArgs::Server { port: 9090 }));
    }

    #[test]
    fn parses_server_with_short_port() {
        let args = vec![
            "apple-matting-cli".to_string(),
            "--server".to_string(),
            "-p".to_string(),
            "9090".to_string(),
        ];

        assert_eq!(parse_args(&args), Ok(CliArgs::Server { port: 9090 }));
    }

    #[test]
    fn parses_batch_with_defaults() {
        let args = vec![
            "apple-matting-cli".to_string(),
            "--batch".to_string(),
            "input".to_string(),
            "-o".to_string(),
            "output".to_string(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(CliArgs::Batch {
                input_dir: "input".to_string(),
                output_dir: "output".to_string(),
                crop_to_subject: false,
                recursive: false,
                jobs: DEFAULT_BATCH_JOBS,
            })
        );
    }

    #[test]
    fn parses_batch_options() {
        let args = vec![
            "apple-matting-cli".to_string(),
            "--batch".to_string(),
            "input".to_string(),
            "--output".to_string(),
            "output".to_string(),
            "--crop".to_string(),
            "--recursive".to_string(),
            "--jobs".to_string(),
            "5".to_string(),
        ];

        assert_eq!(
            parse_args(&args),
            Ok(CliArgs::Batch {
                input_dir: "input".to_string(),
                output_dir: "output".to_string(),
                crop_to_subject: true,
                recursive: true,
                jobs: 5,
            })
        );
    }

    #[test]
    fn rejects_batch_without_output_directory() {
        let args = vec![
            "apple-matting-cli".to_string(),
            "--batch".to_string(),
            "input".to_string(),
        ];

        assert_eq!(parse_args(&args), Err(USAGE.to_string()));
    }

    #[test]
    fn rejects_zero_batch_jobs() {
        let args = vec![
            "apple-matting-cli".to_string(),
            "--batch".to_string(),
            "input".to_string(),
            "-o".to_string(),
            "output".to_string(),
            "--jobs".to_string(),
            "0".to_string(),
        ];

        assert_eq!(
            parse_args(&args),
            Err("Jobs count must be between 1 and 64".to_string())
        );
    }

    #[test]
    fn parses_help() {
        let args = vec!["apple-matting-cli".to_string(), "--help".to_string()];

        assert_eq!(parse_args(&args), Ok(CliArgs::Help));
    }

    #[test]
    fn parses_version() {
        let args = vec!["apple-matting-cli".to_string(), "--version".to_string()];

        assert_eq!(parse_args(&args), Ok(CliArgs::Version));
    }

    #[test]
    fn rejects_invalid_argument_count() {
        let args = vec![
            "apple-matting-cli".to_string(),
            "input.jpg".to_string(),
            "output.png".to_string(),
            "extra".to_string(),
        ];

        assert_eq!(parse_args(&args), Err(USAGE.to_string()));
    }

    #[test]
    fn rejects_output_flag_without_value() {
        let args = vec![
            "apple-matting-cli".to_string(),
            "input.jpg".to_string(),
            "-o".to_string(),
        ];

        assert_eq!(parse_args(&args), Err(USAGE.to_string()));
    }

    #[test]
    fn rejects_output_flag_before_input() {
        let args = vec![
            "apple-matting-cli".to_string(),
            "-o".to_string(),
            "output.png".to_string(),
            "input.jpg".to_string(),
        ];

        assert_eq!(parse_args(&args), Err(USAGE.to_string()));
    }

    #[test]
    fn rejects_invalid_server_port() {
        let args = vec![
            "apple-matting-cli".to_string(),
            "--server".to_string(),
            "--port".to_string(),
            "not-a-port".to_string(),
        ];

        assert_eq!(
            parse_args(&args),
            Err("Invalid port: not-a-port".to_string())
        );
    }
}
