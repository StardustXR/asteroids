use crate::{Context, CreateInnerInfo, ValidState};
use glam::{Quat, Vec2, Vec3};
use stardust_xr_fusion::{
	client::FrameInfo,
	spatial::Transform,
	types::{Color, QuatF, Timestamp, Vec2F, Vec3F},
};
use std::{any::Any, fmt::Debug, sync::Arc};

/// every `Mix` field animates unless it's marked `#[no_animate]`
///
/// pair it with an `AnimatedElement` impl instead of `CustomElement`, the derive writes that one
///
/// ```compile_fail,E0277
/// use stardust_xr_asteroids::Animatable;
/// #[derive(Debug, Animatable)]
/// struct NotMix {
///     name: String,
/// }
/// let _ = NotMixAnimation::default().name(stardust_xr_asteroids::linear(1.0));
/// ```
///
/// ```compile_fail,E0599
/// use stardust_xr_asteroids::Animatable;
/// #[derive(Debug, Animatable)]
/// struct Skipped {
///     #[no_animate]
///     size: f32,
/// }
/// let _ = SkippedAnimation::default().size(stardust_xr_asteroids::linear(1.0));
/// ```
///
/// ```compile_fail,E0119
/// use stardust_xr_asteroids::{Animatable, Context, CreateInnerInfo, CustomElement, ValidState};
/// #[derive(Debug, Animatable)]
/// struct Both {
///     #[no_animate]
///     size: f32,
/// }
/// impl<State: ValidState> CustomElement<State> for Both {
///     type Inner = ();
///     type Error = std::convert::Infallible;
///     async fn create_inner(&self, _: &Context, _: CreateInnerInfo) -> Result<(), Self::Error> {
///         Ok(())
///     }
///     fn diff(&self, _: &Self, _: &Context, _: &mut ()) {}
/// }
/// ```
pub use stardust_xr_asteroids_derive::Animatable;

/// the built-in ones are plain values, only `curve(..)` allocates
// TODO: find a better way, running tracks outlive the reify so the curve's type has to be erased
// somewhere, and an enum was the least bad option without allocating
#[derive(Clone, Debug)]
pub struct Curve {
	shape: Shape,
	delay: f32,
}
impl Curve {
	/// waits before moving, only when it starts from rest so a chased target doesn't stall
	pub fn delay(mut self, seconds: f32) -> Self {
		self.delay += seconds;
		self
	}
	/// elapsed seconds in, progress out, `None` once it's done
	pub fn progress(&self, t: f32) -> Option<f32> {
		if t < self.delay {
			return Some(0.0);
		}
		self.shape.progress(t - self.delay)
	}
}

#[derive(Clone)]
enum Shape {
	Linear(f32),
	EaseIn(f32),
	EaseOut(f32),
	EaseInOut(f32),
	Spring { duration: f32, bounce: f32 },
	Custom(Arc<dyn CustomCurve>),
}
impl Shape {
	fn progress(&self, t: f32) -> Option<f32> {
		match *self {
			Shape::Linear(d) => (t < d).then(|| t / d),
			Shape::EaseIn(d) => (t < d).then(|| (t / d).powi(3)),
			Shape::EaseOut(d) => (t < d).then(|| 1.0 - (1.0 - t / d).powi(3)),
			Shape::EaseInOut(d) => (t < d).then(|| {
				let p = t / d;
				if p < 0.5 {
					4.0 * p.powi(3)
				} else {
					1.0 - (2.0 - 2.0 * p).powi(3) / 2.0
				}
			}),
			Shape::Spring { duration, bounce } => {
				let w = std::f32::consts::TAU / duration;
				let zeta = 1.0 - bounce.clamp(0.0, 0.99);
				let wd = w * (1.0 - zeta * zeta).sqrt();
				let decay = (-zeta * w * t).exp();
				(decay * (1.0 + w * t) > 1e-3).then(|| {
					if wd > 0.0 {
						1.0 - decay * ((wd * t).cos() + zeta * w / wd * (wd * t).sin())
					} else {
						1.0 - decay * (1.0 + w * t)
					}
				})
			}
			Shape::Custom(ref c) => c.progress(t),
		}
	}
}
impl Debug for Shape {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Shape::Linear(d) => f.debug_tuple("Linear").field(d).finish(),
			Shape::EaseIn(d) => f.debug_tuple("EaseIn").field(d).finish(),
			Shape::EaseOut(d) => f.debug_tuple("EaseOut").field(d).finish(),
			Shape::EaseInOut(d) => f.debug_tuple("EaseInOut").field(d).finish(),
			Shape::Spring { duration, bounce } => f
				.debug_struct("Spring")
				.field("duration", duration)
				.field("bounce", bounce)
				.finish(),
			Shape::Custom(_) => f.write_str("Custom"),
		}
	}
}
impl From<Shape> for Curve {
	fn from(shape: Shape) -> Self {
		Curve { shape, delay: 0.0 }
	}
}

