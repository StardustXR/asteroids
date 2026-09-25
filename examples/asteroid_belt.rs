use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};
use stardust_xr_asteroids::{
	ClientState, Context, CustomElement, Element, Entity, Migrate, Reify, Tasker, Transformable,
	client,
	components::{Derezzable, Grabbable, Lines, PointerMode},
	elements::{BeamQuery, BeamQueryCache, Lines as LineSet, Spatial, Text},
};
use stardust_xr_fusion::{
	client::FrameInfo,
	drawable::{Line, XAlign},
	fields::Shape,
	types::{Posef, rgba_linear},
};
use stardust_xr_molecules::{
	derezzable::protocol::Derezzable as DerezzableProxy,
	lines::{LineExt, line_from_points},
};
use std::f32::consts::TAU;
use tracing_subscriber::EnvFilter;

const CENTER: Vec3 = Vec3::new(0.0, 0.0, -0.9);
const HALF: Vec3 = Vec3::new(0.6, 0.35, 0.45);
const NOSE: f32 = 0.07;
const LASER: f32 = 2.5;
const CHARGE: f32 = 0.4;
const BIG: f32 = 0.07;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	tracing_subscriber::fmt()
		.with_env_filter(EnvFilter::from_default_env())
		.init();
	client::run::<Belt>(&[]).await.unwrap()
}

#[derive(Clone, Serialize, Deserialize)]
struct Rock {
	id: u64,
	pos: [f32; 3],
	vel: [f32; 3],
	rot: [f32; 4],
	/// axis scaled by radians per second
	spin: [f32; 3],
	radius: f32,
	#[serde(skip)]
	grabbed: bool,
	/// the belt picks this up next frame, since splitting needs the whole belt
	#[serde(skip)]
	shattered: bool,
}

struct RockProps {
	laser: (Vec3, Vec3),
	heat: f32,
	dt: f32,
}
impl Reify<RockProps> for Rock {
	fn reify(
		&self,
		_context: &Context,
		_tasks: impl Tasker<Self>,
		props: RockProps,
	) -> impl Element<Self> {
		let dt = props.dt;
		Entity::new(Shape::Sphere {
			radius: self.radius,
		})
		.pos(self.pos)
		.rot(self.rot)
		.component(
			Grabbable::new(move |r: &mut Self, pose| r.throw(pose, dt))
				.grab_start(|r: &mut Self| r.grabbed = true)
				.grab_stop(|r: &mut Self| r.grabbed = false),
		)
		.component(Derezzable::new(|r: &mut Self| r.shattered = true))
		.component(Lines::new(self.lines(props)))
		.build()
	}
}
impl Rock {
	fn throw(&mut self, pose: Posef, dt: f32) {
		let p = Vec3::from(pose.position);
		self.vel = ((p - Vec3::from_array(self.pos)) / dt.max(0.001))
			.clamp_length_max(0.8)
			.to_array();
		self.pos = p.to_array();
		self.rot = Quat::from(pose.orientation).to_array();
	}

	fn lines(&self, props: RockProps) -> Vec<Line> {
		let heat = props.heat;
		let (origin, dir) = props.laser;
		let inv = Quat::from_array(self.rot).inverse();
		let lo = inv * (origin - Vec3::from_array(self.pos));
		let ld = inv * dir;
		let color = rgba_linear!(
			0.75 + 0.25 * heat,
			0.78 - 0.3 * heat,
			0.85 - 0.7 * heat,
			1.0
		);

		[
			(Vec3::X, Vec3::Y),
			(Vec3::Y, Vec3::Z),
			(Vec3::Z, Vec3::X),
		]
		.into_iter()
		.enumerate()
		.map(|(k, (a, b))| {
			let mut ring = line_from_points(
				(0..11)
					.map(|i| {
						let t = i as f32 / 11.0 * TAU;
						let jag = 0.72 + 0.4 * hash01(self.id, k as u64, i);
						(a * t.cos() + b * t.sin()) * self.radius * jag
					})
					.collect(),
			);
			ring.cyclic = true;
			ring.thickness(0.0025 + 0.003 * heat)
				.color(color)
				.shimmer_ray(lo, ld, 0.05, 0.0, rgba_linear!(1.0, 0.45, 0.1, 1.0), 2.5)
		})
		.collect()
	}
}

struct Spark {
	pos: Vec3,
	vel: Vec3,
	life: f32,
	max: f32,
	hot: f32,
}

