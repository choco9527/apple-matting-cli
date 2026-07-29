use super::{parse_args, CliArgs, USAGE};
use crate::matting::Background;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

#[test]
fn parses_background_color() {
    let parsed = parse_args(&args(&[
        "apple-matting-cli",
        "input.jpg",
        "--background",
        "#12abcf",
    ]));

    assert_eq!(
        parsed,
        Ok(CliArgs::Run {
            input_path: "input.jpg".to_string(),
            output_path: None,
            crop_to_subject: false,
            background: Background::Solid {
                red: 0x12,
                green: 0xab,
                blue: 0xcf,
            },
        })
    );
}

#[test]
fn rejects_invalid_background_color() {
    let parsed = parse_args(&args(&[
        "apple-matting-cli",
        "input.jpg",
        "--background",
        "red",
    ]));

    assert_eq!(
        parsed,
        Err("Invalid background color: red. Use transparent, white, black, or #RRGGBB".to_string())
    );
}

#[test]
fn rejects_duplicate_batch_background() {
    let parsed = parse_args(&args(&[
        "apple-matting-cli",
        "--batch",
        "input",
        "-o",
        "output",
        "--background",
        "white",
        "--background",
        "black",
    ]));

    assert_eq!(parsed, Err(USAGE.to_string()));
}
