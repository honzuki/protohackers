use std::{collections::HashMap, sync::Arc};

use dashmap::DashSet;
use tokio::sync::mpsc;

use super::{ticket::Ticket, CameraPosition, Limit, Plate, Road, Timestamp};

const DAY_IN_SECS: u32 = 86400;

// Since the system submits it work into subsystems,
// there is no need for a big buffer
const SYSTEM_BUFFER_SIZE: usize = 64;

// since each road get its own worker
// we don't need a particularly big buffer
const WORKER_BUFFER_SIZE: usize = 64;

type SharedTicketRecords = Arc<DashSet<(Plate, u32)>>;

#[derive(Debug)]
enum InternalMessage {
    RegisterCamera(Road, Limit),
    SubmitRecord(Road, CameraPosition, Plate, Timestamp),
}

pub struct System {
    workers: HashMap<Road, RoadWorkerHandler>,
    ticket_system: super::ticket::Handler,
    ticket_records: SharedTicketRecords,
}

impl System {
    /// Starts a new ticket system
    ///
    /// returns an handler that can be used to control the system
    ///
    /// note: this function needs to be called from inside a tokio runtime context
    pub fn start(ticket_system: super::ticket::Handler) -> Handler {
        let (tx, mut rx) = mpsc::channel(SYSTEM_BUFFER_SIZE);

        let mut this = Self {
            workers: HashMap::default(),
            ticket_system,
            ticket_records: Arc::default(),
        };
        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                match message {
                    InternalMessage::RegisterCamera(road, limit) => {
                        this.register_camera(road, limit).await
                    }
                    InternalMessage::SubmitRecord(road, camera, plate, timestamp) => {
                        this.submit_record(road, camera, plate, timestamp).await
                    }
                }
            }
        });

        Handler { sender: tx }
    }

    async fn register_camera(&mut self, road: Road, limit: Limit) {
        self.workers.entry(road).or_insert_with(|| {
            RoadWorker::start(
                road,
                limit,
                self.ticket_system.clone(),
                self.ticket_records.clone(),
            )
        });
    }

    async fn submit_record(
        &mut self,
        road: Road,
        camera: CameraPosition,
        plate: Plate,
        timestamp: Timestamp,
    ) {
        let road_worker = self
            .workers
            .get_mut(&road)
            .expect("a camera must be registered to submit a report");

        road_worker
            .submit_plate_report(plate, camera, timestamp)
            .await;
    }
}

#[derive(Debug, Clone)]
pub struct Handler {
    sender: mpsc::Sender<InternalMessage>,
}

impl Handler {
    /// Register as a camera and convert the handler
    /// into an handler that can submit plate reports
    pub async fn register_camera(self, road: Road, limit: Limit) -> CameraHandler {
        self.sender
            .send(InternalMessage::RegisterCamera(road, limit))
            .await
            .expect("the system should live as long as the handler live");

        CameraHandler {
            sender: self.sender,
            road,
        }
    }
}

pub struct CameraHandler {
    sender: mpsc::Sender<InternalMessage>,
    road: Road,
}

impl CameraHandler {
    pub async fn submit_record(
        &mut self,
        camera: CameraPosition,
        plate: Plate,
        timestamp: Timestamp,
    ) {
        self.sender
            .send(InternalMessage::SubmitRecord(
                self.road, camera, plate, timestamp,
            ))
            .await
            .expect("the system should live as long as the handler live");
    }
}

// Road worker
enum InternalWorkerMessage {
    PlateReport(Plate, CameraPosition, Timestamp),
}

struct RoadWorker {
    records: HashMap<Plate, HashMap<CameraPosition, Timestamp>>,
    road: Road,
    speed_limit: Limit,
    ticket_handler: super::ticket::Handler,
    ticket_records: SharedTicketRecords,
}

impl RoadWorker {
    // Starts a new background road worker on a specific road
    fn start(
        road: Road,
        speed_limit: Limit,
        ticket_handler: super::ticket::Handler,
        ticket_records: SharedTicketRecords,
    ) -> RoadWorkerHandler {
        let (tx, mut rx) = mpsc::channel(WORKER_BUFFER_SIZE);

        let mut this = Self {
            records: HashMap::new(),
            road,
            speed_limit,
            ticket_handler,
            ticket_records,
        };
        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                match message {
                    InternalWorkerMessage::PlateReport(plate, camera, timestamp) => {
                        this.record(plate, camera, timestamp).await
                    }
                }
            }
        });

        RoadWorkerHandler { sender: tx }
    }

    async fn record(&mut self, plate: Plate, camera: CameraPosition, timetsamp: Timestamp) {
        // Insert the new record to the system
        let records = self.records.entry(plate.clone()).or_default();
        records.insert(camera, timetsamp);

        // Check the new record against the existing records to find speed limit violations
        'record_loop: for (entry_camera, entry_timestamp) in records {
            let distance = entry_camera.abs_diff(camera);
            let time: f64 = entry_timestamp.abs_diff(timetsamp) as f64 / 60f64 / 60f64; // convert secs to hours
            if time == 0.0 || distance == 0 {
                continue;
            }

            let Ok(speed) = ((distance as f64 / time).round() as u64).try_into() else {
                // we are guarnteed that no drive can reach a speed limit high enough for this to fail
                return;
            };

            if speed > self.speed_limit {
                let start = (timetsamp, camera).min((*entry_timestamp, *entry_camera));
                let end = (timetsamp, camera).max((*entry_timestamp, *entry_camera));

                let ticket = Ticket::new(
                    plate.clone(),
                    self.road,
                    start.1,
                    start.0,
                    end.1,
                    end.0,
                    speed,
                );

                for day in (start.0 / DAY_IN_SECS)..=(end.0 / DAY_IN_SECS) {
                    if self.ticket_records.get(&(plate.clone(), day)).is_some() {
                        continue 'record_loop;
                    }
                }

                for day in (start.0 / DAY_IN_SECS)..=(end.0 / DAY_IN_SECS) {
                    self.ticket_records.insert((plate.clone(), day));
                }

                self.ticket_handler.submit_ticket(ticket.clone()).await;
            }
        }
    }
}

#[derive(Debug, Clone)]
struct RoadWorkerHandler {
    sender: mpsc::Sender<InternalWorkerMessage>,
}

impl RoadWorkerHandler {
    async fn submit_plate_report(
        &mut self,
        plate: Plate,
        camera: CameraPosition,
        timestamp: Timestamp,
    ) {
        self.sender
            .send(InternalWorkerMessage::PlateReport(plate, camera, timestamp))
            .await
            .expect("the road worker should live as long as the handlers live")
    }
}
