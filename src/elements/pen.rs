use crate::{AnimatedElement, Context, CreateInnerInfo, ValidState, custom::FnWrapper};
use derive_setters::Setters;
use glam::{Quat, Vec3};
use map_range::MapRange as _;
use stardust_xr_asteroids_derive::Animatable;
use stardust_xr_fusion::{
	Error, Result,
	drawable::{Line, LinePoint, Lines, LinesExt},
	fields::{Field, FieldExt, Shape},
	spatial::{Spatial, SpatialExt, Transform},
	suis::{Finger, Hand, InputDataType},
	types::{
		QuatF, Vec3F,
		color::{AlphaColor, Rgb, color_space::LinearRgb, rgba_linear},
	},
};
use stardust_xr_molecules::input_action::{InputQueue, SingleAction};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

#[derive(Debug, Clone, Copy, Default)]
pub enum PenState {
	#[default]
	Floating,
	Grabbed,
	StartedDrawing(f32),
	Drawing(f32),
	StoppedDrawing,
	/// radius of a sphere around the whole fist, none for tips and pointers
	StartedErasing(Option<f32>),
	Erasing(Option<f32>),
	StoppedErasing,
}

pub struct PenInner {
	content_root: Spatial,
	pen_visuals: Lines,
	// the field straddles the shaft, so it lives on a spatial offset to the shaft's midpoint
	field_root: Spatial,
	field: Field,
	pointer_distance: f32,
	input: InputQueue,
	grab_action: SingleAction,
	drawing: bool,
	erasing: bool,
}

#[derive(Setters, Animatable)]
#[derive_where::derive_where(Debug)]
#[setters(into, strip_option)]
pub struct Pen<State: ValidState> {
	pub length: f32,
	pub thickness: f32,
	pub grab_distance: f32,
	/// lower than the grab threshold so tracking jitter doesn't drop the pen
	pub hand_release_threshold: f32,
	pub hand_grab_threshold: f32,
	/// Index–thumb draw strength: 1 at 15 mm apart, 0 at 50 mm apart.
	pub hand_draw_threshold: f32,
	pub tip_draw_threshold: f32,
	pub hand_erase_threshold: f32,
	pub button_erase_threshold: f32,
	pub color: AlphaColor<f32, Rgb<f32, LinearRgb>>,
	pub pos: Vec3F,
	pub rot: QuatF,
	#[expect(clippy::type_complexity)]
	#[setters(skip)]
	pub update: FnWrapper<dyn Fn(&mut State, PenState, Vec3F, QuatF) + Send + Sync>,
}
impl<State: ValidState> Pen<State> {
	pub fn new(
		pos: impl Into<Vec3F>,
		rot: impl Into<QuatF>,
		update: impl Fn(&mut State, PenState, Vec3F, QuatF) + Send + Sync + 'static,
	) -> Self {
		Pen {
			length: 0.075,
			thickness: 0.0025,
			grab_distance: 0.05,
			hand_release_threshold: 0.4,
			hand_grab_threshold: 0.75,
			hand_draw_threshold: 0.75,
			tip_draw_threshold: 0.1,
			hand_erase_threshold: 0.75,
			button_erase_threshold: 0.5,
			color: rgba_linear!(1.0, 1.0, 1.0, 1.0),
			pos: pos.into(),
			rot: rot.into(),
			update: FnWrapper(Box::new(update)),
		}
	}

	fn get_lines(&self) -> Line {
		Line {
			points: vec![
				LinePoint {
					point: [0.0; 3].into(),
					thickness: 0.0,
					color: self.color,
				},
				LinePoint {
					point: [0.0, self.thickness, 0.0].into(),
					thickness: self.thickness,
					color: self.color,
				},
				LinePoint {
					point: [0.0, self.length, 0.0].into(),
					thickness: self.thickness,
					color: self.color,
				},
			],
			cyclic: false,
		}
	}
}
impl<State: ValidState> AnimatedElement<State> for Pen<State> {
	type Inner = PenInner;

