//! Ports `vital/src/common/line_generator.{h,cpp}` (Copyright 2013-2019
//! Matt Tytel, GPL-3.0-or-later): the editable line shape used for LFO
//! shapes, modulation remapping curves and the wavetable Line Source.

use crate::json::{self, Value};
use crate::scalar::{self, PI};

/// `LineGenerator`.
#[derive(Clone, Debug)]
pub struct LineGenerator {
    name: String,
    last_browsed_file: String,
    points: [(f32, f32); Self::MAX_POINTS],
    powers: [f32; Self::MAX_POINTS],
    num_points: usize,
    resolution: usize,
    buffer: Vec<f32>,
    looped: bool,
    smooth: bool,
    linear: bool,
    render_count: u32,
}

impl Default for LineGenerator {
    fn default() -> Self {
        Self::new(Self::DEFAULT_RESOLUTION)
    }
}

impl LineGenerator {
    pub const MAX_POINTS: usize = 100;
    pub const DEFAULT_RESOLUTION: usize = 2048;
    pub const EXTRA_VALUES: usize = 3;

    /// `LineGenerator::smoothTransition`.
    #[inline]
    pub fn smooth_transition(t: f32) -> f32 {
        0.5 * ((t - 0.5) * PI).sin() + 0.5
    }

    /// `LineGenerator(resolution)`: starts as the linear ramp.
    pub fn new(resolution: usize) -> Self {
        let mut generator = Self {
            name: String::new(),
            last_browsed_file: String::new(),
            points: [(0.0, 0.0); Self::MAX_POINTS],
            powers: [0.0; Self::MAX_POINTS],
            num_points: 2,
            resolution,
            buffer: vec![0.0; resolution + Self::EXTRA_VALUES],
            looped: false,
            smooth: false,
            linear: true,
            render_count: 0,
        };
        generator.init_linear();
        generator
    }

    fn init_shape(&mut self, points: &[(f32, f32)], linear: bool, name: &str, smooth: bool) {
        self.num_points = points.len();
        self.points[..points.len()].copy_from_slice(points);
        for power in &mut self.powers[..points.len()] {
            *power = 0.0;
        }
        self.linear = linear;
        self.name = name.to_string();
        self.smooth = smooth;
        self.render();
    }

    pub fn init_linear(&mut self) {
        self.init_shape(&[(0.0, 1.0), (1.0, 0.0)], true, "Linear", false);
    }

    pub fn init_triangle(&mut self) {
        self.init_shape(
            &[(0.0, 1.0), (0.5, 0.0), (1.0, 1.0)],
            false,
            "Triangle",
            false,
        );
    }

    pub fn init_square(&mut self) {
        self.init_shape(
            &[(0.0, 1.0), (0.0, 0.0), (0.5, 0.0), (0.5, 1.0), (1.0, 1.0)],
            false,
            "Square",
            false,
        );
    }

    pub fn init_sin(&mut self) {
        self.init_shape(&[(0.0, 1.0), (0.5, 0.0), (1.0, 1.0)], false, "Sin", true);
    }

    pub fn init_saw_up(&mut self) {
        self.init_shape(
            &[(0.0, 1.0), (1.0, 0.0), (1.0, 1.0)],
            false,
            "Saw Up",
            false,
        );
    }

    pub fn init_saw_down(&mut self) {
        self.init_shape(
            &[(0.0, 0.0), (1.0, 1.0), (1.0, 0.0)],
            false,
            "Saw Down",
            false,
        );
    }

    pub fn set_loop(&mut self, looped: bool) {
        self.looped = looped;
        self.render();
    }

    pub fn set_name(&mut self, name: &str) {
        self.name = name.to_string();
    }

    pub fn set_last_browsed_file(&mut self, path: &str) {
        self.last_browsed_file = path.to_string();
    }

    pub fn set_smooth(&mut self, smooth: bool) {
        self.smooth = smooth;
        self.check_line_is_linear();
        self.render();
    }

