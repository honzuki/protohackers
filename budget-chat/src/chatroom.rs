use std::collections::HashMap;

use tokio::sync::{mpsc, oneshot};

use crate::protocol::*;

// Used to manage a chat room
#[derive(Debug, Clone)]
pub struct ChatRoom {
    sender: mpsc::Sender<ToChatRoomMessage>,
}

pub struct ChatRoomRegistered {
    sender: mpsc::Sender<ToChatRoomMessage>,
    username: String,
}

#[derive(thiserror::Error, Debug)]
pub enum ChatRoomError {
    #[error("{0}")]
    Mpsc(#[from] mpsc::error::SendError<ToChatRoomMessage>),

    #[error("{0}")]
    Oneshot(#[from] oneshot::error::RecvError),

    #[error("{0}")]
    Join(#[from] JoinError),
}

impl ChatRoom {
    // Creates a new chat room and returns an handler that can be used to register new users
    pub fn create() -> Self {
        let (tx, mut rx) = mpsc::channel(MESSAGE_BUFFER_COUNT);

        tokio::spawn(async move {
            let mut users = UserManager::default();

            while let Some(message) = rx.recv().await {
                match message {
                    // A new user attempts to join the chat room
                    ToChatRoomMessage::Join(Join { username, response }) => {
                        match users.add_user(username.clone()) {
                            Ok(rx) => {
                                // User was added successfully
                                users
                                    .emit_message_to_all(
                                        &username,
                                        FromChatRoomMessage::Join(username.clone()),
                                    )
                                    .await;
                                let _ = response.send(Ok(JoinSuccess {
                                    userlist: users
                                        .get_user_list()
                                        .into_iter()
                                        // filter the current user from the list
                                        .filter(|current_username| current_username != &username)
                                        .collect(),
                                    rx,
                                }));
                            }
                            Err(_) => {
                                // Username is already in use
                                let _ = response.send(Err(JoinError::BadUsername(username)));
                            }
                        }
                    }

                    // A user has disconnected
                    ToChatRoomMessage::Leave(Leave { username }) => {
                        users.remove_user(&username);
                        users
                            .emit_message_to_all(
                                &username,
                                FromChatRoomMessage::Leave(username.clone()),
                            )
                            .await
                    }

                    // A user has sent a message
                    ToChatRoomMessage::ChatMessage(ChatMessage { from, text }) => {
                        users
                            .emit_message_to_all(
                                &from,
                                FromChatRoomMessage::ChatMessage(from.clone(), text),
                            )
                            .await
                    }
                };
            }
        });

        Self { sender: tx }
    }

    // Tries to register a new user
    //
    // on success, returnes a chat handler that can be used to send messages
    pub async fn register(
        self,
        username: String,
    ) -> Result<(ChatRoomRegistered, JoinSuccess), ChatRoomError> {
        let (tx, rx) = oneshot::channel();

        self.sender
            .send(ToChatRoomMessage::Join(Join {
                username: username.clone(),
                response: tx,
            }))
            .await?;

        let join_success = rx.await??;

        Ok((ChatRoomRegistered::new(self.sender, username), join_success))
    }
}

impl ChatRoomRegistered {
    fn new(sender: mpsc::Sender<ToChatRoomMessage>, username: String) -> Self {
        Self { sender, username }
    }

    pub async fn send_message(&self, message: String) -> Result<(), ChatRoomError> {
        self.sender
            .send(ToChatRoomMessage::ChatMessage(ChatMessage {
                from: self.username.clone(),
                text: message,
            }))
            .await?;

        Ok(())
    }

    // Leaves the chat room
    //
    // on success, returns an handler that can be used to register new users
    pub async fn leave(self) -> Result<ChatRoom, ChatRoomError> {
        self.sender
            .send(ToChatRoomMessage::Leave(Leave {
                username: self.username,
            }))
            .await?;

        Ok(ChatRoom {
            sender: self.sender,
        })
    }
}

#[derive(Debug)]
struct User {
    sender: mpsc::Sender<FromChatRoomMessage>,
}

#[derive(Debug, Default)]
struct UserManager {
    users: HashMap<String, User>,
}

impl UserManager {
    /// Tries to add a user
    ///
    /// returns an error if the username of the user is already in use
    /// otherwise returns a receiver the user's task can use to receive messages
    fn add_user(&mut self, username: String) -> Result<FromChatRoom, ()> {
        if self.users.get(&username).is_some() {
            return Err(());
        }

        let (tx, rx) = mpsc::channel(MESSAGE_BUFFER_COUNT);
        self.users.insert(username.clone(), User { sender: tx });

        Ok(FromChatRoom { receiver: rx })
    }

    fn remove_user(&mut self, username: &str) {
        self.users.remove(username);
    }

    // Emits a message to all connected users except for the originator
    async fn emit_message_to_all(&self, originator: &str, message: FromChatRoomMessage) {
        for (username, user) in self.users.iter() {
            if username != originator {
                if let Err(err) = user.sender.send(message.clone()).await {
                    eprintln!("failed to emit a message to: {}\n{:?}", username, err);
                }
            }
        }
    }

    fn get_user_list(&self) -> Vec<String> {
        self.users.keys().cloned().collect()
    }
}