	type Error = Error;

	async fn create_inner(&self, context: &Context, info: CreateInnerInfo) -> Result<Self::Inner> {
		let content_root = info.child_space;
		content_root
			.set_local_transform(Transform::from_translation_rotation(self.pos, self.rot))?;

		let pen_visuals = Lines::new(
			&context.stardust_client,
			&content_root,
			vec![self.get_lines()],
		)
		.await?;

		// the cylinder field is centered on its spatial, so offset it up to the shaft midpoint
		let content_root_ref = content_root.spatial_ref().await?;
		let (field_root, _field_root_ref) = Spatial::new(
			&context.stardust_client,
			&content_root_ref,
			Transform::from_translation([0.0, self.length * 0.5, 0.0]),
		)
		.await?;
		let (field, _field_ref) = Field::new(
			&context.stardust_client,
			&field_root,
			Shape::Cylinder {
				length: self.length,
				radius: self.thickness,
			},
		)
		.await?;

		let input = InputQueue::new(
			&context.stardust_client,
			field_root.clone(),
			field.clone(),
			// input is reported relative to the *stationary* parent space, not the moving
			// content_root, so the grab pose doesn't fight itself as the pen follows
			info.parent_space.clone(),
		)
		.await?;

		Ok(PenInner {
			content_root,
			pen_visuals,
			field_root,
			field,
			input,
			pointer_distance: 0.0,
			grab_action: Default::default(),
			drawing: false,
			erasing: false,
		})
	}

	fn diff(&self, old: &Self, _context: &Context, inner: &mut Self::Inner) {
		if self.pos != old.pos || self.rot != old.rot {
			let _ = inner
				.content_root
				.set_local_transform(Transform::from_translation_rotation(self.pos, self.rot));
		}

		if self.thickness != old.thickness || self.length != old.length || self.color != old.color {
			let _ = inner.pen_visuals.set_lines(vec![self.get_lines()]);
			let _ = inner.field.set_shape(Shape::Cylinder {
				length: self.length,
				radius: self.thickness,
			});
			let _ = inner
				.field_root
				.set_local_transform(Transform::from_translation([0.0, self.length * 0.5, 0.0]));
		}
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &stardust_xr_fusion::client::FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		if !inner.input.handle_events() {
			return;
		}

		let holder = inner.grab_action.actor().map(|a| a.method.clone());
		inner.grab_action.update(
			false,
			&inner.input,
			|i| i.distance() < self.grab_distance,
			|i| match i.input() {
				InputDataType::Hand { data: h } => {
					(h.finger_curl(&h.ring) + h.finger_curl(&h.little)) / 2.0
						> if holder.as_ref() == Some(&i.method) {
							self.hand_release_threshold
						} else {
							self.hand_grab_threshold
						}
				}
				_ => i.datamap_f32("grab") > 0.90,
			},
		);

		let Some(actor) = inner.grab_action.actor().cloned() else {
			for pen_state in stop_activity(&mut inner.drawing, &mut inner.erasing)
				.into_iter()
				.flatten()
			{
				(self.update.0)(state, pen_state, self.pos, self.rot);
			}
			if inner.grab_action.actor_stopped() {
				(self.update.0)(state, PenState::Floating, self.pos, self.rot);
			}
			return;
		};

		if let InputDataType::Pointer { data: p } = actor.input() {
			if inner.grab_action.actor_started() {
				// deepest_point is now a distance along the ray
				inner.pointer_distance = p.deepest_point;
			} else {
				inner.pointer_distance += (actor.datamap_vec2("scroll_continuous").y * 0.01) + // continuous +Y -> 1cm farther away
					(actor.datamap_vec2("scroll_discrete").y * 0.1); // discrete +Y -> 10cm farther away
			}
		}

		// the grab already needs ring and little curled, so only index and middle make a fist
		let erasing = match actor.input() {
			InputDataType::Hand { data: h } => {
				(h.finger_curl(&h.index) + h.finger_curl(&h.middle)) / 2.0
					> self.hand_erase_threshold
			}
			InputDataType::Tip { .. } => actor.datamap_f32("context") > self.button_erase_threshold,
			InputDataType::Pointer { .. } => {
				actor.datamap_f32("middle") > self.button_erase_threshold
			}
		};

		let mut radius = None;
		let (pos, rot) = match actor.input() {
			InputDataType::Hand { data: h } => {
				let rot = Quat::from(h.palm.pose.orientation);
				if erasing {
					let (center, r) = fist_sphere(h);
					radius = Some(r);
					(center, rot)
				} else {
					(Vec3::from(h.predicted_pinch_position()), rot)
				}
			}
			InputDataType::Tip { data: t } => (
				Vec3::from(t.pose.position),
				Quat::from(t.pose.orientation) * Quat::from_rotation_x(FRAC_PI_2),
			),
			InputDataType::Pointer { data: p } => {
				// Calculate position at current distance along pointer ray
				let origin = Vec3::from(p.pose.position);
				let orientation = Quat::from(p.pose.orientation);
				let direction = Vec3::from(p.direction()).normalize();
				(
					origin + (direction * inner.pointer_distance),
					orientation * Quat::from_rotation_z(-FRAC_PI_4),
				)
			}
		};

		let pen_state = if erasing {
			if inner.drawing {
				inner.drawing = false;
				PenState::StoppedDrawing
			} else if !inner.erasing {
				inner.erasing = true;
				PenState::StartedErasing(radius)
			} else {
				PenState::Erasing(radius)
			}
		} else if inner.erasing {
			inner.erasing = false;
			PenState::StoppedErasing
		} else {
			drawing_state(
				&mut inner.drawing,
				draw_pressure(
					actor.input(),
					actor.datamap_f32("select"),
					self.hand_draw_threshold,
					self.tip_draw_threshold,
				),
			)
		};

		(self.update.0)(state, pen_state, pos.into(), rot.into());
	}
}

