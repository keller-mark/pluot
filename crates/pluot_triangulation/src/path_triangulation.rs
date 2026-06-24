use crate::point;

#[derive(Debug, Default)]
pub struct PathTriangulation {
    pub triangles: Vec<point::Triangle>,
    pub centers: Vec<point::Point>,
    pub offsets: Vec<point::Vector>,
}

impl PathTriangulation {
    pub fn new() -> Self {
        PathTriangulation {
            triangles: Vec::new(),
            centers: Vec::new(),
            offsets: Vec::new(),
        }
    }

    pub fn reserve(&mut self, size: usize) {
        self.triangles.reserve(size);
        self.centers.reserve(size);
        self.offsets.reserve(size);
    }

    pub fn fix_triangle_orientation(&mut self) {
        for triangle in &mut self.triangles {
            let p1 = self.centers[triangle.x] + self.offsets[triangle.x];
            let p2 = self.centers[triangle.y] + self.offsets[triangle.y];
            let p3 = self.centers[triangle.z] + self.offsets[triangle.z];

            if (p2.x - p1.x) * (p3.y - p1.y) - (p2.y - p1.y) * (p3.x - p1.x) < 0.0 {
                std::mem::swap(&mut triangle.x, &mut triangle.z);
            }
        }
    }
}

fn add_triangles_for_join(
    triangles: &mut PathTriangulation,
    p1: point::Point,
    p2: point::Point,
    p3: point::Point,
    prev_length: point::Coord,
    cos_limit: point::Coord,
    bevel: bool,
) -> f32 {
    let idx = triangles.offsets.len();
    let mitter: point::Vector;
    let length = point::vector_length(p2, p3);
    let p1_p2_diff_norm = (p2 - p1) / prev_length;
    let p2_p3_diff_norm = (p3 - p2) / length;

    let cos_angle = p1_p2_diff_norm.x * p2_p3_diff_norm.x + p1_p2_diff_norm.y * p2_p3_diff_norm.y;
    let sin_angle = p1_p2_diff_norm.x * p2_p3_diff_norm.y - p1_p2_diff_norm.y * p2_p3_diff_norm.x;

    triangles.centers.push(p2);
    triangles.centers.push(p2);

    // Check sin_angle to compute mitter vector
    if sin_angle == 0.0 {
        mitter = point::Vector::new(p1_p2_diff_norm.y / 2.0, -p1_p2_diff_norm.x / 2.0);
    } else {
        let mut scale_factor = 1.0 / sin_angle;
        if bevel || cos_angle < cos_limit {
            // Compute bevel join and handle limits for inner vector length
            let (sign, mag) = sign_abs(scale_factor);
            scale_factor = sign * 0.5 * mag.min(prev_length.min(length));
        }
        mitter = (p1_p2_diff_norm - p2_p3_diff_norm) * scale_factor * 0.5;
    }

    if bevel || cos_angle < cos_limit {
        triangles.centers.push(p2);
        triangles
            .triangles
            .push(point::Triangle::new(idx, idx + 1, idx + 2));

        if sin_angle < 0.0 {
            triangles.offsets.push(mitter);
            triangles.offsets.push(point::Vector::new(
                -p1_p2_diff_norm.y * 0.5,
                p1_p2_diff_norm.x * 0.5,
            ));
            triangles.offsets.push(point::Vector::new(
                -p2_p3_diff_norm.y * 0.5,
                p2_p3_diff_norm.x * 0.5,
            ));
            triangles
                .triangles
                .push(point::Triangle::new(idx, idx + 2, idx + 3));
            triangles
                .triangles
                .push(point::Triangle::new(idx + 2, idx + 3, idx + 4));
        } else {
            triangles.offsets.push(point::Vector::new(
                p1_p2_diff_norm.y * 0.5,
                -p1_p2_diff_norm.x * 0.5,
            ));
            triangles.offsets.push(-mitter);
            triangles.offsets.push(point::Vector::new(
                p2_p3_diff_norm.y * 0.5,
                -p2_p3_diff_norm.x * 0.5,
            ));
            triangles
                .triangles
                .push(point::Triangle::new(idx + 1, idx + 2, idx + 3));
            triangles
                .triangles
                .push(point::Triangle::new(idx + 1, idx + 3, idx + 4));
        }
    } else {
        triangles.offsets.push(mitter);
        triangles.offsets.push(-mitter);
        triangles
            .triangles
            .push(point::Triangle::new(idx, idx + 1, idx + 2));
        triangles
            .triangles
            .push(point::Triangle::new(idx + 1, idx + 2, idx + 3));
    }

    length
}

