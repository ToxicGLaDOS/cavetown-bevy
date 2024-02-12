use bevy::prelude::*;
use bevy_renet::renet::ClientId;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// Magic number that must match between client and server
pub const PROTOCOL_ID:u64 = 1000;
#[derive(Debug, Resource)]
pub struct CurrentClientId(pub ClientId);

#[derive(Debug, Default, Resource)]
pub struct Lobby {
    pub players: HashMap<ClientId, Entity>,
}

#[derive(Debug, Serialize, Deserialize, Event)]
pub enum InitCommand {
    PlayerConnected {
        client_id: ClientId,
        position: Vec3
    }
}

#[derive(Debug, Serialize, Deserialize, Event)]
pub enum PlayerCommand {
    PlayerMove { direction: Vec3 }
}
