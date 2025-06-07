use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, FnWrapper, Transformable, derive_setters::Setters},
};
use derive_where::derive_where;
use glam::{Quat, Vec3};
use map_range::MapRange;
use stardust_xr_fusion::{
	drawable::{Line, LinePoint, Lines, LinesAspect},
	fields::{CylinderShape, Field, FieldAspect, Shape},
	input::{InputData, InputDataType, InputHandler},
	node::NodeError,
	root::FrameInfo,
	spatial::{Spatial, SpatialAspect, SpatialRef, SpatialRefAspect, Transform},
	values::color::rgba_linear,
};
use stardust_xr_molecules::input_action::{InputQueue, InputQueueable, SimpleAction, SingleAction};
use std::f32::consts::{FRAC_PI_2, TAU};

type OnRotate<State> = FnWrapper<dyn Fn(&mut State, f32) + Send + Sync>;
#[derive(Setters)]
#[derive_where(Debug)]
pub struct Turntable<State: ValidState> {
	#[setters(skip)]
	transform: Transform,
	#[setters(skip)]
	rotation: f32,
	line_count: u32,
	line_thickness: f32,
	height: f32,
	inner_radius: f32,
	scroll_multiplier: f32,
	#[setters(skip)]
	on_rotate: OnRotate<State>,
}
impl<State: ValidState> Transformable for Turntable<State> {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}
impl<State: ValidState> CustomElement<State> for Turntable<State> {
	type Inner = TurntableInner;
	type Resource = ();
	type Error = stardust_xr_fusion::node::NodeError;

	fn create_inner(
		&self,
		_asteroids_context: &Context,
		info: CreateInnerInfo,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		TurntableInner::create(info.parent_space, self.transform, self)
	}

	fn update(
		&self,
		old_decl: &Self,
		_state: &mut State,
		inner: &mut Self::Inner,
		_resource: &mut Self::Resource,
	) {
		self.apply_transform(old_decl, &inner.root);
		if self.inner_radius != old_decl.inner_radius || self.height != old_decl.height {
			inner.set_size(self.inner_radius, self.height);
		}
	}

	fn frame(&self, info: &FrameInfo, state: &mut State, inner: &mut Self::Inner) {
		inner.update(info.clone(), self, state);
	}

	fn spatial_aspect(&self, inner: &Self::Inner) -> SpatialRef {
		inner.content_parent.clone().as_spatial_ref()
	}
}

impl<State: ValidState> Turntable<State> {
	pub fn new<F: Fn(&mut State, f32) + Send + Sync + 'static>(
		rotation: f32,
		on_rotate: F,
	) -> Self {
		Turntable {
			transform: Transform::identity(),
			rotation,
			line_count: 106,
			line_thickness: 0.002,
			height: 0.03,
			inner_radius: 0.5,
			scroll_multiplier: 10.0_f32.to_radians(),
			on_rotate: FnWrapper(Box::new(on_rotate)),
		}
	}
	fn grip_lines(&self) -> Vec<Line> {
		(0..self.line_count)
			.map(|c| (c as f32) / (self.line_count as f32) * TAU) // get angle from count
			.map(|a| a.sin_cos()) // get x+y from angle (unit circle)
			.map(|(x, y)| {
				let outer_radius = self.inner_radius + self.height;
				Line {
					points: vec![
						LinePoint {
							point: [x * self.inner_radius, 0.0, y * self.inner_radius].into(),
							thickness: self.line_thickness,
							color: rgba_linear!(1.0, 1.0, 1.0, 1.0),
						},
						LinePoint {
							point: [x * outer_radius, -self.height, y * outer_radius].into(),
							thickness: self.line_thickness,
							color: rgba_linear!(1.0, 1.0, 1.0, 1.0),
						},
					],
					cyclic: false,
				}
			})
			.collect()
	}
}

