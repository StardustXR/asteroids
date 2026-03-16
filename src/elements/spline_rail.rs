use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, FnWrapper, Transformable},
};
use derive_setters::Setters;
use glam::Vec3;
use stardust_xr_fusion::{
	drawable::{Line, Lines, LinesAspect},
	fields::{CubicSplineShape, Field, FieldAspect, Shape},
	input::{InputDataType, InputHandler},
	node::NodeError,
	spatial::{Spatial, SpatialRef, Transform},
	values::{Color, color::rgba_linear},
};
use stardust_xr_molecules::{
	input_action::{InputQueue, InputQueueable, SingleAction},
	lines::{LineExt, line_from_points},
};

pub type OnSlideFn<State> = FnWrapper<dyn Fn(&mut State, f32, f32) + Send + Sync>;

#[derive_where::derive_where(Debug, PartialEq)]
#[derive(Setters)]
#[setters(into, strip_option)]
pub struct SplineRail<State: ValidState> {
	transform: Transform,
	#[setters(skip)]
	spline: CubicSplineShape,
	/// Extra distance beyond spline tube surface where interaction is detected
	interact_margin: f32,
	#[setters(skip)]
	on_slide: OnSlideFn<State>,
}
impl<State: ValidState> SplineRail<State> {
	pub fn new(
		spline: impl Into<CubicSplineShape>,
		on_slide: impl Fn(&mut State, f32, f32) + Send + Sync + 'static,
	) -> Self {
		SplineRail {
			transform: Transform::none(),
			spline: spline.into(),
			interact_margin: 0.02,
			on_slide: FnWrapper(Box::new(on_slide)),
		}
	}
}

impl<State: ValidState> Transformable for SplineRail<State> {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}

impl<State: ValidState> CustomElement<State> for SplineRail<State> {
	type Inner = SplineRailInner;
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		context: &Context,
		info: CreateInnerInfo,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		SplineRailInner::create(
			info.parent_space,
			*self.transform(),
			&self.spline,
			context.accent_color.color(),
		)
	}

	fn diff(&self, old: &Self, inner: &mut Self::Inner, _resource: &mut Self::Resource) {
		self.apply_transform(old, &inner.root);
		if self.spline != old.spline {
			let _ = inner.field.set_shape(Shape::Spline(self.spline.clone()));
			inner.resample(&self.spline);
		}
	}

	fn frame(
		&self,
		context: &Context,
		_info: &stardust_xr_fusion::root::FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		inner.update(self, state, context.accent_color.color());
	}

	fn spatial_aspect(&self, inner: &Self::Inner) -> SpatialRef {
		inner.input.handler().clone().as_spatial().as_spatial_ref()
	}
}

const SEGMENT_COUNT: usize = 16;
const MAX_SEARCH_WINDOW: f32 = 0.1;

pub struct SplineRailInner {
	root: Spatial,
	field: Field,
	input: InputQueue,
	graphics: Lines,
	single_action: SingleAction,
	last_t: Option<f32>,
	sampled_points: Vec<Vec3>,
	cumulative_lengths: Vec<f32>,
	total_length: f32,
	cyclic: bool,
	base_line: Line,
}

impl SplineRailInner {
	fn create(
		parent: &SpatialRef,
		transform: Transform,
		spline: &CubicSplineShape,
		accent_color: Color,
	) -> Result<Self, NodeError> {
		let root = Spatial::create(parent, transform)?;
		let spline_shape = spline.clone();
		let field = Field::create(
			&root,
			Transform::identity(),
			Shape::Spline(spline_shape.clone()),
		)?;
		let input = InputHandler::create(&root, Transform::identity(), &field)?.queue()?;

		let base_line = spline_shape.to_lines(SEGMENT_COUNT).thickness(0.001);
		let graphics = Lines::create(
			&root,
			Transform::identity(),
			&[base_line.clone().color(accent_color)],
		)?;

		let (sampled_points, cumulative_lengths, total_length) = sample_spline(&spline_shape);

		Ok(Self {
			root,
			field,
			input,
			graphics,
			single_action: SingleAction::default(),
			last_t: None,
			sampled_points,
			cumulative_lengths,
			total_length,
			cyclic: spline_shape.cyclic,
			base_line,
		})
	}