pub trait CustomCurve: Send + Sync + 'static {
	fn progress(&self, t: f32) -> Option<f32>;
}
impl<F: Fn(f32) -> Option<f32> + Send + Sync + 'static> CustomCurve for F {
	fn progress(&self, t: f32) -> Option<f32> {
		self(t)
	}
}

pub fn curve(c: impl CustomCurve) -> Curve {
	Shape::Custom(Arc::new(c)).into()
}
pub fn linear(duration: f32) -> Curve {
	Shape::Linear(duration).into()
}
pub fn ease_in(duration: f32) -> Curve {
	Shape::EaseIn(duration).into()
}
pub fn ease_out(duration: f32) -> Curve {
	Shape::EaseOut(duration).into()
}
pub fn ease_in_out(duration: f32) -> Curve {
	Shape::EaseInOut(duration).into()
}
/// like swiftui's, `duration` is the period and `bounce` goes from 0 (none) toward 1 (forever)
///
/// it runs until it settles rather than for `duration`, and overshoots when it bounces
pub fn spring(duration: f32, bounce: f32) -> Curve {
	Shape::Spring { duration, bounce }.into()
}

#[diagnostic::on_unimplemented(
	message = "`{Self}` can't blend, so this field can't animate",
	label = "not `Mix`"
)]
pub trait Mix: PartialEq + Clone {
	fn mix(&self, to: &Self, t: f32) -> Self;
}
impl Mix for f32 {
	fn mix(&self, to: &Self, t: f32) -> Self {
		self + (to - self) * t
	}
}
impl Mix for Vec2F {
	fn mix(&self, to: &Self, t: f32) -> Self {
		Vec2::from(*self).lerp(Vec2::from(*to), t).into()
	}
}
impl Mix for Vec3F {
	fn mix(&self, to: &Self, t: f32) -> Self {
		Vec3::from(*self).lerp(Vec3::from(*to), t).into()
	}
}
impl Mix for QuatF {
	fn mix(&self, to: &Self, t: f32) -> Self {
		Quat::from(*self).slerp(Quat::from(*to), t).into()
	}
}
impl Mix for Transform {
	fn mix(&self, to: &Self, t: f32) -> Self {
		Transform {
			translation: self.translation.mix(&to.translation, t),
			rotation: self.rotation.mix(&to.rotation, t),
			scale: self.scale.mix(&to.scale, t),
		}
	}
}
impl Mix for Color {
	fn mix(&self, to: &Self, t: f32) -> Self {
		let mut c = *self;
		c.c.r = self.c.r.mix(&to.c.r, t);
		c.c.g = self.c.g.mix(&to.c.g, t);
		c.c.b = self.c.b.mix(&to.c.b, t);
		c.a = self.a.mix(&to.a, t);
		c
	}
}