fn interact_point(input: &InputData) -> Option<Vec3> {
	match &input.input {
		InputDataType::Hand(h) => {
			Some(Vec3::from(h.thumb.tip.position).lerp(Vec3::from(h.index.tip.position), 0.5))
		}
		InputDataType::Tip(t) => Some(t.origin.into()),
		_ => None,
	}
}
fn interact_points(input: &InputData) -> Vec<Vec3> {
	match &input.input {
		InputDataType::Hand(h) => {
			vec![
				h.thumb.tip.position.into(),
				h.index.tip.position.into(),
				h.ring.tip.position.into(),
				h.middle.tip.position.into(),
				h.little.tip.position.into(),
			]
		}
		InputDataType::Tip(t) => vec![t.origin.into()],
		_ => vec![],
	}
}
fn interact_proximity(input: &InputQueue, point: Vec3) -> f32 {
	input
		.input()
		.keys()
		.flat_map(|i| match &i.input {
			InputDataType::Hand(h) => {
				vec![
					h.thumb.tip.position,
					h.index.tip.position,
					h.ring.tip.position,
					h.middle.tip.position,
					h.little.tip.position,
				]
			}
			InputDataType::Tip(t) => vec![t.origin],
			_ => vec![],
		})
		.map(|p| Vec3::from(p).distance(point))
		.reduce(|a, b| a.min(b))
		.unwrap_or(f32::INFINITY)
}
fn interact_angle(input: &InputData) -> Option<f32> {
	let p = interact_point(input)?;
	Some(p.z.atan2(p.x))
}

pub struct TurntableInner {
	root: Spatial,
	content_parent: Spatial,
	grip_lines: Vec<Line>,
	grip: Lines,
	field: Field,

	input: InputQueue,
	pointer_hover_action: SimpleAction,
	touch_action: SingleAction,
	angular_momentum: f32,
	prev_angle: Option<f32>,
}
impl TurntableInner {
	pub fn create<State: ValidState>(
		parent: &impl SpatialRefAspect,
		transform: Transform,
		settings: &Turntable<State>,
	) -> Result<Self, NodeError> {
		let root = Spatial::create(parent, transform, false)?;
		let content_parent = Spatial::create(&root, Transform::none(), false)?;
		let field = Field::create(
			&root,
			Transform::from_translation([0.0, -settings.height * 0.5, 0.0]),
			Shape::Cylinder(CylinderShape {
				length: settings.height,
				radius: settings.inner_radius + settings.height,
			}),
		)?;
		let input = InputHandler::create(&root, Transform::none(), &field)?.queue()?;

		let grip_lines: Vec<Line> = settings.grip_lines();
		let grip = Lines::create(&content_parent, Transform::none(), &grip_lines)?;

		Ok(Self {
			root,
			content_parent,
			grip_lines,
			grip,
			field,
			input,
			pointer_hover_action: Default::default(),
			touch_action: Default::default(),
			prev_angle: None,
			angular_momentum: 0.0,
		})
	}

	pub fn root(&self) -> &Spatial {
		&self.root
	}
	pub fn content_parent(&self) -> &Spatial {
		&self.content_parent
	}

	pub fn set_size(&self, inner_radius: f32, height: f32) {
		let _ = self
			.field
			.set_local_transform(Transform::from_translation_rotation(
				[0.0, -height * 0.5, 0.0],
				Quat::from_rotation_x(FRAC_PI_2),
			));
		let _ = self.field.set_shape(Shape::Cylinder(CylinderShape {
			length: height,
			radius: inner_radius + height,
		}));
	}

	#[inline]
	fn scroll(&self) -> f32 {
		self.pointer_hover_action
			.currently_acting()
			.iter()
			.map(|i| {
				i.datamap.with_data(|d| {
					let scroll = d.idx("scroll_continuous").as_vector();
					(scroll.idx(0).as_f32(), scroll.idx(1).as_f32())
				})
			})
			.map(|(scroll_x, scroll_y)| scroll_x + scroll_y)
			.reduce(|a, b| a + b)
			.unwrap_or_default()
	}
	pub fn rotate<State: ValidState>(
		&mut self,
		mut rotation: f32,
		angle: f32,
		state: &mut State,
		on_rotate: &OnRotate<State>,
	) {
		rotation += angle;
		let _ = self
			.content_parent
			.set_local_transform(Transform::from_rotation(Quat::from_rotation_y(rotation)));
		(on_rotate.0)(state, rotation);
	}
	pub fn update<State: ValidState>(
		&mut self,
		info: FrameInfo,
		settings: &Turntable<State>,
		state: &mut State,
	) {
		self.input.handle_events();
		self.update_pointer_hover(settings);
		self.update_touch(settings);
		self.update_scroll_rotation(settings, state);
		self.update_touch_rotation(&info, settings, state);
		self.update_momentum_rotation(&info, settings, state);
		self.update_grip_visuals(settings);
	}