// Helper function to calculate the sign and absolute value of a number
fn sign_abs(value: f32) -> (f32, f32) {
    if value < 0.0 {
        (-1.0, value.abs())
    } else {
        (1.0, value.abs())
    }
}

pub fn triangulate_path_edge(
    path: &[point::Point],
    closed: bool,
    limit: f32,
    bevel: bool,
) -> PathTriangulation {
    if path.len() < 2 {
        return PathTriangulation {
            triangles: vec![point::Triangle::new(0, 1, 3), point::Triangle::new(1, 3, 2)],
            centers: vec![path[0], path[0], path[0], path[0]],
            offsets: vec![
                point::Vector::new(0.0, 0.0),
                point::Vector::new(0.0, 0.0),
                point::Vector::new(0.0, 0.0),
                point::Vector::new(0.0, 0.0),
            ],
        };
    }

    let mut result = PathTriangulation::new();
    result.reserve(path.len() * 3);
    let cos_limit = 1.0 / (limit * limit / 2.0) - 1.0;
    let mut prev_length = if closed {
        point::vector_length(path[0], path[path.len() - 1])
    } else {
        point::vector_length(path[0], path[1])
    };

    if closed {
        prev_length = add_triangles_for_join(
            &mut result,
            path[path.len() - 1],
            path[0],
            path[1],
            prev_length,
            cos_limit,
            bevel,
        );
    } else {
        let norm_diff = (path[1] - path[0]) / prev_length;
        result.centers.push(path[0]);
        result.centers.push(path[0]);
        result
            .offsets
            .push(point::Vector::new(norm_diff.y * 0.5, -norm_diff.x * 0.5));
        result.offsets.push(-*result.offsets.last().unwrap());
        result.triangles.push(point::Triangle::new(0, 1, 2));
        result.triangles.push(point::Triangle::new(1, 2, 3));
    }

    for i in 1..path.len() - 1 {
        prev_length = add_triangles_for_join(
            &mut result,
            path[i - 1],
            path[i],
            path[i + 1],
            prev_length,
            cos_limit,
            bevel,
        );
    }

    if closed {
        add_triangles_for_join(
            &mut result,
            path[path.len() - 2],
            path[path.len() - 1],
            path[0],
            prev_length,
            cos_limit,
            bevel,
        );
        result.centers.push(result.centers[0]);
        result.centers.push(result.centers[0]);
        result.offsets.push(result.offsets[0]);
        result.offsets.push(result.offsets[1]);
    } else {
        let norm_diff = (path[path.len() - 1] - path[path.len() - 2]) / prev_length;
        result.centers.push(path[path.len() - 1]);
        result.centers.push(path[path.len() - 1]);
        result
            .offsets
            .push(point::Vector::new(norm_diff.y * 0.5, -norm_diff.x * 0.5));
        result.offsets.push(-*result.offsets.last().unwrap());
    }

    result.fix_triangle_orientation();
    result
}

/// For list of polygon edges (boundaries) generate its triangulation.
/// This function is to have consistent numeration of triangles.  
pub fn triangulate_paths_edge(
    paths: &[Vec<point::Point>],
    closed: bool,
    limit: f32,
    bevel: bool,
) -> PathTriangulation {
    let mut result = PathTriangulation::new();
    let mut shift = 0;
    for path in paths.iter() {
        let sub_res = triangulate_path_edge(path, closed, limit, bevel);
        let centers_len = sub_res.centers.len();
        result.centers.extend(sub_res.centers);
        result.offsets.extend(sub_res.offsets);
        result.triangles.extend(
            sub_res
                .triangles
                .into_iter()
                .map(|triangle| triangle.shifted_by(shift)),
        );
        shift += centers_len;
    }

    result
}
