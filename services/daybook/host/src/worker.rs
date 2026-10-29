//! One thread does the talking to the services, one call after another (so two quick changes reach the service in the
//! order they were made), and the answer comes back as a message for the program's own thread to give to the waiting
//! callback.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{Sender, channel};

use daybook_net::NetError;

use crate::remote::{Op, Remote, Reply};

/// What the worker thread sends back: the request's number and its answer, to be given to [`Worker::complete`].
pub type Answer = Box<dyn Any + Send>;

type Callback = Box<dyn FnOnce(Answer)>;
type Job = (u64, Box<dyn FnOnce() -> Answer + Send>);

pub struct Worker {
    jobs: Sender<Job>,
    waiting: RefCell<HashMap<u64, Callback>>,
    next: Cell<u64>,
}

impl Worker {
    /// `answers` is called on the worker thread with each result; it must get it to the program's thread and there to
    /// [`complete`](Self::complete).
    #[must_use]
    pub fn new(answers: impl Fn(u64, Answer) + Send + 'static) -> Rc<Self> {
        let (jobs, queue) = channel::<Job>();
        std::thread::spawn(move || {
            for (id, work) in queue {
                answers(id, work());
            }
        });
        Rc::new(Self {
            jobs,
            waiting: RefCell::default(),
            next: Cell::new(0),
        })
    }

    /// Runs one operation and gives its result to `then` on the program's thread.
    pub fn call(
        &self,
        remote: &Arc<Remote>,
        op: Op,
        then: impl FnOnce(Result<Reply, NetError>) + 'static,
    ) {
        let remote = Arc::clone(remote);
        self.enqueue(move || remote.execute(op), then);
    }

    /// Runs `work` in its turn and gives its result to `then` on the program's thread.
    pub fn enqueue<T: Send + 'static>(
        &self,
        work: impl FnOnce() -> T + Send + 'static,
        then: impl FnOnce(T) + 'static,
    ) {
        self.next.set(self.next.get() + 1);
        let id = self.next.get();
        self.waiting.borrow_mut().insert(
            id,
            Box::new(move |answer: Answer| {
                if let Ok(result) = answer.downcast::<T>() {
                    then(*result);
                }
            }),
        );
        let _ = self.jobs.send((id, Box::new(move || Box::new(work()))));
    }

    /// The worker's answer to request `id`.
    pub fn complete(&self, id: u64, answer: Answer) {
        let then = self.waiting.borrow_mut().remove(&id);
        if let Some(then) = then {
            then(answer);
        }
    }
}