	fn resample(&mut self, spline_shape: &CubicSplineShape) {
		let (points, lengths, total) = sample_spline(spline_shape);
		self.sampled_points = points;
		self.cumulative_lengths = lengths;
		self.total_length = total;
		self.cyclic = spline_shape.cyclic;
		self.base_line = spline_shape.to_lines(SEGMENT_COUNT).thickness(0.001);
	}

	fn update<State: ValidState>(
		&mut self,
		decl: &SplineRail<State>,
		state: &mut State,
		accent_color: Color,
	) {
		if !self.input.handle_events() {
			return;
		}
		self.single_action.update(
			false,
			&self.input,
			|data| data.distance < decl.interact_margin,
			|data| match &data.input {
				InputDataType::Hand(h) => h.pinch_strength() > 0.5,
				_ => data.datamap.with_data(|d| d.idx("select").as_f32() > 0.5),
			},
		);

		if self.single_action.actor_stopped() {
			self.last_t.take();
		}

		let Some(actor) = self.single_action.actor() else {
			let _ = self
				.graphics
				.set_lines(&self.signifier_lines(accent_color, None));
			return;
		};

		let interact_point: Vec3 = match &actor.input {
			InputDataType::Hand(h) => Vec3::from(h.predicted_pinch_position()),
			InputDataType::Tip(t) => Vec3::from(t.origin),
			InputDataType::Pointer(p) => {
				let origin = Vec3::from(p.origin);
				let direction = Vec3::from(p.direction()).normalize();
				let deepest = Vec3::from(p.deepest_point);
				let dist = origin.distance(deepest);
				origin + direction * dist
			}
		};

		let new_t = if let Some(last_t) = self.last_t {
			closest_t_windowed(
				&self.sampled_points,
				interact_point,
				last_t,
				MAX_SEARCH_WINDOW,
				self.cyclic,
			)
		} else {
			closest_t_global(&self.sampled_points, interact_point)
		};

		if let Some(last_t) = self.last_t {
			let mut delta_t = new_t - last_t;
			if self.cyclic {
				// Wrap delta to [-0.5, 0.5] so crossing t=0/1 boundary
				// produces a small step, not a massive jump
				if delta_t > 0.5 {
					delta_t -= 1.0;
				} else if delta_t < -0.5 {
					delta_t += 1.0;
				}
			}
			let mut delta_distance =
				t_to_distance(&self.cumulative_lengths, self.total_length, new_t)
					- t_to_distance(&self.cumulative_lengths, self.total_length, last_t);
			if self.cyclic {
				let half_length = self.total_length * 0.5;
				if delta_distance > half_length {
					delta_distance -= self.total_length;
				} else if delta_distance < -half_length {
					delta_distance += self.total_length;
				}
			}
			(decl.on_slide.0)(state, delta_t, delta_distance);
		}

		self.last_t = Some(new_t);

		// Closest point on spline for the indicator line
		let closest_on_spline = t_to_point(&self.sampled_points, new_t);
		let _ = self.graphics.set_lines(
			&self.signifier_lines(accent_color, Some((interact_point, closest_on_spline))),
		);
	}

	fn signifier_lines(
		&self,
		accent_color: Color,
		active_points: Option<(Vec3, Vec3)>,
	) -> Vec<Line> {
		let color = if active_points.is_some() {
			accent_color
		} else {
			rgba_linear!(1.0, 1.0, 1.0, 0.5)
		};

		let mut lines = vec![
			self.base_line.clone().color(color).shimmer(
				&self
					.single_action
					.hovering()
					.current()
					.iter()
					.flat_map(|i| match &i.input {
						InputDataType::Pointer(pointer) => vec![pointer.deepest_point],
						InputDataType::Hand(hand) => {
							vec![
								hand.index.tip.position,
								hand.thumb.tip.position,
								hand.stable_pinch_position(),
							]
						}
						InputDataType::Tip(tip) => vec![tip.origin],
					})
					.collect::<Vec<_>>(),
				0.025,
				0.0,
				accent_color,
				1.25,
			),
		];

		if let Some((interact_pt, closest_pt)) = active_points {
			lines.push(
				line_from_points(vec![
					[interact_pt.x, interact_pt.y, interact_pt.z],
					[closest_pt.x, closest_pt.y, closest_pt.z],
				])
				.thickness(0.001),
			);
		}

		lines
	}
}