#[diagnostic::on_unimplemented(message = "`{Self}` needs `#[derive(Animatable)]`")]
pub trait Animatable: Sized {
	/// one optional curve per field, what `.animation(|a| ..)` builds
	type Animation: Default + Send + Sync + 'static;
	/// one optional value and curve per field, what `.appear(|a| ..)` and `.vanish(|a| ..)` build
	type Transition: Default + Send + Sync + 'static;
	type Tracks: Default + Send + Sync + 'static;
	/// `self` is the fresh target and `old` is what's on screen, this blends `self` in place
	fn blend(
		&mut self,
		old: &Self,
		animation: &Self::Animation,
		tracks: &mut Self::Tracks,
		now: Option<Timestamp>,
	);
	/// moves every field the transition covers to where it starts, taking the start values out of it
	fn appear_from(&mut self, transition: &mut Self::Transition);
	/// blends every field the transition covers from where it starts, taking the start values out of it
	fn arrive(
		&mut self,
		transition: &mut Self::Transition,
		animation: &Self::Animation,
		tracks: &mut Self::Tracks,
		now: Option<Timestamp>,
	);
	/// moves every field the transition covers to where it ends
	fn vanish_to(&mut self, transition: &Self::Transition);
	fn departure(transition: &Self::Transition) -> Self::Animation;
	fn moving(tracks: &Self::Tracks) -> bool;
}

/// `CustomElement` for elements that `#[derive(Animatable)]`, the derive implements `CustomElement` from this
pub trait AnimatedElement<State: ValidState>:
	Animatable + Any + Debug + Send + Sync + Sized + 'static
{
	type Inner: Send + Sync + 'static;
	type Error: std::error::Error + Send + Sync + 'static;
	fn create_inner(
		&self,
		asteroids_context: &Context,
		info: CreateInnerInfo,
	) -> impl Future<Output = Result<Self::Inner, Self::Error>> + Send + Sync;
	fn diff(&self, old_self: &Self, context: &Context, inner: &mut Self::Inner);
	fn frame(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		_state: &mut State,
		_inner: &mut Self::Inner,
	) {
	}
}

pub(crate) fn seconds_between(from: Timestamp, to: Timestamp) -> f32 {
	(to.seconds - from.seconds) as f32 + (to.nanoseconds - from.nanoseconds) as f32 / 1e9
}

#[doc(hidden)]
pub mod __private {
	use super::*;
	pub use stardust_xr_fusion::{client::FrameInfo, types::Timestamp};
	use std::marker::PhantomData;

	pub struct Track<T> {
		to: T,
		/// when the target last moved
		start: Option<Timestamp>,
		/// what's left of the delay when the target moved
		wait: f32,
		curve: Curve,
		motion: Motion<T>,
	}
	enum Motion<T> {
		Eased {
			from: T,
		},
		// the value one substep before what's shown, so the velocity is `shown - prev`
		Spring {
			prev: T,
			last: Option<Timestamp>,
			carry: f32,
		},
	}

	const SUBSTEP: f32 = 1.0 / 240.0;

