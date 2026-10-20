use glam::{Quat, Vec3};
use serde::{Deserialize, Serialize};
use stardust_xr_asteroids::{
	ClientState, Context, CustomElement, Element, Entity, Migrate, Mix, Reify, Tasker,
	Transformable, client,
	components::{Derezzable, Grabbable, Lines, PointerMode},
	ease_in, ease_out,
	elements::{BeamQuery, BeamQueryCache, Lines as LineSet, Spatial, Text},
	linear, spring,
};
use stardust_xr_fusion::{
	client::FrameInfo,
	drawable::{Line, XAlign},
	fields::Shape,
	spatial::Transform,
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
const FLAPS: &str = " ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

#[tokio::main(flavor = "current_thread")]
async fn main() {
	tracing_subscriber::fmt()
		.with_env_filter(EnvFilter::from_default_env())
		.init();
	client::run::<Belt>(&[]).await.unwrap()
}

/// drifts in a straight line from where it was last thrown, wrapping around the belt
#[derive(Serialize, Deserialize)]
struct Rock {
	id: u64,
	pos: Vec3,
	vel: Vec3,
	rot: Quat,
	/// axis scaled by radians per second
	spin: Vec3,
	thrown: f32,
	radius: f32,
}
impl Rock {
	fn at(&self, time: f32) -> (Vec3, Quat) {
		let t = time - self.thrown;
		let lo = CENTER - HALF;
		let p = self.pos + self.vel * t;
		(
			lo + (p - lo).rem_euclid(HALF * 2.0),
			Quat::from_scaled_axis(self.spin * t) * self.rot,
		)
	}

	fn lines(&self, pos: Vec3, rot: Quat, (origin, dir): (Vec3, Vec3), heat: f32) -> Vec<Line> {
		let lo = rot.inverse() * (origin - pos);
		let ld = rot.inverse() * dir;
		let color = rgba_linear!(
			0.75 + 0.25 * heat,
			0.78 - 0.3 * heat,
			0.85 - 0.7 * heat,
			1.0
		);
		let mut rng = fastrand::Rng::with_seed(self.id);

		[
			(Vec3::X, Vec3::Y),
			(Vec3::Y, Vec3::Z),
			(Vec3::Z, Vec3::X),
		]
		.into_iter()
		.map(|(a, b)| {
			let mut ring = line_from_points(
				(0..11)
					.map(|i| {
						let t = i as f32 / 11.0 * TAU;
						(a * t.cos() + b * t.sin()) * self.radius * (0.72 + 0.4 * rng.f32())
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

struct Burst {
	id: u64,
	pos: Vec3,
	sparks: Vec<Line>,
	born: f32,
}

struct Aim {
	end: f32,
	target: Option<u64>,
	blocked: bool,
}

#[derive(Default)]
struct Run {
	hits: BeamQueryCache<(DerezzableProxy,)>,
	bursts: Vec<Burst>,
	/// the rock the beam is on and since when
	lock: Option<(u64, f32)>,
	ship_grabbed: bool,
	/// `time` when this launch started, since `context.time()` starts over every launch
	epoch: f32,
}

#[derive(Serialize, Deserialize)]
struct Belt {
	/// game seconds at the predicted display time, saved so the rocks pick up where they left off
	time: f32,
	ship_pos: Vec3,
	ship_rot: Quat,
	rocks: Vec<Rock>,
	next_id: u64,
	score: u32,
	wave: u32,
	wave_started: f32,
	#[serde(skip)]
	run: Run,
}
impl Default for Belt {
	fn default() -> Self {
		Belt {
			time: 0.0,
			ship_pos: Vec3::new(0.0, -0.1, -0.3),
			ship_rot: Quat::IDENTITY,
			rocks: Vec::new(),
			next_id: 0,
			score: 0,
			wave: 0,
			wave_started: 0.0,
			run: Run::default(),
		}
	}
}
impl Migrate for Belt {
	type Old = Self;
}
impl ClientState for Belt {
	const APP_ID: &'static str = "org.asteroids.AsteroidBelt";

	fn on_start(&mut self, _context: &Context, _tasks: impl Tasker<Self>) {
		self.run.epoch = self.time;
		if self.rocks.is_empty() {
			self.next_wave();
		}
	}

	// holding the beam on a rock is the one thing that really builds up frame by frame
	fn on_frame(&mut self, context: &Context, _info: &FrameInfo) {
		self.time = self.run.epoch + context.time();
		let target = self.aim().target;
		match self.run.lock {
			Some((id, since)) if Some(id) == target => {
				if self.time - since >= CHARGE {
					self.shatter(id);
				}
			}
			_ => self.run.lock = target.map(|id| (id, self.time)),
		}
	}
}
impl Belt {
	fn spawn_rock(&mut self, pos: Vec3, vel: Vec3, radius: f32) {
		self.next_id += 1;
		self.rocks.push(Rock {
			id: self.next_id,
			pos,
			vel,
			rot: Quat::IDENTITY,
			spin: rand_dir() * (0.3 + fastrand::f32() * 1.2),
			thrown: self.time,
			radius,
		});
	}
	fn next_wave(&mut self) {
		self.wave += 1;
		self.wave_started = self.time;
		for _ in 0..3 + self.wave {
			let p = Vec3::new(fastrand::f32(), fastrand::f32(), fastrand::f32()) * 2.0 - 1.0;
			let v = rand_dir() * (0.02 + fastrand::f32() * 0.03 + 0.005 * self.wave as f32);
			self.spawn_rock(CENTER + p * HALF * 0.8, v, BIG);
		}
	}

	fn throw(&mut self, id: u64, pose: Posef) {
		let time = self.time;
		let Some(r) = self.rocks.iter_mut().find(|r| r.id == id) else {
			return;
		};
		let p = Vec3::from(pose.position);
		// grab updates can land more than once a frame
		if time > r.thrown {
			r.vel = ((p - r.pos) / (time - r.thrown)).clamp_length_max(0.8);
		}
		r.pos = p;
		r.rot = pose.orientation.into();
		r.thrown = time;
	}

	fn shatter(&mut self, id: u64) {
		let Some(i) = self.rocks.iter().position(|r| r.id == id) else {
			return;
		};
		let rock = self.rocks.swap_remove(i);
		let (pos, _) = rock.at(self.time);
		let vel = rock.vel;
		self.score += match rock.radius {
			r if r > 0.05 => 20,
			r if r > 0.03 => 50,
			_ => 100,
		};
		if rock.radius > 0.03 {
			for _ in 0..fastrand::usize(2..4) {
				let d = rand_dir();
				let v = vel + d * (0.05 + fastrand::f32() * 0.06);
				self.spawn_rock(pos + d * rock.radius * 0.4, v, rock.radius * 0.6);
			}
		}

		let time = self.time;
		self.run.bursts.retain(|b| time - b.born < 1.0);
		self.run.bursts.push(Burst {
			id,
			pos,
			sparks: (0..28).map(|_| spark(vel)).collect(),
			born: time,
		});
		if self.rocks.is_empty() {
			self.next_wave();
		}
	}

	fn laser(&self) -> (Vec3, Vec3) {
		(
			self.ship_pos + self.ship_rot * Vec3::new(0.0, 0.0, -NOSE),
			self.ship_rot * Vec3::NEG_Z,
		)
	}

	// the beam hits anything derezzable, other apps included, so it only ever shatters
	// rocks it can find in our own state and never calls derez over ipc
	fn aim(&self) -> Aim {
		let (origin, dir) = self.laser();
		let nearest = self
			.run
			.hits
			.0
			.values()
			.map(|q| q.ray.deepest_point_distance)
			.min_by(f32::total_cmp);
		let Some(d) = nearest else {
			return Aim {
				end: LASER,
				target: None,
				blocked: false,
			};
		};
		let hit = origin + dir * d;
		let rock = self
			.rocks
			.iter()
			.map(|r| (r, r.at(self.time).0))
			.find(|(r, p)| p.distance(hit) < r.radius * 1.3);
		let Some((r, p)) = rock else {
			return Aim {
				end: d,
				target: None,
				blocked: true,
			};
		};
		let c = p - origin;
		let along = c.dot(dir);
		let miss = (c - dir * along).length_squared();
		Aim {
			end: (along - (r.radius * r.radius - miss).max(0.0).sqrt() * 0.8).max(0.0),
			target: Some(r.id),
			blocked: false,
		}
	}

	fn heat(&self, id: u64) -> f32 {
		match self.run.lock {
			Some((locked, since)) if locked == id => ((self.time - since) / CHARGE).min(1.0),
			_ => 0.0,
		}
	}

	fn ship_lines(&self, aim: &Aim) -> Vec<Line> {
		let t = self.time;
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
		let end = start + Vec3::NEG_Z * aim.end;
		let heat = aim.target.map_or(0.0, |id| self.heat(id));
		let pulse = 1.0 + 0.25 * (t * 30.0).sin();
		let (color, thickness) = match (aim.blocked, aim.target) {
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

		if aim.blocked {
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
		} else if aim.target.is_some() {
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

	fn ship(&self, context: &Context, aim: &Aim) -> impl Element<Self> + use<> {
		let shrunk =
			Transform::from_translation_rotation_scale(self.ship_pos, self.ship_rot, [0.0; 3]);
		Entity::new(Shape::Sphere { radius: 0.04 })
			.pos(self.ship_pos)
			.rot(self.ship_rot)
			.component(
				Grabbable::new(|s: &mut Self, pose| {
					s.ship_pos = pose.position.into();
					s.ship_rot = pose.orientation.into();
				})
				.grab_start(|s: &mut Self| s.run.ship_grabbed = true)
				.grab_stop(|s: &mut Self| s.run.ship_grabbed = false)
				.max_distance(0.08)
				.pointer_mode(PointerMode::Align),
			)
			.component(Derezzable::program_stopper(context))
			.component(Lines::new(self.ship_lines(aim)))
			.build()
			.appear(|a| a.transform(shrunk, spring(0.6, 0.3)))
			.vanish(|a| a.transform(shrunk, ease_in(0.3)))
			.child(
				BeamQuery::new_cached([0.0, 0.0, -NOSE], [0.0, 0.0, -1.0], |s: &mut Self| {
					&mut s.run.hits
				})
				.max_length(LASER)
				.margin(0.005)
				.build(),
			)
	}

	fn rock(&self, r: &Rock) -> impl Element<Self> + use<> {
		let id = r.id;
		let (pos, rot) = r.at(self.time);
		let shrunk = Transform::from_translation_rotation_scale(pos, rot, [0.0; 3]);
		Entity::new(Shape::Sphere { radius: r.radius })
			.pos(pos)
			.rot(rot)
			.component(Grabbable::new(move |s: &mut Self, pose| s.throw(id, pose)))
			.component(Derezzable::new(move |s: &mut Self| s.shatter(id)))
			.component(Lines::new(r.lines(pos, rot, self.laser(), self.heat(id))))
			.build()
			.appear(|a| a.transform(shrunk, spring(0.5, 0.4)))
			.vanish(|a| a.transform(shrunk, ease_in(0.12)))
	}
}
impl Reify for Belt {
	fn reify(
		&self,
		context: &Context,
		_tasks: impl Tasker<Self>,
		_props: (),
	) -> impl Element<Self> {
		Spatial::default()
			.build()
			.child(self.ship(context, &self.aim()))
			.stable_children(self.rocks.iter().map(|r| (r.id, self.rock(r))))
			.stable_children(
				self.run
					.bursts
					.iter()
					.filter(|b| self.time - b.born < 1.0)
					.map(|b| {
						let burst = LineSet::new(b.sparks.clone())
							.pos(b.pos)
							.build()
							.appear(|a| a.lines(vec![], ease_out(1.0).via(fly)));
						(b.id, burst)
					}),
			)
			.child(LineSet::new(corners()).build())
			.child(
				Text::new(format!("SCORE {:05}    WAVE {}", self.score, self.wave))
					.character_height(0.035)
					.align_x(XAlign::Center)
					.color(rgba_linear!(0.6, 0.95, 1.0, 1.0))
					.pos(CENTER + Vec3::Y * (HALF.y + 0.08))
					.build()
					.animation(|a| a.text(ease_out(0.6).via(tally))),
			)
			.maybe_child((self.time - self.wave_started < 2.5).then(|| {
				Text::new(format!("WAVE {}", self.wave))
					.character_height(0.09)
					.align_x(XAlign::Center)
					.pos(CENTER)
					.build()
					.appear(|a| a.text(String::new(), linear(1.0).via(flap)))
					.vanish(|a| a.text(String::new(), linear(1.0).via(flap)))
			}))
	}
}

fn rand_dir() -> Vec3 {
	(Vec3::new(fastrand::f32(), fastrand::f32(), fastrand::f32()) * 2.0 - 1.0).normalize_or(Vec3::Y)
}

fn spark(vel: Vec3) -> Line {
	let p = vel * 0.5 + rand_dir() * (0.08 + fastrand::f32() * 0.28);
	// this is where the spark burns out, `fly` gets it there
	line_from_points(vec![p * 0.85, p])
		.color(rgba_linear!(1.0, 0.5 + 0.5 * fastrand::f32(), 0.25, 0.0))
		.thickness(0.0)
}

#[allow(clippy::ptr_arg)]
fn fly(_from: &Vec<Line>, to: &Vec<Line>, t: f32) -> Vec<Line> {
	to.iter()
		.map(|l| {
			let mut l = l.clone();
			for p in &mut l.points {
				p.point = (Vec3::from(p.point) * t).into();
				p.color.a = 1.0 - t;
				p.thickness = 0.0035 * (1.0 - t);
			}
			l
		})
		.collect()
}

// every character flips forward through FLAPS at the same rate and stops when it lands, like a
// split-flap board, anything missing from FLAPS or past the end of a string counts as a blank
#[allow(clippy::ptr_arg)]
fn flap(from: &String, to: &String, t: f32) -> String {
	let flaps: Vec<char> = FLAPS.chars().collect();
	let n = flaps.len();
	let at = |c: Option<char>| {
		c.and_then(|c| flaps.iter().position(|&f| f == c))
			.unwrap_or(0)
	};
	let flipped = (t * n as f32) as usize;
	let (mut a, mut b) = (from.chars(), to.chars());
	(0..from.chars().count().max(to.chars().count()))
		.map(|_| {
			let (i, j) = (at(a.next()), at(b.next()));
			flaps[(i + ((j + n - i) % n).min(flipped)) % n]
		})
		.collect()
}

// rolls every number in the text up from what's shown, keeping its zero padding
#[allow(clippy::ptr_arg)]
fn tally(from: &String, to: &String, t: f32) -> String {
	let digits = |c: char| c.is_ascii_digit();
	let mut olds = from
		.split(|c: char| !digits(c))
		.filter_map(|n| n.parse::<u32>().ok());
	let mut out = String::new();
	let mut rest = to.as_str();
	while let Some(i) = rest.find(digits) {
		out += &rest[..i];
		rest = &rest[i..];
		let w = rest.find(|c: char| !digits(c)).unwrap_or(rest.len());
		let new = rest[..w].parse::<u32>().unwrap_or(0) as f32;
		let old = olds.next().map_or(new, |o| o as f32);
		out += &format!("{:0w$}", old.mix(&new, t).round() as u32);
		rest = &rest[w..];
	}
	out + rest
}

fn corners() -> Vec<Line> {
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
	corners
}
