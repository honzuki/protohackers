use std::{
    collections::{BTreeSet, HashMap},
    future::Future,
    hash::Hash,
    pin::Pin,
    sync::{Arc, Mutex},
};

use tokio::sync::oneshot;

use crate::request::Response;

#[derive(Debug, Clone)]
pub struct Job {
    id: u64,
    queue: String,
    job: serde_json::Value,
    priority: u64,
    // the id of the client that is currently working on it
    owner: Option<u64>,
}

impl From<Job> for Response {
    fn from(value: Job) -> Self {
        Self::job(value.id, value.queue, value.job, value.priority)
    }
}

impl Job {
    pub fn id(&self) -> u64 {
        self.id
    }
}

type SharedJobSender = Arc<Mutex<Option<oneshot::Sender<Job>>>>;

// A stab for a queue structure in the state
// a queue can either have pending jobs or waiting clients
#[derive(Debug)]
enum QueueStab {
    // set of (priority, job_id)
    Jobs(BTreeSet<(u64, u64)>),

    // list of oneshot channels that contain a list of (waiting_client_id, oneshot::sender<job>)
    Clients(Vec<(u64, SharedJobSender)>),
}

#[derive(Debug, Default)]
pub struct Manager {
    // maps job_id -> Job
    jobs: HashMap<u64, Job>,
    new_job_id: u64,

    // Maps queue_name -> queue_stab
    queues: HashMap<String, QueueStab>,
}

pub struct PermissionDeniedErr;

impl Manager {
    /// Add a new job to the manager
    ///
    /// returns an id that can be used to identified the newly added job
    pub fn add(&mut self, queue: String, job: serde_json::Value, priority: u64) -> u64 {
        let id = self.new_job_id;
        self.new_job_id += 1;

        // create the job & push to queue
        self.jobs.insert(
            id,
            Job {
                id,
                queue: queue.clone(),
                job,
                priority,
                owner: None,
            },
        );
        self.add_job_to_queue(id, queue);

        id
    }

    /// Try to remove the highest priority job from a list of queues
    ///
    /// will return None if all the listed queues are empty.
    /// in a successfull call, the returned job structure is a stem
    /// structure that can be used to fetch information from the snapshot
    /// of the job at the time it was retrieved. this stemp will not get updated.
    pub fn try_get<T: AsRef<str> + Hash + Eq>(
        &mut self,
        requester_id: u64,
        queues: &[T],
    ) -> Option<Job> {
        let mut best_job: Option<(u64, u64)> = None;

        // for every queue of pending jobs in the list of requested queues
        for queue in queues {
            if let Some(QueueStab::Jobs(set)) = self.queues.get(queue.as_ref()) {
                // compare the job with the highest priority in this queue
                // to the best job we've found so far
                match (best_job, set.last()) {
                    (Some((best_priority, _)), Some((current_priority, current_job_id)))
                        if *current_priority > best_priority =>
                    {
                        // the current job is better than the best one so far
                        best_job = Some((*current_priority, *current_job_id));
                    }
                    (None, Some(item)) => {
                        // the first job we've found is also the best one so far
                        best_job = Some(*item);
                    }
                    _ => {}
                }
            }
        }

        best_job.map(|(_, job_id)| {
            // fetch the job and remove it from the queue
            let job = self
                .jobs
                .get_mut(&job_id)
                .expect("a job that was found in a queue must exist within the jobs map");

            if let QueueStab::Jobs(set) = self
                .queues
                .get_mut(&job.queue)
                .expect("a job must point back to the queue that contains it")
            {
                set.remove(&(job.priority, job.id));
            }

            // make sure to update the owner
            job.owner = Some(requester_id);

            // this clone will not be updated
            // and can only be used as a stem for fetching information from this snapshot of the job
            job.clone()
        })
    }

    /// Works the same way as `Self::try_get`,
    /// but instead of returning None, will return a future that will resolve once a job is available
    ///
    /// if the list of queues is empty, this function will sleep forever
    pub fn get<T: AsRef<str> + Hash + Eq>(
        &mut self,
        requester_id: u64,
        queues: &[T],
    ) -> Pin<Box<dyn Future<Output = Job> + 'static + Send>> {
        if queues.is_empty() {
            return Box::pin(std::future::pending());
        }

        // if there is an available job, return it
        if let Some(job) = self.try_get(requester_id, queues) {
            return Box::pin(async { job });
        }

        // no job is available, register to all requested queues, and wait for a new job
        let (tx, rx) = oneshot::channel();
        let sender = Arc::new(Mutex::new(Some(tx)));

        // for every requested queue
        for queue in queues {
            // fetch the queue or create a new waiting client list
            let queue = self
                .queues
                .entry(queue.as_ref().into())
                .or_insert(QueueStab::Clients(Vec::default()));

            if matches!(queue, QueueStab::Jobs(_)) {
                // the pending job list is empty, convert it to a waiting client list
                *queue = QueueStab::Clients(Vec::default());
            }

            if let QueueStab::Clients(list) = queue {
                list.push((requester_id, sender.clone()));
            }
        }

        Box::pin(async move {
            rx.await.expect(
                "the sender part of a waiting client should never be dropped before resolved",
            )
        })
    }

    /// Tries to removes a job from the manager
    ///
    /// return false if the job does not exist
    pub fn remove(&mut self, job_id: u64) -> bool {
        let Some(job) = self.jobs.remove(&job_id) else {
            return false;
        };

        if let Some(QueueStab::Jobs(set)) = self.queues.get_mut(&job.queue) {
            set.remove(&(job.priority, job.id));
        }

        true
    }

    /// Aborts an active job by putting it back on its queue
    ///
    /// can only abort jobs that are owned by the requester id,
    /// returns an error when the requester does not own the job.
    ///
    /// returns false when the job does not exist.
    pub fn abort(&mut self, requester_id: u64, job_id: u64) -> Result<bool, PermissionDeniedErr> {
        let Some(job) = self.jobs.get_mut(&job_id) else {
            return Ok(false);
        };

        if job.owner != Some(requester_id) {
            return Err(PermissionDeniedErr);
        }

        let queue = job.queue.clone();
        self.add_job_to_queue(job_id, queue);

        Ok(true)
    }

    fn add_job_to_queue(&mut self, job_id: u64, queue: String) {
        let Some(job) = self.jobs.get_mut(&job_id) else {
            // ignore jobs that don't exist
            return;
        };

        // fetch the queue, and create an empty pending jobs queue if necessary
        let queue = self
            .queues
            .entry(queue)
            .or_insert(QueueStab::Jobs(BTreeSet::default()));

        match queue {
            QueueStab::Clients(wait_list) => {
                // if the queue is a list of waiting clients, try to submit the job to one of the waiting clients
                while let Some((client, sender)) = wait_list.pop() {
                    // take ownership of the sender
                    let sender = sender.lock().unwrap().take();
                    if let Some(sender) = sender {
                        // we check that the receiver is open before sending to avoid wasteful clones of 'job'
                        if !sender.is_closed() && sender.send(job.clone()).is_ok() {
                            // successfully submitted the job, update the owner
                            job.owner = Some(client);
                            return;
                        }
                    }
                }
            }
            QueueStab::Jobs(set) => {
                set.insert((job.priority, job.id));
                return;
            }
        };

        // the waiting clients list is empty
        // we need to change it to a pending queue and insert the job
        let mut set = BTreeSet::new();
        set.insert((job.priority, job.id));
        *queue = QueueStab::Jobs(set);
    }
}