// Stop events must precede Floating, and clearing both flags makes re-grabbing start fresh.
fn stop_activity(drawing: &mut bool, erasing: &mut bool) -> [Option<PenState>; 2] {
	[
		std::mem::take(drawing).then_some(PenState::StoppedDrawing),
		std::mem::take(erasing).then_some(PenState::StoppedErasing),
	]
}

fn drawing_state(drawing: &mut bool, pressure: Option<f32>) -> PenState {
	match pressure {
		Some(p) => {
			if std::mem::replace(drawing, true) {
				PenState::Drawing(p)
			} else {
				PenState::StartedDrawing(p)
			}
		}
		None if std::mem::take(drawing) => PenState::StoppedDrawing,
		None => PenState::Grabbed,
	}
}

// Only the grab actor is considered, never other inputs in the queue.
fn draw_pressure(
	input: &InputDataType,
	select: f32,
	hand_threshold: f32,
	tip_threshold: f32,
) -> Option<f32> {
	let (strength, threshold) = match input {
		InputDataType::Hand { data: h } => (index_thumb_pinch_strength(h), hand_threshold),
		_ => (select, tip_threshold),
	};
	(strength > threshold).then(|| strength.map_range(threshold..1.0, 0.0..1.0))
}

/// Index–thumb draw gesture, independent of the other fingers curled to hold the pen.
/// `Hand::pinch_strength` instead accepts the nearest fingertip to the thumb.
fn index_thumb_pinch_strength(h: &Hand) -> f32 {
	let thumb = Vec3::from(h.thumb.tip.pose.position);
	let index = Vec3::from(h.index.tip.pose.position);
	let d = thumb.distance(index);
	// Absolute metres (adult hand): 5cm → 0 (open), 1.5cm → 1 (pinched)
	const D_OPEN: f32 = 0.050;
	const D_PINCH: f32 = 0.015;
	((D_OPEN - d) / (D_OPEN - D_PINCH)).clamp(0.0, 1.0)
}