	pub fn step<T: Mix>(
		v: &mut T,
		old: &T,
		curve: Option<&Curve>,
		track: &mut Option<Track<T>>,
		now: Option<Timestamp>,
	) {
		let moved = match track {
			Some(t) => *v != t.to,
			None => *v != *old,
		};
		if moved {
			let prev = track.take();
			*track = curve.map(|c| {
				let wait = match &prev {
					None => c.delay,
					Some(p) => match (p.start, now) {
						(Some(s), Some(now)) => (p.wait - seconds_between(s, now)).max(0.0),
						_ => p.wait,
					},
				};
				let motion = match (&c.shape, prev) {
					// a spring keeps its momentum when the target moves, so chasing one stays smooth
					(Shape::Spring { .. }, Some(p))
						if matches!(p.motion, Motion::Spring { .. }) =>
					{
						p.motion
					}
					(Shape::Spring { .. }, _) => Motion::Spring {
						prev: old.clone(),
						last: None,
						carry: 0.0,
					},
					_ => Motion::Eased { from: old.clone() },
				};
				Track {
					to: v.clone(),
					start: None,
					wait,
					curve: c.clone(),
					motion,
				}
			});
		}
		let Some(t) = track else {
			return;
		};
		let elapsed = match now {
			Some(now) => seconds_between(*t.start.get_or_insert(now), now) - t.wait,
			None => -t.wait,
		};
		if elapsed < 0.0 {
			*v = old.clone();
			if let Motion::Spring { last, .. } = &mut t.motion {
				*last = None;
			}
			return;
		}
		let Some(p) = t.curve.shape.progress(elapsed) else {
			*track = None;
			return;
		};
		match (&mut t.motion, &t.curve.shape) {
			(Motion::Spring { prev, last, carry }, &Shape::Spring { duration, bounce }) => {
				let mut x = old.clone();
				if let Some(now) = now {
					*carry += seconds_between(*last.get_or_insert(now), now).min(0.25);
					*last = Some(now);
				}
				let w = std::f32::consts::TAU / duration;
				let zeta = 1.0 - bounce.clamp(0.0, 0.99);
				let damping = (-2.0 * zeta * w * SUBSTEP).exp();
				let pull = (w * SUBSTEP).powi(2);
				while *carry >= SUBSTEP {
					let next = prev.mix(&x, 1.0 + damping).mix(&t.to, pull);
					*prev = std::mem::replace(&mut x, next);
					*carry -= SUBSTEP;
				}
				*v = x;
			}
			(Motion::Eased { from }, _) => *v = from.mix(&t.to, p),
			(Motion::Spring { .. }, _) => {
				unreachable!("spring motion only comes from spring curves")
			}
		}
	}

	pub struct AnimatedInner<I, T> {
		pub inner: I,
		pub tracks: T,
	}

	// autoref picks `ViaMix` when the field is `Mix` and falls through to the no-op otherwise
	pub struct Probe<T>(PhantomData<T>);
	impl<T> Probe<T> {
		#[allow(clippy::new_without_default)]
		pub fn new() -> Self {
			Probe(PhantomData)
		}
	}
	pub trait ViaMix<T> {
		fn step(
			&self,
			v: &mut T,
			old: &T,
			curve: Option<&Curve>,
			track: &mut Option<Track<T>>,
			now: Option<Timestamp>,
		);
		fn assign(&self, v: &mut T, to: &Option<T>);
	}
	impl<T: Mix> ViaMix<T> for Probe<T> {
		fn step(
			&self,
			v: &mut T,
			old: &T,
			curve: Option<&Curve>,
			track: &mut Option<Track<T>>,
			now: Option<Timestamp>,
		) {
			step(v, old, curve, track, now)
		}
		fn assign(&self, v: &mut T, to: &Option<T>) {
			if let Some(to) = to {
				*v = to.clone();
			}
		}
	}
	pub trait ViaOther<T> {
		fn step(
			&self,
			_v: &mut T,
			_old: &T,
			_curve: Option<&Curve>,
			_track: &mut Option<Track<T>>,
			_now: Option<Timestamp>,
		) {
		}
		fn assign(&self, _v: &mut T, _to: &Option<T>) {}
	}
	impl<T> ViaOther<T> for &Probe<T> {}
}

#[cfg(test)]
mod tests {
	use super::{__private::*, *};

	fn at(s: f32) -> Option<Timestamp> {
		Some(Timestamp {
			seconds: 0,
			nanoseconds: (s * 1e9) as i64,
		})
	}

	#[test]
	fn blends_to_target() {
		let c = linear(1.0);
		let mut track = None;

		let mut v = 1.0;
		step(&mut v, &0.0, Some(&c), &mut track, at(0.0));
		assert_eq!(v, 0.0);

		let mut v = 1.0;
		step(&mut v, &0.0, Some(&c), &mut track, at(0.5));
		assert_eq!(v, 0.5);

		let mut v = 1.0;
		step(&mut v, &0.5, Some(&c), &mut track, at(1.0));
		assert_eq!(v, 1.0);
		assert!(track.is_none());
	}