/// Sample the spline into a polyline and compute cumulative arc lengths.
fn sample_spline(shape: &CubicSplineShape) -> (Vec<Vec3>, Vec<f32>, f32) {
	let line = shape.to_lines(SEGMENT_COUNT);
	let points: Vec<Vec3> = line
		.points
		.iter()
		.map(|p| Vec3::new(p.point.x, p.point.y, p.point.z))
		.collect();

	let mut cumulative = Vec::with_capacity(points.len());
	cumulative.push(0.0);
	for i in 1..points.len() {
		let dist = points[i].distance(points[i - 1]);
		cumulative.push(cumulative[i - 1] + dist);
	}
	let total = *cumulative.last().unwrap_or(&0.0);
	(points, cumulative, total)
}

/// Find the closest t value [0, 1] on the sampled polyline (global search).
fn closest_t_global(sampled: &[Vec3], point: Vec3) -> f32 {
	if sampled.is_empty() {
		return 0.0;
	}
	let n = sampled.len();
	let mut best_t = 0.0;
	let mut best_dist_sq = f32::INFINITY;

	for i in 0..n.saturating_sub(1) {
		let (t, dist_sq) = closest_on_segment(sampled[i], sampled[i + 1], point);
		let global_t = (i as f32 + t) / (n - 1).max(1) as f32;
		if dist_sq < best_dist_sq {
			best_dist_sq = dist_sq;
			best_t = global_t;
		}
	}

	best_t
}

/// Find closest t within a window around last_t (for track continuity).
fn closest_t_windowed(
	sampled: &[Vec3],
	point: Vec3,
	last_t: f32,
	window: f32,
	cyclic: bool,
) -> f32 {
	if sampled.len() < 2 {
		return 0.0;
	}
	let n = sampled.len();
	let n_segments = n - 1;

	let t_min = last_t - window;
	let t_max = last_t + window;

	let mut best_t = last_t;
	let mut best_dist_sq = f32::INFINITY;

	for i in 0..n_segments {
		let seg_t_start = i as f32 / n_segments as f32;
		let seg_t_end = (i + 1) as f32 / n_segments as f32;

		let in_window = if cyclic {
			// For cyclic, check wrapped ranges
			segment_overlaps_wrapped(seg_t_start, seg_t_end, t_min, t_max)
		} else {
			seg_t_end >= t_min && seg_t_start <= t_max
		};

		if !in_window {
			continue;
		}

		let (local_t, dist_sq) = closest_on_segment(sampled[i], sampled[i + 1], point);
		let global_t = (i as f32 + local_t) / n_segments as f32;
		if dist_sq < best_dist_sq {
			best_dist_sq = dist_sq;
			best_t = global_t;
		}
	}

	best_t
}

/// Check if a segment [s0, s1] overlaps a wrapped window [t_min, t_max] in [0, 1].
fn segment_overlaps_wrapped(s0: f32, s1: f32, t_min: f32, t_max: f32) -> bool {
	// Normal range check
	if s1 >= t_min && s0 <= t_max {
		return true;
	}
	// Wrapped: if t_min < 0, also check [t_min+1, 1]
	if t_min < 0.0 && s1 >= t_min + 1.0 {
		return true;
	}
	// Wrapped: if t_max > 1, also check [0, t_max-1]
	if t_max > 1.0 && s0 <= t_max - 1.0 {
		return true;
	}
	false
}