#[derive(Default)]
struct Run {
	hits: BeamQueryCache<(DerezzableProxy,)>,
	sparks: Vec<Spark>,
	dt: f32,
	time: f32,
	charge: f32,
	target: Option<u64>,
	laser_end: f32,
	blocked: bool,
	banner: f32,
	ship_grabbed: bool,
}

#[derive(Serialize, Deserialize)]
struct Belt {
	ship: Posef,
	rocks: Vec<Rock>,
	next_id: u64,
	rng: u64,
	score: u32,
	wave: u32,
	#[serde(skip)]
	run: Run,
}
impl Default for Belt {
	fn default() -> Self {
		Belt {
			ship: Posef {
				position: [0.0, -0.1, -0.3].into(),
				..Default::default()
			},
			rocks: Vec::new(),
			next_id: 0,
			rng: 0x9E37_79B9_7F4A_7C15,
			score: 0,
			wave: 0,
			run: Run::default(),
		}
	}
}
impl Migrate for Belt {
	type Old = Self;
}
impl ClientState for Belt {
	const APP_ID: &'static str = "org.asteroids.AsteroidBelt";

	fn on_frame(&mut self, info: &FrameInfo) {
		let dt = info.delta.clamp(0.0, 0.1);
		self.run.dt = dt;
		self.run.time += dt;
		self.run.banner -= dt;
		if self.rocks.is_empty() {
			self.next_wave();
		}

		let lo = CENTER - HALF;
		for r in self.rocks.iter_mut().filter(|r| !r.grabbed) {
			let p = Vec3::from_array(r.pos) + Vec3::from_array(r.vel) * dt;
			r.pos = (lo + (p - lo).rem_euclid(HALF * 2.0)).to_array();
			r.rot = (Quat::from_scaled_axis(Vec3::from_array(r.spin) * dt)
				* Quat::from_array(r.rot))
			.normalize()
			.to_array();
		}

		self.run.sparks.retain(|s| s.life > 0.0);
		for s in &mut self.run.sparks {
			s.pos += s.vel * dt;
			s.vel *= 1.0 - 2.5 * dt;
			s.life -= dt;
		}

		self.aim(dt);

		let doomed: Vec<u64> = self
			.rocks
			.iter()
			.filter(|r| r.shattered)
			.map(|r| r.id)
			.collect();
		for id in doomed {
			self.shatter(id);
		}
	}
}
impl Belt {
	fn rand(&mut self) -> f32 {
		self.rng ^= self.rng << 13;
		self.rng ^= self.rng >> 7;
		self.rng ^= self.rng << 17;
		(self.rng >> 40) as f32 / (1u64 << 24) as f32
	}
	fn rand_dir(&mut self) -> Vec3 {
		Vec3::new(self.rand(), self.rand(), self.rand())
			.mul_add(Vec3::splat(2.0), Vec3::NEG_ONE)
			.normalize_or(Vec3::Y)
	}

	fn spawn_rock(&mut self, pos: Vec3, vel: Vec3, radius: f32) {
		let id = self.next_id;
		self.next_id += 1;
		let spin = self.rand_dir() * (0.3 + self.rand() * 1.2);
		self.rocks.push(Rock {
			id,
			pos: pos.to_array(),
			vel: vel.to_array(),
			rot: Quat::IDENTITY.to_array(),
			spin: spin.to_array(),
			radius,
			grabbed: false,
			shattered: false,
		});
	}
	fn next_wave(&mut self) {
		self.wave += 1;
		self.run.banner = 2.5;
		for _ in 0..3 + self.wave {
			let p = Vec3::new(self.rand(), self.rand(), self.rand())
				.mul_add(Vec3::splat(2.0), Vec3::NEG_ONE);
			let v = self.rand_dir() * (0.02 + self.rand() * 0.03 + 0.005 * self.wave as f32);
			self.spawn_rock(CENTER + p * HALF * 0.8, v, BIG);
		}
	}

	fn shatter(&mut self, id: u64) {
		let Some(i) = self.rocks.iter().position(|r| r.id == id) else {
			return;
		};
		let rock = self.rocks.swap_remove(i);
		let pos = Vec3::from_array(rock.pos);
		let vel = Vec3::from_array(rock.vel);
		self.score += match rock.radius {
			r if r > 0.05 => 20,
			r if r > 0.03 => 50,
			_ => 100,
		};
		if rock.radius > 0.03 {
			for _ in 0..2 + (self.rand() * 1.5) as usize {
				let d = self.rand_dir();
				let v = vel + d * (0.05 + self.rand() * 0.06);
				self.spawn_rock(pos + d * rock.radius * 0.4, v, rock.radius * 0.6);
			}
		}
		for _ in 0..28 {
			let d = self.rand_dir();
			let vel = vel + d * (0.2 + self.rand() * 0.7);
			let max = 0.3 + self.rand() * 0.7;
			let hot = self.rand();
			self.run.sparks.push(Spark {
				pos,
				vel,
				life: max,
				max,
				hot,
			});
		}
		if self.run.target == Some(id) {
			self.run.target = None;
			self.run.charge = 0.0;
		}
	}