	#[test]
	fn retargets_from_shown() {
		let c = linear(1.0);
		let mut track = None;
		let mut v = 1.0;
		step(&mut v, &0.0, Some(&c), &mut track, at(0.0));
		let mut v = 1.0;
		step(&mut v, &0.0, Some(&c), &mut track, at(0.5));

		let mut back = 0.0;
		step(&mut back, &v, Some(&c), &mut track, at(0.5));
		assert_eq!(back, 0.5);
		let mut back = 0.0;
		step(&mut back, &0.5, Some(&c), &mut track, at(1.0));
		assert_eq!(back, 0.25);
	}

	#[test]
	fn snaps_without_curve() {
		let c = linear(1.0);
		let mut track = None;
		let mut v = 1.0;
		step(&mut v, &0.0, Some(&c), &mut track, at(0.0));

		let mut v = 2.0;
		step(&mut v, &0.0, None, &mut track, at(0.5));
		assert_eq!(v, 2.0);
		assert!(track.is_none());
	}

	#[test]
	fn curves_start_at_zero_and_end() {
		for c in [
			linear(1.0),
			ease_in(1.0),
			ease_out(1.0),
			ease_in_out(1.0),
			spring(1.0, 0.0),
			spring(1.0, 0.5),
			curve(|t: f32| (t < 1.0).then_some(t)),
			linear(1.0).delay(0.5),
		] {
			assert_eq!(c.progress(0.0), Some(0.0));
			assert_eq!(c.progress(100.0), None);
		}
		assert_eq!(ease_in_out(1.0).progress(0.5), Some(0.5));
	}

	#[test]
	fn spring_overshoots_only_when_bouncy() {
		let peak = |c: Curve| {
			(0..1000)
				.filter_map(|i| c.progress(i as f32 / 100.0))
				.fold(0.0, f32::max)
		};
		assert!(peak(spring(1.0, 0.0)) <= 1.0);
		assert!(peak(spring(1.0, 0.5)) > 1.1);
	}

	#[test]
	fn appears_from_start_value() {
		#[derive(Debug, Animatable)]
		struct Dot {
			x: f32,
			y: f32,
			z: f32,
		}
		let appear = || {
			DotTransition::default()
				.x(0.0, linear(1.0))
				.y(2.0, linear(1.0))
		};
		let animation = DotAnimation::default().z(linear(1.0));
		let dot = || Dot {
			x: 1.0,
			y: 1.0,
			z: 1.0,
		};
		let mut decl = dot();
		decl.appear_from(&mut appear());
		assert_eq!((decl.x, decl.y, decl.z), (0.0, 2.0, 1.0));

		let mut tracks = Default::default();
		let mut new = dot();
		new.arrive(&mut appear(), &animation, &mut tracks, at(0.0));
		assert_eq!((new.x, new.y, new.z), (0.0, 2.0, 1.0));
		let shown = new;
		let mut new = dot();
		new.blend(&shown, &animation, &mut tracks, at(0.5));
		assert_eq!((new.x, new.y, new.z), (0.5, 1.5, 1.0));
	}

	#[test]
	fn vanishes_to_end_value() {
		#[derive(Debug, Clone, Animatable)]
		struct Dot {
			x: f32,
			y: f32,
		}
		let vanish = DotTransition::default().x(0.0, linear(1.0));
		let mut target = Dot { x: 1.0, y: 1.0 };
		target.vanish_to(&vanish);
		let curves = Dot::departure(&vanish);
		let mut tracks = Default::default();

		let mut shown = Dot { x: 1.0, y: 1.0 };
		for (t, x, moving) in [(0.0, 1.0, true), (0.5, 0.5, true), (1.0, 0.0, false)] {
			let mut next = target.clone();
			next.blend(&shown, &curves, &mut tracks, at(t));
			assert_eq!((next.x, next.y), (x, 1.0));
			assert_eq!(Dot::moving(&tracks), moving);
			shown = next;
		}
	}

	#[test]
	fn spring_settles_on_target() {
		let c = spring(0.3, 0.0);
		let mut track = None;
		let mut shown = 0.0;
		for i in 0..=180 {
			let mut v = 1.0;
			step(&mut v, &shown, Some(&c), &mut track, at(i as f32 / 90.0));
			shown = v;
		}
		assert_eq!(shown, 1.0);
		assert!(track.is_none());
	}

