use std::time::{Duration, Instant};

use stardust_xr_fusion::root::FrameInfo;

pub struct FrameWarning {
	actual_time: Instant,
	real_delta: Duration,
	delta: Option<f32>,
}
impl Default for FrameWarning {
	fn default() -> Self {
		Self {
			actual_time: Instant::now(),
			real_delta: Duration::ZERO,
			delta: None,
		}
	}
}

impl FrameWarning {
	pub fn update(&mut self, info: &FrameInfo) {
		self.real_delta = self.actual_time.elapsed();
		self.actual_time = Instant::now();
		self.delta.replace(info.delta);
	}
	pub fn danger(&self) -> bool {
		let Some(delta) = self.delta else {
			return false;
		};
		let delta = delta as f64;
		let real_delta = self.real_delta.as_millis() as f64 / 1000.0;
		delta < real_delta
	}
	pub fn times(&self) -> (f64, f64) {
		let delta = self.delta.unwrap_or(0.0) as f64;
		let real_delta = self.real_delta.as_millis() as f64 / 1000.0;
		(delta, real_delta)
	}
}