/// Find the closest point on line segment (a, b) to point p.
/// Returns (t in [0,1] along segment, squared distance).
fn closest_on_segment(a: Vec3, b: Vec3, p: Vec3) -> (f32, f32) {
	let ab = b - a;
	let len_sq = ab.length_squared();
	if len_sq < f32::EPSILON {
		return (0.0, a.distance_squared(p));
	}
	let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
	let closest = a + ab * t;
	(t, closest.distance_squared(p))
}

/// Convert a t value [0, 1] to a distance along the spline.
fn t_to_distance(cumulative: &[f32], _total: f32, t: f32) -> f32 {
	if cumulative.is_empty() {
		return 0.0;
	}
	let n = cumulative.len();
	let idx_f = t * (n - 1) as f32;
	let idx = (idx_f as usize).min(n - 2);
	let frac = idx_f - idx as f32;
	cumulative[idx] + frac * (cumulative[idx + 1] - cumulative[idx])
}

/// Convert a t value [0, 1] to a 3D point on the sampled polyline.
fn t_to_point(sampled: &[Vec3], t: f32) -> Vec3 {
	if sampled.is_empty() {
		return Vec3::ZERO;
	}
	let n = sampled.len();
	let idx_f = t * (n - 1) as f32;
	let idx = (idx_f as usize).min(n.saturating_sub(2));
	let frac = idx_f - idx as f32;
	sampled[idx].lerp(sampled[(idx + 1).min(n - 1)], frac)
}

/// Convert a distance along the spline to a t value [0, 1].
fn distance_to_t(cumulative: &[f32], total: f32, distance: f32) -> f32 {
	if cumulative.len() < 2 || total <= 0.0 {
		return 0.0;
	}
	let d = distance.clamp(0.0, total);
	// Binary search for the segment containing this distance
	let idx = cumulative
		.partition_point(|&len| len < d)
		.saturating_sub(1)
		.min(cumulative.len() - 2);
	let seg_len = cumulative[idx + 1] - cumulative[idx];
	let frac = if seg_len > f32::EPSILON {
		(d - cumulative[idx]) / seg_len
	} else {
		0.0
	};
	(idx as f32 + frac) / (cumulative.len() - 1) as f32
}

/// Get the tangent direction at a point on the sampled polyline.
fn t_to_tangent(sampled: &[Vec3], t: f32) -> Vec3 {
	if sampled.len() < 2 {
		return Vec3::X;
	}
	let n = sampled.len();
	let idx_f = t * (n - 1) as f32;
	let idx = (idx_f as usize).min(n.saturating_sub(2));
	(sampled[(idx + 1).min(n - 1)] - sampled[idx]).normalize_or(Vec3::X)
}

/// Generate `count` evenly-spaced rider circles along a spline, offset by a
/// distance (in meters) that wraps around. Works for any spline shape.
pub fn spline_rider_circles(
	spline: &CubicSplineShape,
	count: usize,
	distance_offset: f32,
	circle_radius: f32,
) -> Vec<Line> {
	use glam::{Mat4, Quat};
	use stardust_xr_molecules::lines::circle;

	let (sampled, cumulative, total) = sample_spline(spline);
	if total <= 0.0 || sampled.len() < 2 {
		return Vec::new();
	}

	(0..count)
		.map(|i| {
			// Evenly space circles by arc length, offset by distance_offset, wrap
			let base_dist = i as f32 / count as f32 * total;
			let dist = (base_dist + distance_offset).rem_euclid(total);
			let t = distance_to_t(&cumulative, total, dist);
			let pos = t_to_point(&sampled, t);
			let tang = t_to_tangent(&sampled, t);
			let rot = Quat::from_rotation_arc(Vec3::Y, tang);
			circle(12, 0.0, circle_radius)
				.thickness(0.001)
				.transform(Mat4::from_rotation_translation(rot, pos))
		})
		.collect()
}