    /// `render()`: fills the lookup buffer at the generator's resolution.
    pub fn render(&mut self) {
        self.render_count = self.render_count.wrapping_add(1);

        let num_points = self.num_points;
        let mut point_index = 0usize;
        let mut last_point = self.points[point_index];
        let mut current_power = 0.0f32;
        let mut current_point = self.points[point_index];
        if self.looped {
            last_point = self.points[num_points.wrapping_sub(1)];
            last_point.0 -= 1.0;
            current_power = self.powers[num_points.wrapping_sub(1)];
        }

        let resolution = self.resolution;
        for i in 0..resolution {
            let x = i as f32 / (resolution as f32 - 1.0);
            let mut t = 1.0f32;
            if current_point.0 > last_point.0 {
                t = (x - last_point.0) / (current_point.0 - last_point.0);
            }

            if self.smooth {
                t = Self::smooth_transition(t);
            }

            t = scalar::clamp(scalar::power_scale(t, current_power), 0.0, 1.0);

            let y = last_point.1 + t * (current_point.1 - last_point.1);
            self.buffer[i + 1] = 1.0 - y;

            while x > current_point.0 && point_index < num_points {
                current_power = self.powers[point_index % num_points];
                point_index += 1;
                last_point = current_point;
                current_point = self.points[point_index % num_points];
                if point_index >= num_points {
                    current_point.0 += 1.0;
                    break;
                }
            }
        }

        if self.looped {
            self.buffer[0] = self.buffer[resolution];
            self.buffer[resolution + 1] = self.buffer[1];
            self.buffer[resolution + 2] = self.buffer[2];
        } else {
            self.buffer[0] = self.buffer[1];
            self.buffer[resolution + 1] = self.buffer[resolution];
            self.buffer[resolution + 2] = self.buffer[resolution];
        }
    }

    /// `stateToJson()`.
    pub fn to_json(&self) -> Value {
        let mut point_data = Value::Null;
        let mut power_data = Value::Null;
        for i in 0..self.num_points {
            let (x, y) = self.points[i];
            point_data.push(x);
            point_data.push(y);
            power_data.push(self.powers[i]);
        }

        let mut data = Value::Null;
        data.set("num_points", self.num_points as i32);
        data.set("points", point_data);
        data.set("powers", power_data);
        data.set("name", self.name.as_str());
        data.set("smooth", self.smooth);
        data
    }

    /// `isValidJson(data)`.
    pub fn is_valid_json(data: &Value) -> bool {
        if data.count("num_points") == 0 || data.count("points") == 0 || data.count("powers") == 0 {
            return false;
        }
        let point_data = data.get("points").expect("object");
        let power_data = data.get("powers").expect("object");
        point_data.is_array() && power_data.is_array()
    }

    /// `jsonToState(data)`. Reads `min(100, powers.len())` points, keeps
    /// `num_points` from the data, fills in an empty name and `smooth =
    /// false` when missing, then re-renders. On error (where upstream would
    /// throw) the generator is left unchanged.
    pub fn from_json(&mut self, data: &Value) -> json::Result<()> {
        let num_points = data.get("num_points")?.to_i32()?;
        if !(0..=Self::MAX_POINTS as i32).contains(&num_points) {
            return Err(json::JsonError(format!(
                "line generator num_points {num_points} out of range"
            )));
        }
        let point_data = data.get("points")?;
        let power_data = data.get("powers")?;

        let mut name = String::new();
        if data.count("name") != 0 {
            name = data.get("name")?.to_str()?.to_string();
        }
        let mut smooth = false;
        if data.count("smooth") != 0 {
            smooth = data.get("smooth")?.to_bool()?;
        }

        let num_read = Self::MAX_POINTS.min(power_data.size());
        let mut points = self.points;
        let mut powers = self.powers;
        for i in 0..num_read {
            points[i] = (
                point_data.at(2 * i)?.to_f32()?,
                point_data.at(2 * i + 1)?.to_f32()?,
            );
            powers[i] = power_data.at(i)?.to_f32()?;
        }

        self.num_points = num_points as usize;
        self.name = name;
        self.smooth = smooth;
        self.points = points;
        self.powers = powers;
        self.check_line_is_linear();
        self.render();
        Ok(())
    }

    /// `valueAtPhase(phase)`: linear lookup into the rendered buffer.
    pub fn value_at_phase(&self, phase: f32) -> f32 {
        let scaled_phase = scalar::clamp(phase, 0.0, 1.0) * self.resolution as f32;
        let index = scaled_phase as usize;
        scalar::interpolate(
            self.buffer[index + 1],
            self.buffer[index + 2],
            scaled_phase - index as f32,
        )
    }

    /// `checkLineIsLinear()`.
    pub fn check_line_is_linear(&mut self) {
        self.linear = !self.smooth
            && self.num_points == 2
            && self.powers[0] == 0.0
            && self.points[0] == (0.0, 1.0)
            && self.points[1] == (1.0, 0.0);
    }

