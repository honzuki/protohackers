use std::{
    collections::HashSet,
    sync::atomic::{self, AtomicU64},
};

use crate::{
    jobs::PermissionDeniedErr,
    request::{Request, Response},
    SharedJobManager,
};

static NEW_CLIENT_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct Client {
    id: u64,
    // list of jobs the client is currently working on
    jobs: HashSet<u64>,
    job_manager: SharedJobManager,
}

impl Client {
    pub fn new(job_manager: SharedJobManager) -> Client {
        Self {
            id: NEW_CLIENT_ID.fetch_add(1, atomic::Ordering::SeqCst),
            jobs: HashSet::default(),
            job_manager,
        }
    }

    pub async fn handle_request(&mut self, request: &str) -> Response {
        let Ok(request) = serde_json::from_str(request) else {
            return Response::error("failed to parse request".into());
        };

        match request {
            Request::Put {
                queue,
                job,
                priority,
            } => {
                let job_id = self.job_manager.lock().unwrap().add(queue, job, priority);
                Response::created(job_id)
            }
            Request::Delete { id } => match self.job_manager.lock().unwrap().remove(id) {
                true => Response::ok(),
                false => Response::NoJob,
            },
            Request::Abort { id } => match self.job_manager.lock().unwrap().abort(self.id, id) {
                Ok(true) => {
                    self.jobs.remove(&id);
                    Response::ok()
                }
                Ok(false) => Response::NoJob,
                Err(PermissionDeniedErr) => {
                    Response::error("you can only abort jobs you're currently working on".into())
                }
            },
            Request::Get { queues, wait } => match wait {
                true => {
                    let fut = self.job_manager.lock().unwrap().get(self.id, &queues);
                    let job = fut.await;
                    self.jobs.insert(job.id());
                    job.into()
                }
                false => match self.job_manager.lock().unwrap().try_get(self.id, &queues) {
                    Some(job) => {
                        self.jobs.insert(job.id());
                        job.into()
                    }
                    None => Response::NoJob,
                },
            },
        }
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // abort all active jobs
        let mut job_manager = self.job_manager.lock().unwrap();
        for job_id in self.jobs.iter() {
            let _ = job_manager.abort(self.id, *job_id);
        }
    }
}