#[tokio::test]
async fn asteroids_spline_rail_element() {
	use crate::{
		Tasker,
		client::{self, ClientState},
		elements::{SplineRail, Text},
	};
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::fields::CubicControlPoint;

	#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
	struct TestState {
		total_displacement_distance: f32,
	}
	impl crate::util::Migrate for TestState {
		type Old = Self;
	}
	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.spline_rail";
	}
	impl crate::Reify for TestState {
		fn reify(
			&self,
			_context: &Context,
			_tasks: impl Tasker<Self>,
		) -> impl crate::Element<Self> {
			// Lemniscate (figure-8): x(t) = r·sin(t), y(t) = r·sin(2t)/2
			// 8 control points at t = k·π/4, handles from tangent scaled by Δt/3
			let r = 0.05_f32;
			let s = std::f32::consts::FRAC_PI_4 / 3.0;

			let fig8_spline = CubicSplineShape {
				control_points: (0..8)
					.map(|k| {
						let t = k as f32 * std::f32::consts::FRAC_PI_4;
						let ax = r * t.sin();
						let ay = r * (2.0 * t).sin() / 2.0;
						let tx = r * t.cos() * s;
						let ty = r * (2.0 * t).cos() * s;
						CubicControlPoint {
							handle_in: [ax - tx, ay - ty, 0.0].into(),
							anchor: [ax, ay, 0.0].into(),
							handle_out: [ax + tx, ay + ty, 0.0].into(),
							thickness: 0.005,
						}
					})
					.collect(),
				cyclic: true,
			};

			// Coil: helix with 4 turns along x-axis, radius sized to contain the figure-8
			// Figure-8 spans x ∈ [-r, r] = [-0.05, 0.05], y ∈ [-r/2, r/2]
			// Coil radius = 0.04 (contains y range), extends beyond in x
			let coil_r = 0.04_f32;
			let turns = 4.0_f32;
			let coil_len = 0.14_f32; // total x-span, centered on origin
			let coil_points_per_turn = 3;
			let coil_segments = (turns * coil_points_per_turn as f32) as usize;
			let coil_spline = CubicSplineShape {
				control_points: (0..=coil_segments)
					.map(|k| {
						let frac = k as f32 / coil_segments as f32;
						let theta = frac * turns * std::f32::consts::TAU;
						let ax = frac * coil_len - coil_len / 2.0;
						let ay = coil_r * theta.cos();
						let az = coil_r * theta.sin();
						let dx = coil_len / coil_segments as f32;
						let dy = -coil_r * theta.sin() * turns * std::f32::consts::TAU
							/ coil_segments as f32;
						let dz = coil_r * theta.cos() * turns * std::f32::consts::TAU
							/ coil_segments as f32;
						let scale = 1.0 / 3.0;
						CubicControlPoint {
							handle_in: [ax - dx * scale, ay - dy * scale, az - dz * scale].into(),
							anchor: [ax, ay, az].into(),
							handle_out: [ax + dx * scale, ay + dy * scale, az + dz * scale].into(),
							thickness: 0.003,
						}
					})
					.collect(),
				cyclic: false,
			};

			let on_slide = |state: &mut TestState, _delta_t: f32, delta_distance: f32| {
				state.total_displacement_distance += delta_distance;
			};

			crate::elements::Spatial::default()
				.build()
				.child(
					SplineRail::new(fig8_spline.clone(), on_slide)
						.pos([0.0, 0.1, 0.0])
						.build()
						.child(
							crate::elements::Lines::new(spline_rider_circles(
								&fig8_spline,
								8,
								self.total_displacement_distance,
								0.001,
							))
							.build(),
						),
				)
				.child(
					SplineRail::new(coil_spline.clone(), on_slide)
						.build()
						.child(
							crate::elements::Lines::new(spline_rider_circles(
								&coil_spline,
								8,
								self.total_displacement_distance,
								0.001,
							))
							.build(),
						),
				)
				.child(
					Text::new(format!("Δ{:.3}m", self.total_displacement_distance))
						.character_height(0.005)
						.pos([0.0, 0.06, 0.0])
						.build(),
				)
		}
	}
	client::run::<TestState>(&[]).await
}
