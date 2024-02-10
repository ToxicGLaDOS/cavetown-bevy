use bevy::prelude::*;
use bevy_renet::renet::*;
use bevy_renet::*;
use bevy_renet::transport::NetcodeServerPlugin;
use bevy_renet::{
    renet::ClientId,
    renet::ServerEvent
};
use std::{
    collections::HashMap,
    net::UdpSocket,
    time::SystemTime
};

use cavetown::*;

#[derive(Bundle)]
pub struct ServerPlayerBundle {
    player: Player,
    input: PlayerInput,
    transform: Transform,
    name: Name
}

#[derive(Component)]
pub struct Player {
    pub speed: f32,
    pub client_id: ClientId,
    pub name: Name
}

#[derive(Debug, Component)]
pub struct PlayerInput {
    direction: Vec2
}

// Manages server level events like client connections, disconnections
pub fn server_events(mut commands: Commands,
    mut lobby: ResMut<Lobby>,
    mut events: EventReader<ServerEvent>,
    mut server: ResMut<RenetServer>,
    query: Query<(&Player, &Transform)>) {
    for event in events.read() {
        match event {
            ServerEvent::ClientConnected{client_id} => {
                println!("Connected {}!", client_id);

                let starting_position = Transform::default();
                let player_entity = commands.spawn((
                    starting_position,
                    Player {
                        speed: 400.0,
                        client_id: *client_id,
                        name: Name::new("Server player")
                    },
                    PlayerInput {
                        direction: Vec2::new(0.0, 0.0)
                    }
                    )
                ).id();

                lobby.players.insert(*client_id, player_entity);
                let mut spawn_players_map: HashMap<ClientId, Vec3> = HashMap::new();

                // Add the player we just created
                spawn_players_map.insert(*client_id, Vec3::ZERO);

                // Send the message about the player that just connected
                // to the player that just connected
                let connected_player_message = bincode::serialize(&InitCommand::PlayerConnected { client_id: *client_id, position: starting_position.translation }).unwrap();
                server.send_message(*client_id, DefaultChannel::ReliableOrdered, connected_player_message);



                for (player, transform) in query.iter() {
                    // Send the message about existing player to the newly joined player
                    let existing_player_message = bincode::serialize(&InitCommand::PlayerConnected { client_id: player.client_id, position: transform.translation }).unwrap();
                    server.send_message(*client_id, DefaultChannel::ReliableOrdered, existing_player_message);

                    // Send message about new player to existing player
                    let new_player_message = bincode::serialize(&InitCommand::PlayerConnected { client_id: *client_id, position: starting_position.translation }).unwrap();
                    server.send_message(player.client_id, DefaultChannel::ReliableOrdered, new_player_message);
                }
            },
            ServerEvent::ClientDisconnected{client_id, reason: _} => println!("Disconnected {}!", client_id),
        }
    }
}


// Recieves all the messages from the clients every game update
pub fn server_update_system(mut server: ResMut<RenetServer>,
    mut query: Query<(&mut Transform, &Player, &mut PlayerInput)>,
    time: Res<Time>) {
    for (mut transform, player, mut input) in query.iter_mut() {
        while let Some(message) = server.receive_message(player.client_id, DefaultChannel::ReliableOrdered) {
            let command: PlayerCommand = bincode::deserialize(&message).unwrap();
            match command {
                PlayerCommand::PlayerMove{direction} => {
                    if let Some(normalized) = direction.try_normalize() {
                        input.direction.x = normalized.x;
                        input.direction.y = normalized.y;
                    // Happens when direction is close to 0 or non-finite
                    } else {
                        input.direction.x = 0.0;
                        input.direction.y = 0.0;
                    }
                },
            }
        }
        transform.translation.x += input.direction.x * player.speed * time.delta_seconds();
        transform.translation.y += input.direction.y * player.speed * time.delta_seconds();
    }
}


pub fn server_sync_players(mut server: ResMut<RenetServer>,
    query: Query<(&Transform, &Player)>) {
    let mut player_positions: HashMap<ClientId, Vec3> = HashMap::new();

    for (transform, player) in query.iter() {
        player_positions.insert(player.client_id, transform.translation);
    }

    let sync_message = bincode::serialize(&player_positions).unwrap();
    server.broadcast_message(DefaultChannel::Unreliable, sync_message);
}

// Adds the server systems/plugins/resources/etc. to the app
pub fn add_server(app: &mut App, port: u16) {
    use bevy_renet::renet::transport::{NetcodeServerTransport, ServerAuthentication, ServerConfig};
    app.add_plugins(RenetServerPlugin);
    app.add_plugins(NetcodeServerPlugin);

    let server = RenetServer::new(ConnectionConfig::default());

    let public_addr = format!("0.0.0.0:{}", port).parse().unwrap();
    println!("Listening on {}", public_addr);
    let socket = UdpSocket::bind(public_addr).unwrap();
    let current_time: std::time::Duration = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
    let server_config = ServerConfig {
        current_time,
        max_clients: 64,
        protocol_id: PROTOCOL_ID,
        public_addresses: vec![public_addr],
        authentication: ServerAuthentication::Unsecure,
    };

    let transport = NetcodeServerTransport::new(server_config, socket).unwrap();
    app.add_event::<PlayerCommand>();
    app.insert_resource(server);
    app.insert_resource(transport);

    app.add_systems(Update, (server_events, server_update_system, server_sync_players));
}
