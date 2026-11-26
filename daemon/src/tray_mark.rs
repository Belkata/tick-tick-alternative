//! The u-check mark of the app icon (`packaging/icon/utre-monochrome.svg`), rasterised for
//! the tray. Both trays draw it from the same geometry, so it stays in step with the app
//! icon: update `PATH` here when the master SVG's path changes.

/// Stroke width of the mark, in the SVG's path units.
const STROKE: f32 = 150.0;
/// The amber dot inside the u (the app icon's accent), as (cx, cy, r).
const DOT: (f32, f32, f32) = (512.0, 535.0, 72.0);
/// The stroked path's bounding box including the stroke: x 247..845, y 187..835.
const BOX_CENTER: (f32, f32) = (546.0, 511.0);
const BOX_HEIGHT: f32 = 648.0;

/// The path `M322 400 V570 A190 190 0 0 0 702 570 C706 480 735 360 770 262` as a polyline.
fn polyline() -> Vec<(f32, f32)> {
    let mut pts = vec![(322.0, 400.0), (322.0, 570.0)];
    // The half circle under the u, from its left end through the bottom to its right end.
    for i in 1..=24 {
        let a = std::f32::consts::PI * (1.0 - i as f32 / 24.0);
        pts.push((512.0 + 190.0 * a.cos(), 570.0 + 190.0 * a.sin()));
    }
    // The right arm, a cubic curve rising to the tick's tip.
    let (p0, p1, p2, p3) = (
        (702.0, 570.0),
        (706.0, 480.0),
        (735.0, 360.0),
        (770.0, 262.0),
    );
    for i in 1..=24 {
        let t = i as f32 / 24.0;
        let u = 1.0 - t;
        let w = [u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t];
        pts.push((
            w[0] * p0.0 + w[1] * p1.0 + w[2] * p2.0 + w[3] * p3.0,
            w[0] * p0.1 + w[1] * p1.1 + w[2] * p2.1 + w[3] * p3.1,
        ));
    }
    pts
}

fn seg_dist(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let t = (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / (vx * vx + vy * vy)).clamp(0.0, 1.0);
    ((a.0 + t * vx - p.0).hypot(a.1 + t * vy - p.1)).abs()
}

/// The mark scaled to fill `height` of a `size` x `size` square, centred.
pub struct Mark {
    pts: Vec<(f32, f32)>,
    /// Pixels per path unit.
    scale: f32,
    offset: (f32, f32),
}

impl Mark {
    pub fn new(size: f32, height: f32) -> Self {
        let scale = size * height / BOX_HEIGHT;
        Mark {
            pts: polyline(),
            scale,
            offset: (
                size / 2.0 - BOX_CENTER.0 * scale,
                size / 2.0 - BOX_CENTER.1 * scale,
            ),
        }
    }

    /// Coverage (0..1, antialiased over a pixel) of the u-check at pixel centre (`x`, `y`).
    pub fn stroke(&self, x: f32, y: f32) -> f32 {
        let p = (
            (x - self.offset.0) / self.scale,
            (y - self.offset.1) / self.scale,
        );
        let d = self
            .pts
            .windows(2)
            .map(|w| seg_dist(p, w[0], w[1]))
            .fold(f32::MAX, f32::min);
        ((STROKE / 2.0 - d) * self.scale + 0.5).clamp(0.0, 1.0)
    }

    /// Coverage of the dot inside the u.
    pub fn dot(&self, x: f32, y: f32) -> f32 {
        let p = (
            (x - self.offset.0) / self.scale,
            (y - self.offset.1) / self.scale,
        );
        ((DOT.2 - (p.0 - DOT.0).hypot(p.1 - DOT.1)) * self.scale + 0.5).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_the_stroke_and_leaves_the_corners_and_the_u_hollow_empty() {
        let m = Mark::new(100.0, 0.7);
        let at = |px: f32, py: f32| (m.offset.0 + px * m.scale, m.offset.1 + py * m.scale);
        let (x, y) = at(322.0, 480.0); // the left arm's centre line
        assert_eq!(m.stroke(x, y), 1.0);
        let (x, y) = at(512.0, 760.0); // the bottom of the curve
        assert_eq!(m.stroke(x, y), 1.0);
        assert_eq!(m.stroke(1.0, 1.0), 0.0);
        let (x, y) = at(512.0, 450.0); // inside the u, above the dot
        assert_eq!(m.stroke(x, y), 0.0);
        let (x, y) = at(512.0, 535.0);
        assert_eq!(m.dot(x, y), 1.0);
        assert_eq!(m.dot(1.0, 1.0), 0.0);
    }
}
