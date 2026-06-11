use kompari::{compare_images, load_image, ImageDifference};
use std::path::PathBuf;
use std::process;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: img_diff <left.png> <right.png>");
        process::exit(1);
    }

    let left_path = PathBuf::from(&args[1]);
    let right_path = PathBuf::from(&args[2]);

    let left = load_image(&left_path).unwrap_or_else(|e| {
        eprintln!("Error loading {}: {}", left_path.display(), e);
        process::exit(1);
    });
    let right = load_image(&right_path).unwrap_or_else(|e| {
        eprintln!("Error loading {}: {}", right_path.display(), e);
        process::exit(1);
    });

    match compare_images(&left, &right) {
        ImageDifference::None => {
            eprintln!("Images are identical.");
            println!("0");
        }
        ImageDifference::SizeMismatch { left_size, right_size } => {
            eprintln!(
                "Size mismatch: left={}x{}, right={}x{}",
                left_size.0, left_size.1, right_size.0, right_size.1
            );
            process::exit(1);
        }
        ImageDifference::Content { n_pixels, n_different_pixels, distance_sum, .. } => {
            eprintln!("{}/{} pixels differ", n_different_pixels, n_pixels);
            println!("{}", distance_sum);
        }
    }
}
