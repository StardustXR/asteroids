use crate::{
	Context, ValidState,
	custom::{ElementTrait, FnWrapper},
};
use derive_where::derive_where;
use futures_util::StreamExt;
use inotify::{EventMask, Inotify, WatchMask};
use stardust_xr_fusion::spatial::SpatialRef;
use std::{
	path::{Path, PathBuf},
	sync::{
		Arc,
		atomic::{AtomicBool, Ordering},
	},
};
use tokio::task::AbortHandle;

pub struct FileWatcherInner {
	spatial: SpatialRef,
	watch_loop: AbortHandle,
	modified: Arc<AtomicBool>,
}
impl Drop for FileWatcherInner {
	fn drop(&mut self) {
		self.watch_loop.abort();
	}
}

#[derive_where(Debug)]
pub struct FileWatcher<State: ValidState> {
	file_path: PathBuf,
	on_change: FnWrapper<dyn Fn(&mut State) + Send + Sync>,
}
impl<State: ValidState> FileWatcher<State> {
	pub fn new<F: Fn(&mut State) + Send + Sync + 'static>(
		file_path: PathBuf,
		on_change: F,
	) -> Self {
		FileWatcher {
			file_path,
			on_change: FnWrapper(Box::new(on_change)),
		}
	}

	async fn watch_loop(file_path: PathBuf, modified: Arc<AtomicBool>) -> std::io::Result<()> {
		let inotify = Inotify::init()?;
		let _watcher = inotify.watches().add(file_path, WatchMask::MODIFY)?;
		let mut event_stream = inotify.into_event_stream([0; 1024])?;

		while let Some(Ok(event)) = event_stream.next().await {
			if event.mask.contains(EventMask::MODIFY) {
				modified.store(true, Ordering::Relaxed);
			}
		}

		Ok(())
	}
}
// TODO: make one watch_loop as a resource to only have one Inotify instance
impl<State: ValidState> ElementTrait<State> for FileWatcher<State> {
	type Inner = FileWatcherInner;
	type Resource = ();
	type Error = std::io::Error;

	fn create_inner(
		&self,
		parent_space: &SpatialRef,
		_context: &Context,
		_path: &Path,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		let modified = Arc::new(AtomicBool::new(false));
		let watch_loop =
			tokio::spawn(Self::watch_loop(self.file_path.clone(), modified.clone())).abort_handle();

		Ok(FileWatcherInner {
			spatial: parent_space.clone(),
			watch_loop,
			modified,
		})
	}

	fn update(
		&self,
		old_decl: &Self,
		state: &mut State,
		inner: &mut Self::Inner,
		_resource: &mut Self::Resource,
	) {
		if old_decl.file_path != self.file_path {
			inner.watch_loop.abort();
			inner.modified.store(false, Ordering::Relaxed);
			inner.watch_loop = tokio::spawn(Self::watch_loop(
				self.file_path.clone(),
				inner.modified.clone(),
			))
			.abort_handle();
		}

		if inner.modified.load(Ordering::Relaxed) {
			inner.modified.store(false, Ordering::Relaxed);
			(self.on_change.0)(state);
		}
	}

	fn spatial_aspect(&self, inner: &Self::Inner) -> SpatialRef {
		inner.spatial.clone()
	}
}