	fn laser(&self) -> (Vec3, Vec3) {
		let rot = Quat::from(self.ship.orientation);
		(
			Vec3::from(self.ship.position) + rot * Vec3::new(0.0, 0.0, -NOSE),
			rot * Vec3::NEG_Z,
		)
	}

	// the beam hits anything derezzable, other apps included, so it only ever shatters
	// rocks it can find in our own state and never calls derez over ipc
	fn aim(&mut self, dt: f32) {
		let (origin, dir) = self.laser();
		let nearest = self
			.run
			.hits
			.0
			.values()
			.map(|q| q.ray.deepest_point_distance)
			.min_by(f32::total_cmp);
		let Some(d) = nearest else {
			self.run.laser_end = LASER;
			self.run.blocked = false;
			self.run.target = None;
			self.run.charge = 0.0;
			return;
		};
		let hit = origin + dir * d;
		let rock = self
			.rocks
			.iter()
			.find(|r| Vec3::from_array(r.pos).distance(hit) < r.radius * 1.3);

		self.run.laser_end = match rock {
			Some(r) => {
				let c = Vec3::from_array(r.pos) - origin;
				let along = c.dot(dir);
				let miss = (c - dir * along).length_squared();
				along - (r.radius * r.radius - miss).max(0.0).sqrt() * 0.8
			}
			None => d,
		}
		.max(0.0);
		self.run.blocked = rock.is_none();

		let id = rock.map(|r| r.id);
		if id != self.run.target {
			self.run.target = id;
			self.run.charge = 0.0;
		} else if let Some(id) = id {
			self.run.charge += dt;
			if self.run.charge >= CHARGE
				&& let Some(r) = self.rocks.iter_mut().find(|r| r.id == id)
			{
				r.shattered = true;
			}
		}
	}

	fn heat(&self) -> f32 {
		(self.run.charge / CHARGE).min(1.0)
	}

	fn ship_lines(&self) -> Vec<Line> {
		let t = self.run.time;
		let hull_color = rgba_linear!(0.6, 0.95, 1.0, 1.0);
		let mut hull = line_from_points(vec![
			[0.0, 0.0, -0.05],
			[
				0.032, 0.0, 0.035,
			],
			[0.0, 0.0, 0.018],
			[
				-0.032, 0.0, 0.035,
			],
		]);
		hull.cyclic = true;
		let mut lines = vec![
			hull.color(hull_color).thickness(0.003),
			line_from_points(vec![
				[0.0, 0.0, -0.01],
				[0.0, 0.02, 0.03],
				[0.0, 0.0, 0.03],
			])
			.color(hull_color)
			.thickness(0.002),
		];

		if self.run.ship_grabbed {
			let flame = 0.02 + 0.015 * (t * 43.0).sin().abs();
			lines.push(
				line_from_points(vec![
					[
						-0.012, 0.0, 0.026,
					],
					[
						0.0,
						0.0,
						0.026 + flame,
					],
					[
						0.012, 0.0, 0.026,
					],
				])
				.color(rgba_linear!(1.0, 0.55, 0.1, 1.0))
				.thickness(0.002),
			);
		}

		let start = Vec3::new(0.0, 0.0, -NOSE);
		let end = start + Vec3::NEG_Z * self.run.laser_end;
		let heat = self.heat();
		let pulse = 1.0 + 0.25 * (t * 30.0).sin();
		let (color, thickness) = match (self.run.blocked, self.run.target) {
			(true, _) => (rgba_linear!(1.0, 0.7, 0.1, 1.0), 0.002),
			(_, Some(_)) => (
				rgba_linear!(1.0, 0.25 + 0.75 * heat, 0.15 + 0.85 * heat, 1.0),
				(0.002 + 0.006 * heat) * pulse,
			),
			_ => (rgba_linear!(1.0, 0.1, 0.1, 0.8), 0.0015),
		};
		lines.push(
			line_from_points(vec![start, end])
				.color(color)
				.thickness(thickness),
		);

		if self.run.blocked {
			let s = 0.015;
			for (a, b) in [
				(Vec3::X, Vec3::Y),
				(Vec3::X, Vec3::NEG_Y),
			] {
				lines.push(
					line_from_points(vec![
						end - (a + b) * s,
						end + (a + b) * s,
					])
					.color(color)
					.thickness(0.003),
				);
			}
		} else if self.run.target.is_some() {
			let s = 0.008 + 0.02 * heat * pulse;
			for a in [
				Vec3::X,
				Vec3::Y,
				Vec3::Z,
			] {
				lines.push(
					line_from_points(vec![
						end - a * s,
						end + a * s,
					])
					.color(color)
					.thickness(0.002),
				);
			}
		}
		lines
	}

