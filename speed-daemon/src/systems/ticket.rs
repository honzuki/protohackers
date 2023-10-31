use std::collections::HashMap;

use tokio::sync::mpsc;

use crate::protocol::message::ToClient;

use super::Road;

// Since this system is mostly used by internal systems,
// we want to provide a big enough buffer that wouldn't stuck
// other systems from doing their own work
const SYSTEM_BUFFER_SIZE: usize = 1024;

pub type DispatcherSender = mpsc::Sender<ToClient>;

#[derive(Debug, Clone)]
pub struct Ticket {
    plate: String,
    road: u16,
    mile1: u16,
    timestamp1: u32,
    mile2: u16,
    timestamp2: u32,
    speed: u16,
}

impl Ticket {
    pub fn new(
        plate: String,
        road: u16,
        mile1: u16,
        timestamp1: u32,
        mile2: u16,
        timestamp2: u32,
        speed: u16,
    ) -> Self {
        Self {
            plate,
            road,
            mile1,
            timestamp1,
            mile2,
            timestamp2,
            speed,
        }
    }
}

impl From<Ticket> for ToClient {
    fn from(ticket: Ticket) -> Self {
        Self::ticket(
            ticket.plate,
            ticket.road,
            (ticket.mile1, ticket.timestamp1),
            (ticket.mile2, ticket.timestamp2),
            ticket.speed,
        )
    }
}

// Used for communication between the handler and the system
enum InternalMessage {
    SubmitTicket(Ticket),
    RegisterDispatcher(Vec<Road>, DispatcherSender),
}

pub struct System {
    dispatchers: HashMap<Road, Vec<DispatcherSender>>,
    pending_tickets: HashMap<Road, Vec<Ticket>>,
}

impl System {
    /// Starts a new ticket system
    ///
    /// returns an handler that can be used to control the system
    ///
    /// note: this function needs to be called from inside a tokio runtime context
    pub fn start() -> Handler {
        let (tx, mut rx) = mpsc::channel(SYSTEM_BUFFER_SIZE);

        let mut this = Self {
            dispatchers: HashMap::default(),
            pending_tickets: HashMap::default(),
        };
        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                match message {
                    InternalMessage::RegisterDispatcher(roads, drx) => {
                        this.register_dispatcher(roads, drx).await
                    }
                    InternalMessage::SubmitTicket(ticket) => this.submit_ticket(ticket).await,
                }
            }
        });

        Handler { sender: tx }
    }

    async fn register_dispatcher(&mut self, roads: Vec<Road>, drx: DispatcherSender) {
        // register the dispatcher in the system
        for &road in roads.iter() {
            self.dispatchers.entry(road).or_default().push(drx.clone());
        }

        // check if there are any pending tickets that the dispatcher can accept
        for road in roads {
            if let Some(tickets) = self.pending_tickets.remove(&road) {
                for ticket in tickets {
                    // if the 'send' fails it means that the dispatcher just disconnected
                    // in which case we can simply discard these tickets
                    if drx.send(ticket.into()).await.is_err() {
                        return;
                    }
                }
            }
        }
    }

    async fn submit_ticket(&mut self, ticket: Ticket) {
        // try to submit the ticket to any of the registered dispatchers
        if let Some(dispatchers) = self.dispatchers.get(&ticket.road) {
            for dispatcher in dispatchers {
                if dispatcher.send(ticket.clone().into()).await.is_ok() {
                    return; // successfully submitted the ticket
                }
            }
        }

        // failed to submit the ticket, add it to a pending queue
        self.pending_tickets
            .entry(ticket.road)
            .or_default()
            .push(ticket);
    }
}

#[derive(Debug, Clone)]
pub struct Handler {
    sender: mpsc::Sender<InternalMessage>,
}

impl Handler {
    pub async fn submit_ticket(&mut self, ticket: Ticket) {
        self.sender
            .send(InternalMessage::SubmitTicket(ticket))
            .await
            .expect("the system should live as long as the handler does");
    }

    pub async fn register_dispatcher(
        &mut self,
        roads: Vec<Road>,
        dispatcher_channel: DispatcherSender,
    ) {
        self.sender
            .send(InternalMessage::RegisterDispatcher(
                roads,
                dispatcher_channel,
            ))
            .await
            .expect("the system should live as long as the handler does");
    }
}
