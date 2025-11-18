use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, FnWrapper},
};
use derive_setters::Setters;
use glam::{Quat, Vec3};
use map_range::MapRange as _;
use mint::{Quaternion, Vector3};
use stardust_xr_fusion::{
	drawable::{Line, LinePoint, Lines, LinesAspect},
	fields::{CylinderShape, Field, FieldAspect, Shape},
	input::{InputData, InputDataType, InputHandler},
	node::{NodeError, NodeResult},
	spatial::{Spatial, SpatialAspect, SpatialRef, Transform},
	values::color::{AlphaColor, Rgb, color_space::LinearRgb, rgba_linear},
};
use stardust_xr_molecules::input_action::{
	InputQueue, InputQueueable as _, SimpleAction, SingleAction,
};
use std::f32::consts::FRAC_PI_2;

#[derive(Debug)]
pub enum PenState {
	Grabbed,
	StartedDrawing(f32),
	Drawing(f32),
	StoppedDrawing,
}

#[derive_where::derive_where(Debug)]
#[derive(Setters)]
#[setters(into, strip_option)]
pub struct Pen<State: ValidState> {
	pub length: f32,
	pub thickness: f32,
	pub grab_distance: f32,
	pub hand_draw_threshold: f32,
	pub tip_draw_threshold: f32,
	pub color: AlphaColor<f32, Rgb<f32, LinearRgb>>,
	pub pos: Vector3<f32>,
	pub rot: Quaternion<f32>,
	#[expect(clippy::type_complexity)]
	#[setters(skip)]
	pub update:
		FnWrapper<dyn Fn(&mut State, PenState, Vector3<f32>, Quaternion<f32>) + Send + Sync>,
}
impl<State: ValidState> Pen<State> {
	pub fn new(
		pos: impl Into<Vector3<f32>>,
		rot: impl Into<Quaternion<f32>>,
		update: impl Fn(&mut State, PenState, Vector3<f32>, Quaternion<f32>) + Send + Sync + 'static,
	) -> Self {
		Pen {
			length: 0.075,
			thickness: 0.0025,
			grab_distance: 0.05,
			hand_draw_threshold: 0.75,
			tip_draw_threshold: 0.1,
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
impl<State: ValidState> CustomElement<State> for Pen<State> {
	type Inner = PenInner;
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		_asteroids_context: &Context,
		info: CreateInnerInfo,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		PenInner::create(info.parent_space, self)
	}

	fn diff(&self, old: &Self, inner: &mut Self::Inner, _resource: &mut Self::Resource) {
		if self.thickness != old.thickness || self.length != old.length {
			_ = inner.visuals.set_lines(&[self.get_lines()]);
			_ = inner.field.set_shape(Shape::Cylinder(CylinderShape {
				length: self.length,
				radius: self.thickness,
			}));
		}
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &stardust_xr_fusion::root::FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		if let Some((pen_state, pos, rot)) = inner.handle_events(
			self.grab_distance,
			self.hand_draw_threshold,
			self.tip_draw_threshold,
		) {
			(self.update.0)(state, pen_state, pos.into(), rot.into());
		}
	}

	fn spatial_aspect(&self, inner: &Self::Inner) -> SpatialRef {
		inner.child_root.clone().as_spatial_ref()
	}
}

pub struct PenInner {
	child_root: Spatial,
	field: Field,
	pen_root: Spatial,
	input: InputQueue,
	grab_action: SingleAction,
	draw_action: SimpleAction,
	visuals: Lines,
	drawing: bool,
}
impl PenInner {
	fn create<State: ValidState>(parent_space: &SpatialRef, decl: &Pen<State>) -> NodeResult<Self> {
		let pen_root = Spatial::create(parent_space, Transform::none(), true)?;
		let field = Field::create(
			&pen_root,
			Transform::from_translation([0.0, 0.0, decl.length * 0.5]),
			Shape::Cylinder(CylinderShape {
				length: decl.length,
				radius: decl.thickness,
			}),
		)?;
		let queue = InputHandler::create(parent_space, Transform::none(), &field)?.queue()?;
		let visuals = Lines::create(&pen_root, Transform::none(), &[decl.get_lines()])?;

		let child_root = Spatial::create(
			&pen_root,
			Transform::from_translation(Vec3::new(0., decl.length, 0.)),
			false,
		)?;

		Ok(PenInner {
			field,
			pen_root,
			input: queue,
			grab_action: Default::default(),
			draw_action: Default::default(),
			visuals,
			child_root,
			drawing: false,
		})
	}

	fn handle_events(
		&mut self,
		grab_distance: f32,
		hand_draw_threshold: f32,
		tip_draw_threshold: f32,
	) -> Option<(PenState, Vec3, Quat)> {
		if !self.input.handle_events() {
			return None;
		}

		self.grab_action.update(
			false,
			&self.input,
			|data| data.distance < grab_distance,
			|data| match &data.input {
				InputDataType::Hand(h) => {
					(h.finger_curl(&h.ring) + h.finger_curl(&h.little)) / 2.0 > 0.75
				}
				InputDataType::Tip(_) => data
					.datamap
					.with_data(|datamap| datamap.idx("grab").as_f32() > 0.90),
				_ => false,
			},
		);

		self.draw_action
			.update(&self.input, &|data| match &data.input {
				InputDataType::Hand(h) => h.pinch_strength() > hand_draw_threshold,
				InputDataType::Tip(_) => data
					.datamap
					.with_data(|datamap| datamap.idx("select").as_f32() > tip_draw_threshold),
				_ => false,
			});

		let Some(actor) = self.grab_action.actor() else {
			if self.drawing {
				self.drawing = false;
				return Some((PenState::StoppedDrawing, Vec3::ZERO, Quat::IDENTITY));
			}
			return None;
		};

		let (pos, rot) = match &actor.input {
			InputDataType::Hand(h) => (
				h.predicted_pinch_position().into(),
				Quat::from(h.palm.rotation),
			),
			InputDataType::Tip(t) => (
				t.origin.into(),
				Quat::from(t.orientation) * Quat::from_rotation_x(FRAC_PI_2),
			),
			_ => (Vec3::ZERO, Quat::IDENTITY),
		};

		let transform = Transform::from_translation_rotation(pos, rot);
		let _ = self
			.pen_root
			.set_relative_transform(self.input.handler(), transform);

		let pen_state = if !self.draw_action.currently_acting().is_empty() {
			if !self.drawing {
				self.drawing = true;
				PenState::StartedDrawing(self.get_pressure(
					actor,
					hand_draw_threshold,
					tip_draw_threshold,
				))
			} else {
				PenState::Drawing(self.get_pressure(actor, hand_draw_threshold, tip_draw_threshold))
			}
		} else if self.drawing {
			self.drawing = false;
			PenState::StoppedDrawing
		} else {
			PenState::Grabbed
		};

		Some((pen_state, pos, rot))
	}

	fn get_pressure(
		&self,
		data: &InputData,
		hand_draw_threshold: f32,
		tip_draw_threshold: f32,
	) -> f32 {
		data.datamap.with_data(|datamap| match &data.input {
			InputDataType::Hand(h) => h
				.pinch_strength()
				.map_range(hand_draw_threshold..1.0, 0.0..1.0),
			InputDataType::Tip(_) => datamap
				.idx("select")
				.as_f32()
				.map_range(tip_draw_threshold..1.0, 0.0..1.0),
			_ => 0.0,
		})
	}
}

#[tokio::test]
async fn asteroids_pen_test() {
	use crate::{
		client::{self, ClientState},
		custom::CustomElement,
		elements::Pen,
	};
	use mint::{Quaternion, Vector3};
	use serde::{Deserialize, Serialize};

	#[derive(Default, Serialize, Deserialize)]
	struct TestState {
		#[serde(skip)]
		pen_state: Option<String>,
	}

	impl TestState {
		pub fn update_pen_state(&mut self, state: String) {
			self.pen_state = Some(state);
		}
	}

	impl crate::util::Migrate for TestState {
		type Old = Self;
	}

	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.pen";
	}
	impl crate::Reify for TestState {
		fn reify(&self) -> impl crate::Element<Self> {
			Pen::new(
				Vector3 {
					x: 0.0,
					y: 0.0,
					z: 0.0,
				},
				Quaternion {
					v: Vector3 {
						x: 0.0,
						y: 0.0,
						z: 0.0,
					},
					s: 1.0,
				},
				|state: &mut TestState, pen_state, pos, rot| {
					state.update_pen_state(format!("{pen_state:?} at {pos:?} {rot:?}"));
				},
			)
			.length(0.1)
			.thickness(0.01)
			.build()
		}
	}

	client::run::<TestState>(&[]).await;
}