	fn update_pointer_hover<State: ValidState>(&mut self, _settings: &Turntable<State>) {
		self.pointer_hover_action
			.update(&self.input, &|input| match &input.input {
				InputDataType::Pointer(_) => input.distance < 0.0,
				_ => false,
			});
	}

	fn update_touch<State: ValidState>(&mut self, settings: &Turntable<State>) {
		self.touch_action.update(
			false,
			&self.input,
			|_| true,
			|input| {
				let slope_condition = interact_points(input).into_iter().any(|p| {
					// p.y is always negative since input handler is center top of turntable, so this gets it relative to bottom
					let interact_point_height = p.y;
					// distance on XZ plane from center
					let interact_point_radius = p.x.hypot(p.z);
					// treat it as a cone so we can compare height to width for slope
					let interact_point_radius_slope =
						(interact_point_radius - settings.inner_radius).max(0.0);
					interact_point_height.abs() > interact_point_radius_slope
				});
				let distance_condition = input.distance < 0.0;
				slope_condition && distance_condition
			},
		);
	}

	fn update_scroll_rotation<State: ValidState>(
		&mut self,
		settings: &Turntable<State>,
		state: &mut State,
	) {
		let scroll_rotation = -self.scroll() * settings.scroll_multiplier;
		self.rotate(
			scroll_rotation,
			settings.rotation,
			state,
			&settings.on_rotate,
		);
	}

	fn update_touch_rotation<State: ValidState>(
		&mut self,
		info: &FrameInfo,
		settings: &Turntable<State>,
		state: &mut State,
	) {
		if let Some(angle) = self
			.touch_action
			.actor()
			.cloned()
			.as_deref()
			.and_then(interact_angle)
		{
			if let Some(prev_angle) = self.prev_angle {
				let delta = prev_angle - angle;
				self.angular_momentum = delta * info.delta;
				self.rotate(delta, settings.rotation, state, &settings.on_rotate);
			}
			self.prev_angle.replace(angle);
		}
		if self.touch_action.actor_stopped() {
			self.prev_angle.take();
		}
	}

	fn update_momentum_rotation<State: ValidState>(
		&mut self,
		info: &FrameInfo,
		settings: &Turntable<State>,
		state: &mut State,
	) {
		self.angular_momentum *= 0.98;
		if !self.touch_action.actor_acting() && self.angular_momentum.abs() > 0.0 {
			self.rotate(
				self.angular_momentum / info.delta,
				settings.rotation,
				state,
				&settings.on_rotate,
			);
		}
	}

	fn update_grip_visuals<State: ValidState>(&mut self, settings: &Turntable<State>) {
		for line in &mut self.grip_lines {
			for point in &mut line.points {
				let lerp = interact_proximity(
					&self.input,
					Quat::from_rotation_y(settings.rotation) * Vec3::from(point.point),
				)
				.map_range(0.05..0.0, 1.0..0.0)
				.clamp(0.0, 1.0);
				point.color = rgba_linear!(lerp, lerp, lerp, 1.0);
			}
		}
		self.grip.set_lines(&self.grip_lines).unwrap();
	}
}

#[tokio::test]
async fn asteroids_turntable_element() {
	use crate::{
		Element,
		client::{self, ClientState},
		custom::CustomElement,
		elements::{Lines, Turntable},
	};
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::spatial::BoundingBox;
	use stardust_xr_molecules::lines::{LineExt, bounding_box};

	#[derive(Default, Serialize, Deserialize)]
	struct TestState {
		#[serde(skip)]
		rotation: f32,
	}

	impl TestState {
		pub fn handle_rotation(&mut self, rotation: f32) {
			self.rotation = rotation;
		}
	}

	impl crate::util::Migrate for TestState {
		type Old = Self;
	}

	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.turntable";

		fn reify(&self) -> Element<Self> {
			crate::elements::Spatial::default()
				.zoneable(true)
				.build()
				.child(
					Turntable::new(self.rotation, Self::handle_rotation)
						.line_count(64)
						.line_thickness(0.002)
						.height(0.03)
						.inner_radius(0.1)
						.scroll_multiplier(1.0_f32.to_radians())
						.build()
						.child(
							Lines::new(
								bounding_box(BoundingBox {
									center: [0.0; 3].into(),
									size: [0.05; 3].into(),
								})
								.into_iter()
								.map(|l| l.thickness(0.002)),
							)
							.pos([0.0, 0.025, 0.0])
							.build(),
						),
				)
		}
	}

	client::run::<TestState>(&[]).await
}
