use rand::thread_rng;
use rand_distr::{Normal, Distribution};

// Generates two arrays of length N representing X and Y coordinates
// from a 2D Gaussian distribution with mean (mean_x, mean_y) and
// standard deviations (sigma_x, sigma_y).
// Usage:
// let n = 10;
// let mean_x = 0.0;
// let mean_y = 0.0;
// let sigma_x = 1.0;
// let sigma_y = 1.0;
// let (xs, ys) = generate_gaussian_points(n, mean_x, mean_y, sigma_x, sigma_y);
// 
fn generate_gaussian_points(
    n: usize,
    mean_x: f64,
    mean_y: f64,
    sigma_x: f64,
    sigma_y: f64
) -> (Vec<f64>, Vec<f64>) {
    let mut rng = thread_rng();

    let normal_x = Normal::new(mean_x, sigma_x).expect("Invalid normal distribution for X");
    let normal_y = Normal::new(mean_y, sigma_y).expect("Invalid normal distribution for Y");

    let mut xs = Vec::with_capacity(n);
    let mut ys = Vec::with_capacity(n);

    for _ in 0..n {
        xs.push(normal_x.sample(&mut rng));
        ys.push(normal_y.sample(&mut rng));
    }

    (xs, ys)
}
