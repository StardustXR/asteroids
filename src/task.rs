use crate::ValidState;
use std::{future::Future, marker::PhantomData, sync::mpsc};

pub type FinishedTaskCallback<State> = Box<dyn FnOnce(&mut State) + Send>;

pub trait Tasker<State: ValidState>: Clone + Send + Sync + 'static {
	/// Direct spawn from element frame() methods. Requires explicit State generic.
	fn spawn<
		T: Send + 'static,
		Fut: Future<Output = T> + Send + 'static,
		CB: FnOnce(&mut State, T) + Send + 'static,
	>(
		&self,
		future: Fut,
		callback: CB,
	);

	fn spawn_detached<O: Send + 'static, Fut: Future<Output = O> + Send + 'static>(
		&self,
		future: Fut,
	) {
		tokio::spawn(future);
	}

	#[allow(private_interfaces)]
	fn map<
		MappedState: ValidState,
		Mapper: Fn(&mut State) -> Option<&mut MappedState> + Clone + Send + Sync + 'static,
	>(
		self,
		mapper: Mapper,
	) -> MappedTasker<State, MappedState, Self, Mapper> {
		MappedTasker {
			wrapped: self,
			mapper,
			phantom_state: PhantomData,
			phantom_mapped_state: PhantomData,
		}
	}
}

/// Non-generic channel sender. Lives on Context.
pub struct RootTasker<State: ValidState>(pub mpsc::Sender<FinishedTaskCallback<State>>);
impl<State: ValidState> Clone for RootTasker<State> {
	fn clone(&self) -> Self {
		Self(self.0.clone())
	}
}
impl<State: ValidState> Tasker<State> for RootTasker<State> {
	/// Direct spawn from element frame() methods. Requires explicit State generic.
	fn spawn<
		T: Send + 'static,
		Fut: Future<Output = T> + Send + 'static,
		CB: FnOnce(&mut State, T) + Send + 'static,
	>(
		&self,
		future: Fut,
		callback: CB,
	) {
		let tx = self.0.clone();
		tokio::spawn(async move {
			let result = future.await;
			let boxed: Box<dyn FnOnce(&mut State) + Send> =
				Box::new(move |state| callback(state, result));
			let _ = tx.send(boxed);
		});
	}
}

pub(crate) struct MappedTasker<
	State: ValidState,
	MappedState: ValidState,
	WrappedTasker: Tasker<State>,
	Mapper: Fn(&mut State) -> Option<&mut MappedState> + Clone + Send + Sync + 'static,
> {
	wrapped: WrappedTasker,
	mapper: Mapper,
	phantom_state: PhantomData<State>,
	phantom_mapped_state: PhantomData<MappedState>,
}
impl<
	State: ValidState,
	MappedState: ValidState,
	WrappedTasker: Tasker<State>,
	Mapper: Fn(&mut State) -> Option<&mut MappedState> + Clone + Send + Sync + 'static,
> Clone for MappedTasker<State, MappedState, WrappedTasker, Mapper>
{
	fn clone(&self) -> Self {
		Self {
			wrapped: self.wrapped.clone(),
			mapper: self.mapper.clone(),
			phantom_state: PhantomData,
			phantom_mapped_state: PhantomData,
		}
	}
}

impl<
	State: ValidState,
	MappedState: ValidState,
	WrappedTasker: Tasker<State>,
	Mapper: Fn(&mut State) -> Option<&mut MappedState> + Clone + Send + Sync + 'static,
> Tasker<MappedState> for MappedTasker<State, MappedState, WrappedTasker, Mapper>
{
	fn spawn<
		T: Send + 'static,
		Fut: Future<Output = T> + Send + 'static,
		CB: FnOnce(&mut MappedState, T) + Send + 'static,
	>(
		&self,
		future: Fut,
		callback: CB,
	) {
		let mapper = self.mapper.clone();
		self.wrapped.spawn(future, move |state, t| {
			if let Some(mapped_state) = (mapper)(state) {
				(callback)(mapped_state, t)
			}
		});
	}
}