	#[test]
	fn spring_chases_a_moving_target() {
		let c = spring(0.3, 0.0);
		let mut track = None;
		let mut shown = 0.0;
		let mut eased = 0.0;
		let mut eased_track = None;
		for i in 0..=90 {
			let t = i as f32 / 90.0;
			let mut v = t;
			step(&mut v, &shown, Some(&c), &mut track, at(t));
			shown = v;
			let mut v = t;
			step(
				&mut v,
				&eased,
				Some(&ease_in_out(1.0)),
				&mut eased_track,
				at(t),
			);
			eased = v;
		}
		// a target that moves every update restarts a duration curve, so it never leaves the start
		assert_eq!(eased, 0.0);
		assert!((0.8..1.0).contains(&shown), "{shown}");
	}

	#[test]
	fn delays_from_rest_but_not_when_chasing() {
		let c = linear(1.0).delay(0.5);
		let mut track = None;
		let mut shown = 0.0;
		for (t, want) in [(0.0, 0.0), (0.25, 0.0), (0.5, 0.0), (1.0, 0.5)] {
			let mut v = 1.0;
			step(&mut v, &shown, Some(&c), &mut track, at(t));
			assert_eq!(v, want, "at {t}");
			shown = v;
		}

		let mut v = 2.0;
		step(&mut v, &shown, Some(&c), &mut track, at(1.0));
		let mut v = 2.0;
		step(&mut v, &0.5, Some(&c), &mut track, at(1.5));
		assert_eq!(v, 1.25);
	}

	#[test]
	fn delayed_spring_waits_then_moves() {
		let c = spring(0.3, 0.0).delay(0.5);
		let mut track = None;
		let mut shown = 0.0;
		for i in 0..=45 {
			let mut v = 1.0;
			step(&mut v, &shown, Some(&c), &mut track, at(i as f32 / 90.0));
			shown = v;
		}
		assert_eq!(shown, 0.0);
		for i in 46..=270 {
			let mut v = 1.0;
			step(&mut v, &shown, Some(&c), &mut track, at(i as f32 / 90.0));
			shown = v;
		}
		assert_eq!(shown, 1.0);
		assert!(track.is_none());
	}

	#[test]
	fn derive_handles_tuple_structs() {
		#[derive(Debug, Animatable)]
		struct Pair(f32, #[no_animate] f32, String);
		let pair = |v: f32| Pair(v, v, v.to_string());
		let animation = PairAnimation::default()._0(linear(1.0));
		let mut tracks = Default::default();

		let mut new = pair(1.0);
		new.blend(&pair(0.0), &animation, &mut tracks, at(0.0));
		let shown = new;
		let mut new = pair(1.0);
		new.blend(&shown, &animation, &mut tracks, at(0.5));
		assert_eq!(new.0, 0.5);
		assert_eq!(new.1, 1.0);
		assert_eq!(new.2, "1");
	}

	#[test]
	fn derive_skips_no_animate() {
		#[derive(Debug, Animatable)]
		struct Thing<T: Debug + Send + Sync + 'static> {
			x: f32,
			#[no_animate]
			y: f32,
			name: String,
			t: std::marker::PhantomData<T>,
		}
		let thing = |v: f32| Thing::<u8> {
			x: v,
			y: v,
			name: v.to_string(),
			t: std::marker::PhantomData,
		};
		let animation = ThingAnimation::default().x(linear(1.0));
		let mut tracks = Default::default();

		let mut new = thing(1.0);
		new.blend(&thing(0.0), &animation, &mut tracks, at(0.0));
		let shown = new;
		let mut new = thing(1.0);
		new.blend(&shown, &animation, &mut tracks, at(0.5));
		assert_eq!(new.x, 0.5);
		assert_eq!(new.y, 1.0);
		assert_eq!(new.name, "1");
	}
}