    /// `getValueBetweenPoints(x, index_from, index_to)`.
    pub fn value_between_points(&self, x: f32, index_from: usize, index_to: usize) -> f32 {
        let first = self.points[index_from];
        let second = self.points[index_to];
        let power = self.powers[index_from];

        let width = second.0 - first.0;
        if width <= 0.0 {
            return second.1;
        }

        let mut t = (x - first.0) / width;
        if self.smooth {
            t = Self::smooth_transition(t);
        }
        t = scalar::clamp(scalar::power_scale(t, power), 0.0, 1.0);
        t * (second.1 - first.1) + first.1
    }

    /// `getValueAtPhase(phase)`: evaluates the line directly.
    pub fn value_at_phase_exact(&self, phase: f32) -> f32 {
        for i in 0..self.num_points.saturating_sub(1) {
            if self.points[i].0 <= phase && self.points[i + 1].0 >= phase {
                return self.value_between_points(phase, i, i + 1);
            }
        }
        self.last_point().1
    }

    /// `addPoint(index, position)`.
    pub fn add_point(&mut self, index: usize, position: (f32, f32)) {
        let mut i = self.num_points;
        while i > index {
            self.points[i] = self.points[i - 1];
            self.powers[i] = self.powers[i - 1];
            i -= 1;
        }
        self.num_points += 1;
        self.points[index] = position;
        self.powers[index] = 0.0;
        self.check_line_is_linear();
    }

    /// `addMiddlePoint(index)`.
    pub fn add_middle_point(&mut self, index: usize) {
        let x = (self.points[index - 1].0 + self.points[index].0) * 0.5;
        let y = self.value_between_points(x, index - 1, index);
        self.add_point(index, (x, y));
    }

    /// `removePoint(index)`.
    pub fn remove_point(&mut self, index: usize) {
        self.num_points -= 1;
        for i in index..self.num_points {
            self.points[i] = self.points[i + 1];
            self.powers[i] = self.powers[i + 1];
        }
        self.check_line_is_linear();
    }

    /// `flipHorizontal()`.
    pub fn flip_horizontal(&mut self) {
        let n = self.num_points;
        for i in 0..n.div_ceil(2) {
            let tmp_x = 1.0 - self.points[i].0;
            let tmp_y = self.points[i].1;
            self.points[i].0 = 1.0 - self.points[n - i - 1].0;
            self.points[i].1 = self.points[n - i - 1].1;
            self.points[n - i - 1].0 = tmp_x;
            self.points[n - i - 1].1 = tmp_y;
        }
        for i in 0..n / 2 {
            let tmp_power = self.powers[i];
            self.powers[i] = -self.powers[n - i - 2];
            self.powers[n - i - 2] = -tmp_power;
        }
        self.render();
        self.check_line_is_linear();
    }

    /// `flipVertical()`.
    pub fn flip_vertical(&mut self) {
        for point in &mut self.points[..self.num_points] {
            point.1 = 1.0 - point.1;
        }
        self.render();
        self.check_line_is_linear();
    }

    pub fn last_point(&self) -> (f32, f32) {
        self.points[self.num_points - 1]
    }

    pub fn last_power(&self) -> f32 {
        self.powers[self.num_points - 1]
    }

    pub fn resolution(&self) -> usize {
        self.resolution
    }

    pub fn is_linear(&self) -> bool {
        self.linear
    }

    pub fn smooth(&self) -> bool {
        self.smooth
    }