// bounding box center rather than centroid, the fingers would drag a centroid off center
fn fist_sphere(h: &Hand) -> (Vec3, f32) {
	let f = |f: &Finger| [f.tip, f.distal, f.intermediate, f.proximal, f.metacarpal];
	let joints = [
		h.thumb.tip,
		h.thumb.distal,
		h.thumb.proximal,
		h.thumb.metacarpal,
		h.palm,
		h.wrist,
	]
	.into_iter()
	.chain(
		[&h.index, &h.middle, &h.ring, &h.little]
			.into_iter()
			.flat_map(f),
	)
	.map(|j| (Vec3::from(j.pose.position), j.radius));
	let (min, max) = joints.clone().fold(
		(Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
		|(min, max), (p, r)| (min.min(p - r), max.max(p + r)),
	);
	let c = (min + max) / 2.0;
	(
		c,
		joints.map(|(p, r)| p.distance(c) + r).fold(0.0, f32::max),
	)
}

#[cfg(test)]
mod pinch_tests {
	use super::*;
	use stardust_xr_fusion::{
		suis::{Chirality, Joint, Thumb},
		types::Posef,
	};

	fn joint(x: f32) -> Joint {
		Joint {
			pose: Posef {
				position: [x, 0.0, 0.0].into(),
				orientation: Quat::IDENTITY.into(),
			},
			radius: 0.005,
			distance: 0.0,
		}
	}

	fn finger(tip_x: f32) -> Finger {
		Finger {
			tip: joint(tip_x),
			distal: joint(0.01),
			intermediate: joint(0.0),
			proximal: joint(0.0),
			metacarpal: joint(-0.06),
		}
	}

	fn hand(index_distance: f32) -> Hand {
		Hand {
			chirality: Chirality::Right,
			thumb: Thumb {
				tip: joint(0.0),
				distal: joint(0.0),
				proximal: joint(0.0),
				metacarpal: joint(-0.06),
			},
			index: finger(index_distance),
			middle: finger(0.005),
			ring: finger(0.005),
			little: finger(0.005),
			palm: joint(0.0),
			wrist: joint(0.0),
			elbow: None,
		}
	}

	#[test]
	fn open_index_releases_even_with_curled_grab_fingers() {
		let h = hand(0.04);
		assert_eq!(
			h.pinch_strength(),
			1.0,
			"old any-finger signal stays pinched"
		);
		assert!(index_thumb_pinch_strength(&h) < 0.3);
		let actor = InputDataType::Hand { data: h };
		assert!(draw_pressure(&actor, 1.0, 0.6928417, 0.1).is_none());
	}

	#[test]
	fn index_thumb_distance_mapping_is_bounded() {
		for (distance, expected) in [
			(0.0, 1.0),
			(0.015, 1.0),
			(0.0325, 0.5),
			(0.05, 0.0),
			(0.1, 0.0),
		] {
			assert!((index_thumb_pinch_strength(&hand(distance)) - expected).abs() < 1e-6);
		}
	}

	#[test]
	fn another_pinched_hand_cannot_keep_the_grab_actor_drawing() {
		let actor = InputDataType::Hand { data: hand(0.04) };
		let other = InputDataType::Hand { data: hand(0.015) };
		assert!(draw_pressure(&other, 0.0, 0.7, 0.1).is_some());
		assert!(draw_pressure(&actor, 0.0, 0.7, 0.1).is_none());
	}

	#[test]
	fn drawing_stops_once_then_regrab_starts_fresh() {
		let (mut drawing, mut erasing) = (false, false);
		assert!(matches!(
			drawing_state(&mut drawing, Some(1.0)),
			PenState::StartedDrawing(_)
		));
		assert!(matches!(
			drawing_state(&mut drawing, Some(1.0)),
			PenState::Drawing(_)
		));
		let [stop, erase] = stop_activity(&mut drawing, &mut erasing);
		assert!(matches!(stop, Some(PenState::StoppedDrawing)));
		assert!(erase.is_none());
		assert!(!drawing);
		assert!(
			stop_activity(&mut drawing, &mut erasing)
				.iter()
				.all(Option::is_none)
		);
		assert!(matches!(
			drawing_state(&mut drawing, Some(1.0)),
			PenState::StartedDrawing(_)
		));
	}

	#[test]
	fn unpinch_stops_then_returns_to_grabbed() {
		let mut drawing = true;
		assert!(matches!(
			drawing_state(&mut drawing, None),
			PenState::StoppedDrawing
		));
		assert!(matches!(
			drawing_state(&mut drawing, None),
			PenState::Grabbed
		));
	}

	#[test]
	fn tracking_loss_also_clears_erase_activity() {
		let (mut drawing, mut erasing) = (false, true);
		let [stop, erase] = stop_activity(&mut drawing, &mut erasing);
		assert!(stop.is_none());
		assert!(matches!(erase, Some(PenState::StoppedErasing)));
		assert!(!erasing);
	}
}

#[tokio::test]
async fn asteroids_pen_test() {
	use crate::{
		Tasker,
		client::{self, ClientState},
		custom::CustomElement,
		elements::{Axes, Lines, Pen, line_from_points},
	};
	use serde::{Deserialize, Serialize};
	use stardust_xr_molecules::lines::LineExt;

	#[derive(Serialize, Deserialize)]
	struct TestState {
		#[serde(skip)]
		pen_state: PenState,
		pos: Vec3F,
		rot: QuatF,
	}
	impl Default for TestState {
		fn default() -> Self {
			Self {
				pen_state: PenState::Floating,
				pos: [0.1; 3].into(),
				rot: Quat::from_rotation_z(FRAC_PI_4).into(),
			}
		}
	}

	impl crate::util::Migrate for TestState {
		type Old = Self;
	}

	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.pen";
	}
	impl crate::Reify for TestState {
		fn reify(
			&self,
			_context: &Context,
			_tasks: impl Tasker<Self>,
			_props: (),
		) -> impl crate::Element<Self> {
			Axes::default()
				.build()
				.child(
					Lines::new([
						line_from_points(vec![[0.0; 3].into(), self.pos]).thickness(0.0025)
					])
					.build(),
				)
				.child(
					Pen::new(
						self.pos,
						self.rot,
						|state: &mut TestState, pen_state, pos, rot| {
							state.pen_state = dbg!(pen_state);
							state.pos = pos;
							state.rot = rot;
						},
					)
					.color(match &self.pen_state {
						PenState::Floating => {
							rgba_linear!(1.0, 1.0, 1.0, 1.0)
						}
						PenState::Grabbed => {
							rgba_linear!(0.1, 0.1, 1.0, 1.0)
						}
						PenState::StartedDrawing(p) => {
							rgba_linear!(0.1 * p, 1.0 * p, 0.1 * p, 1.0)
						}
						PenState::Drawing(p) => {
							rgba_linear!(1.0 * p, 1.0 * p, 0.1 * p, 1.0)
						}
						PenState::StoppedDrawing => {
							rgba_linear!(1.0, 0.1, 0.1, 1.0)
						}
						PenState::StartedErasing(_) | PenState::Erasing(_) => {
							rgba_linear!(1.0, 0.1, 1.0, 1.0)
						}
						PenState::StoppedErasing => {
							rgba_linear!(0.5, 0.0, 0.5, 1.0)
						}
					})
					.length(0.1)
					.thickness(0.01)
					.build(),
				)
		}
	}

	client::run::<TestState>(&[]).await.unwrap();
}