	fn effect_lines(&self) -> Vec<Line> {
		let sparks = self.run.sparks.iter().map(|s| {
			let fade = s.life / s.max;
			line_from_points(vec![
				s.pos - s.vel * 0.03,
				s.pos,
			])
			.color(rgba_linear!(
				1.0,
				0.5 + 0.5 * s.hot * fade,
				0.25 * fade,
				fade
			))
			.thickness(0.0005 + 0.003 * fade)
		});
		let mut corners = Vec::new();
		for x in [-1.0, 1.0] {
			for y in [-1.0, 1.0] {
				for z in [-1.0, 1.0] {
					let signs = Vec3::new(x, y, z);
					let c = CENTER + HALF * signs;
					for e in [
						Vec3::X,
						Vec3::Y,
						Vec3::Z,
					] {
						corners.push(
							line_from_points(vec![
								c,
								c - e * signs * 0.08,
							])
							.color(rgba_linear!(0.3, 0.8, 1.0, 0.5))
							.thickness(0.0015),
						);
					}
				}
			}
		}
		sparks.chain(corners).collect()
	}

	fn ship(&self, context: &Context) -> impl Element<Self> + use<> {
		Entity::new(Shape::Sphere { radius: 0.04 })
			.pose(self.ship)
			.component(
				Grabbable::new(|s: &mut Self, pose| s.ship = pose)
					.grab_start(|s: &mut Self| s.run.ship_grabbed = true)
					.grab_stop(|s: &mut Self| s.run.ship_grabbed = false)
					.max_distance(0.08)
					.pointer_mode(PointerMode::Align),
			)
			.component(Derezzable::program_stopper(context))
			.component(Lines::new(self.ship_lines()))
			.build()
			.child(
				BeamQuery::new_cached([0.0, 0.0, -NOSE], [0.0, 0.0, -1.0], |s: &mut Self| {
					&mut s.run.hits
				})
				.max_length(LASER)
				.margin(0.005)
				.build(),
			)
	}

	fn rock_props(&self, r: &Rock) -> RockProps {
		RockProps {
			laser: self.laser(),
			heat: if self.run.target == Some(r.id) {
				self.heat()
			} else {
				0.0
			},
			dt: self.run.dt,
		}
	}
}
impl Reify for Belt {
	fn reify(&self, context: &Context, tasks: impl Tasker<Self>, _props: ()) -> impl Element<Self> {
		Spatial::default()
			.build()
			.child(self.ship(context))
			.stable_children(self.rocks.iter().map(|r| {
				let id = r.id;
				let rock = r.reify_substate(
					context,
					tasks.clone(),
					self.rock_props(r),
					move |s: &mut Self| s.rocks.iter_mut().find(|r| r.id == id),
				);
				(id, rock)
			}))
			.child(LineSet::new(self.effect_lines()).build())
			.child(
				Text::new(format!("SCORE {:05}    WAVE {}", self.score, self.wave))
					.character_height(0.035)
					.align_x(XAlign::Center)
					.color(rgba_linear!(0.6, 0.95, 1.0, 1.0))
					.pos(CENTER + Vec3::Y * (HALF.y + 0.08))
					.build(),
			)
			.maybe_child((self.run.banner > 0.0).then(|| {
				Text::new(format!("WAVE {}", self.wave))
					.character_height(0.09)
					.align_x(XAlign::Center)
					.color(rgba_linear!(1.0, 1.0, 1.0, self.run.banner.min(1.0)))
					.pos(CENTER)
					.build()
			}))
	}
}

fn hash01(id: u64, k: u64, i: u64) -> f32 {
	let mut x = id
		.wrapping_mul(0x9E37_79B9_7F4A_7C15)
		.wrapping_add(k << 32 | i);
	x = (x ^ (x >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
	x = (x ^ (x >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
	((x ^ (x >> 31)) >> 40) as f32 / (1u64 << 24) as f32
}