    pub fn looped(&self) -> bool {
        self.looped
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn last_browsed_file(&self) -> &str {
        &self.last_browsed_file
    }

    /// `getBuffer()`: the `resolution` rendered values.
    pub fn buffer(&self) -> &[f32] {
        &self.buffer[1..=self.resolution]
    }

    /// `getCubicInterpolationBuffer()`: the full buffer with one value of
    /// padding before and two after.
    pub fn cubic_interpolation_buffer(&self) -> &[f32] {
        &self.buffer
    }

    pub fn point(&self, index: usize) -> (f32, f32) {
        self.points[index]
    }

    pub fn power(&self, index: usize) -> f32 {
        self.powers[index]
    }

    pub fn num_points(&self) -> usize {
        self.num_points
    }

    /// `setPoint(index, point)`. Does not re-render, like upstream.
    pub fn set_point(&mut self, index: usize, point: (f32, f32)) {
        self.points[index] = point;
        self.check_line_is_linear();
    }

    /// `setPower(index, power)`. Does not re-render, like upstream.
    pub fn set_power(&mut self, index: usize, power: f32) {
        self.powers[index] = power;
        self.check_line_is_linear();
    }

    /// `setNumPoints(num_points)`. Does not re-render, like upstream.
    pub fn set_num_points(&mut self, num_points: usize) {
        assert!(num_points <= Self::MAX_POINTS);
        self.num_points = num_points;
        self.check_line_is_linear();
    }

    pub fn render_count(&self) -> u32 {
        self.render_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_ramp_renders_upward() {
        // Upstream advances to the next segment only after writing a sample,
        // so the first sample past a point still uses the previous segment.
        let generator = LineGenerator::new(5);
        assert!(generator.is_linear());
        assert_eq!(generator.buffer(), &[0.0, 0.0, 0.5, 0.75, 1.0]);
        let cubic = generator.cubic_interpolation_buffer();
        assert_eq!(cubic[0], 0.0);
        assert_eq!(cubic[6], 1.0);
        assert_eq!(cubic[7], 1.0);
    }

    #[test]
    fn json_round_trip() {
        let mut generator = LineGenerator::default();
        generator.init_triangle();
        let saved = generator.to_json();
        assert_eq!(
            saved.dump(),
            r#"{"name":"Triangle","num_points":3,"points":[0.0,1.0,0.5,0.0,1.0,1.0],"powers":[0.0,0.0,0.0],"smooth":false}"#
        );

        let mut loaded = LineGenerator::default();
        loaded.from_json(&saved).unwrap();
        assert_eq!(loaded.to_json(), saved);
        assert_eq!(loaded.buffer(), generator.buffer());
        assert!(!loaded.is_linear());
    }

    #[test]
    fn json_defaults_and_truncation() {
        let data = json::parse(
            r#"{"num_points": 2, "points": [0, 1, 1, 0, 0.5, 0.5], "powers": [0, 0, 3]}"#,
        )
        .unwrap();
        assert!(LineGenerator::is_valid_json(&data));
        let mut generator = LineGenerator::default();
        generator.from_json(&data).unwrap();
        assert!(generator.is_linear());
        assert_eq!(generator.power(2), 3.0);
        assert_eq!(
            generator.to_json().dump(),
            r#"{"name":"","num_points":2,"points":[0.0,1.0,1.0,0.0],"powers":[0.0,0.0],"smooth":false}"#
        );
    }

    #[test]
    fn json_errors_like_upstream() {
        let mut generator = LineGenerator::default();
        let short_points =
            json::parse(r#"{"num_points": 2, "points": [0, 1], "powers": [0, 0]}"#).unwrap();
        assert!(generator.from_json(&short_points).is_err());
        let bad_type = json::parse(
            r#"{"num_points": 2, "points": [0, 1, 1, 0], "powers": [0, 0], "smooth": 1}"#,
        )
        .unwrap();
        assert!(generator.from_json(&bad_type).is_err());
        assert!(!LineGenerator::is_valid_json(
            &json::parse(r#"{"num_points": 2}"#).unwrap()
        ));
    }

    #[test]
    fn boolean_points_and_powers_load_like_upstream() {
        let data = json::parse(
            r#"{"num_points": 2, "points": [false, true, true, false], "powers": [false, true], "smooth": false}"#,
        )
        .unwrap();
        let mut generator = LineGenerator::default();
        generator.from_json(&data).unwrap();
        assert_eq!(generator.point(0), (0.0, 1.0));
        assert_eq!(generator.point(1), (1.0, 0.0));
        assert_eq!(generator.power(1), 1.0);
        assert_eq!(
            generator.to_json().dump(),
            r#"{"name":"","num_points":2,"points":[0.0,1.0,1.0,0.0],"powers":[0.0,1.0],"smooth":false}"#
        );
        let boolean_count =
            json::parse(r#"{"num_points": true, "points": [0, 1], "powers": [0]}"#).unwrap();
        generator.from_json(&boolean_count).unwrap();
        assert_eq!(generator.num_points(), 1);
    }

    #[test]
    fn empty_line_saves_nulls() {
        let mut generator = LineGenerator::default();
        generator.set_num_points(0);
        assert_eq!(
            generator.to_json().dump(),
            r#"{"name":"Linear","num_points":0,"points":null,"powers":null,"smooth":false}"#
        );
    }
}
