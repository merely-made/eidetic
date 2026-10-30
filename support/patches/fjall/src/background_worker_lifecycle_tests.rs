use super::*;
use std::sync::{atomic::{AtomicBool, Ordering}, mpsc};
use std::time::Duration;

struct HeldActivity {
    entered: Option<mpsc::Sender<()>>,
    release: Option<mpsc::Receiver<()>>,
    _lease: Arc<()>,
}
impl Activity for HeldActivity {
    fn name(&self) -> &'static str { "lifecycle-test" }
    fn run(&mut self) -> crate::Result<()> { Ok(()) }
}
impl Drop for HeldActivity {
    fn drop(&mut self) {
        if let Some(entered) = self.entered.take() {
            entered.send(()).unwrap();
            self.release.take().unwrap().recv_timeout(Duration::from_secs(10)).unwrap();
        }
    }
}
fn worker(counter: Arc<AtomicUsize>, lease: Arc<()>, entered: Option<mpsc::Sender<()>>, release: Option<mpsc::Receiver<()>>) -> BackgroundWorker<HeldActivity> {
    BackgroundWorker::new(HeldActivity { entered, release, _lease: lease }, PoisonDart::new("lifecycle-test", Arc::new(AtomicBool::new(false))), counter, StopSignal::default())
}

#[test]
fn registered_before_thread_start_and_released_if_spawn_fails() {
    let counter = Arc::new(AtomicUsize::new(0));
    let lease = Arc::new(());
    let pending = worker(counter.clone(), lease.clone(), None, None);
    // The captured worker is held at the pre-start barrier, as in a spawned
    // thread that has not yet been scheduled.
    assert_eq!(counter.load(Ordering::Acquire), 1);
    assert_eq!(Arc::strong_count(&lease), 2);
    // Dropping the closure on spawn failure must unregister exactly once.
    let closure = move || pending.start();
    drop(closure);
    assert_eq!(counter.load(Ordering::Acquire), 0);
    assert_eq!(Arc::strong_count(&lease), 1);
}

#[test]
fn registered_until_activity_and_its_lease_are_dropped() {
    let counter = Arc::new(AtomicUsize::new(0));
    let lease = Arc::new(());
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let pending = worker(counter.clone(), lease.clone(), Some(entered_tx), Some(release_rx));
    pending.stop_signal.send();
    let thread = std::thread::spawn(move || pending.start());
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let held_leases = Arc::strong_count(&lease);
    let held_count = counter.load(Ordering::Acquire);
    release_tx.send(()).unwrap();
    thread.join().unwrap();
    assert_eq!(held_leases, 2);
    assert_eq!(held_count, 1);
    assert_eq!(counter.load(Ordering::Acquire), 0);
    assert_eq!(Arc::strong_count(&lease), 1);
}
