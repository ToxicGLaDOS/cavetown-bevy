use bevy::prelude::*;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use serde::{Deserialize, Serialize};
use clap::Parser;
use local_ip_address::local_ip;
use bevy_renet::renet::*;
use bevy_renet::*;
use bevy_renet::renet::transport::{ClientAuthentication, NetcodeClientTransport, NetcodeTransportError};
use bevy_renet::transport::NetcodeServerPlugin;
use bevy_renet::{
    client_connected,
    renet::{ClientId, RenetClient, ServerEvent},
    RenetClientPlugin,
};
use std::str::FromStr;
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, UdpSocket},
    time::SystemTime,
};

const PORT:u16 = 5001;
// Magic number that must match between client and server
const PROTOCOL_ID:u64 = 1000;

#[derive(Component)]
struct Player {
    speed: f32,
    client_id: ClientId
}

#[derive(Parser, PartialEq, Resource)]
enum Cli {
    SinglePlayer,
    Server {
        #[arg(short, long, default_value_t = PORT)]
        port: u16,
    },
    Client {
        #[arg(short, long, default_value_t = Ipv4Addr::LOCALHOST.into())]
        ip: IpAddr,

        #[arg(short, long, default_value_t = PORT)]
        port: u16,
    },
}

impl Default for Cli {
    fn default() -> Self {
        Self::parse()
    }
}

#[derive(Debug, Default, Resource)]
pub struct Lobby {
    pub players: HashMap<ClientId, Entity>,
}

#[derive(Bundle)]
struct PlayerBundle {
    player: Player,

    // We can nest/include another bundle.
    // Add the components for a standard Bevy Sprite:
    sprite: SpriteSheetBundle,
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
}

fn spawn_player(commands: &mut Commands,
    asset_server: &Res<AssetServer>,
    texture_atlases: &mut ResMut<Assets<TextureAtlas>>,
    client_id: ClientId
    ) -> Entity {
    let img_path ="character.png".to_string();

    let texture_handle = asset_server.load(&img_path);
    let texture_atlas = TextureAtlas::from_grid(
        texture_handle,
        Vec2::new(64.0, 64.0), 1, 4, Some(Vec2::new(0.0, 0.0)), Some(Vec2::new(0.0, 0.0)));

    let texture_atlas_handle = texture_atlases.add(texture_atlas);

    let player_entity = commands.spawn(
        PlayerBundle {
            player: Player {
                speed: 400.0,
                client_id
            },
            sprite: SpriteSheetBundle {
                sprite: TextureAtlasSprite{
                    index : 0,
                      ..default()
                },
                texture_atlas: texture_atlas_handle.clone(), 
                ..default()
            }
        }
    ).id();

    player_entity
}

fn player_input(keys: Res<Input<KeyCode>>, mut client: ResMut<RenetClient>) {
    let mut direction = Vec3::new(0.0, 0.0, 0.0);

    if keys.pressed(KeyCode::W) {
        direction.y += 1.0;
        
    } else if keys.pressed(KeyCode::S) {
        direction.y -= 1.0;
    }

    if keys.pressed(KeyCode::A) {
        direction.x -= 1.0;
    } else if keys.pressed(KeyCode::D) {
        direction.x += 1.0;
    }

    let move_message = bincode::serialize(&PlayerCommand::PlayerMove { direction: (direction) }).unwrap();
    client.send_message(DefaultChannel::ReliableOrdered, move_message);
}

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
            app.add_systems(Update, player_input.run_if(client_connected()));
    }
}

fn server_events(mut commands: Commands,
    mut lobby: ResMut<Lobby>,
    mut events: EventReader<ServerEvent>,
    asset_server: Res<AssetServer>,
    mut texture_atlases: ResMut<Assets<TextureAtlas>>,
    mut server: ResMut<RenetServer>,
    query: Query<(&Player, &Transform)>,
    maybe_my_client_id: Option<Res<CurrentClientId>>) {
    for event in events.read() {
        match event {
            ServerEvent::ClientConnected{client_id} => {
                let player_entity = spawn_player(&mut commands, &asset_server, &mut texture_atlases, *client_id);

                lobby.players.insert(*client_id, player_entity);
                let mut spawn_players_map: HashMap<ClientId, Vec3> = HashMap::new();

                // Add the player we just created
                spawn_players_map.insert(*client_id, Vec3::ZERO);

                // Add the players that already existed
                for (player, transform) in query.iter() {
                    spawn_players_map.insert(player.client_id, transform.translation);
                }
                match maybe_my_client_id {
                    Some(ref my_client_id) => {
                        if *client_id != my_client_id.0 {
                            let init_message = bincode::serialize(&InitCommand::SpawnPlayers { players: spawn_players_map }).unwrap();
                            server.send_message(*client_id, DefaultChannel::ReliableOrdered, init_message);
                        }
                    },
                    None => {
                        let init_message = bincode::serialize(&InitCommand::SpawnPlayers { players: spawn_players_map }).unwrap();
                        server.send_message(*client_id, DefaultChannel::ReliableOrdered, init_message);
                    }
                }

                info!("Connected {}!", client_id);

            },
            ServerEvent::ClientDisconnected{client_id, reason: _} => info!("Disconnected {}!", client_id),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Component, Event)]
pub enum InitCommand {
    SpawnPlayers {players: HashMap<ClientId, Vec3>}
}

#[derive(Debug, Serialize, Deserialize, Component, Event)]
pub enum PlayerCommand {
    PlayerMove { direction: Vec3 }
}

fn client_initalize(mut commands: Commands, mut client: ResMut<RenetClient>,
    mut lobby: ResMut<Lobby>,
    asset_server: Res<AssetServer>,
    mut texture_atlases: ResMut<Assets<TextureAtlas>>,) {
    while let Some(message) = client.receive_message(DefaultChannel::ReliableOrdered) {
        let command: InitCommand = bincode::deserialize(&message).unwrap();
        match command {
            InitCommand::SpawnPlayers { players } => {
                for (client_id, _translation) in players.iter() {
                    let player_entity = spawn_player(&mut commands, &asset_server, &mut texture_atlases, *client_id);

                    info!("Inserting player {}", client_id);
                    lobby.players.insert(*client_id, player_entity);
                }
            }
        }
    }
}

#[derive(Debug, Resource)]
struct CurrentClientId(ClientId);

fn server_update_system(mut server: ResMut<RenetServer>,
    mut query: Query<(&mut Transform, &Player)>,
    time: Res<Time>) {
    for client_id in server.clients_id() {
        while let Some(message) = server.receive_message(client_id, DefaultChannel::ReliableOrdered) {
            let command: PlayerCommand = bincode::deserialize(&message).unwrap();
            match command {
                PlayerCommand::PlayerMove{direction} => {
                    for (mut transform, player) in query.iter_mut() {
                        if player.client_id == client_id {
                            if let Some(normalized) = direction.try_normalize() {
                                transform.translation += normalized * player.speed * time.delta_seconds();
                            }
                        }
                    } 
                },
            }
        }
    } 
}

// If client_id exists on the server then we're a client/server combo
// because CurrentClientId only exists on a client and this function
// is only called on the server
fn server_sync_players(mut server: ResMut<RenetServer>,
    query: Query<(&Transform, &Player)>,
    maybe_my_client_id: Option<Res<CurrentClientId>>) {
    let mut player_positions: HashMap<ClientId, Vec3> = HashMap::new();

    for (transform, player) in query.iter() {
        player_positions.insert(player.client_id, transform.translation);
    }

    let sync_message = bincode::serialize(&player_positions).unwrap();
    match maybe_my_client_id {
        // client/server combo
        Some(my_client_id) => {
            server.broadcast_message_except(my_client_id.0, DefaultChannel::Unreliable, sync_message);
        }
        // headless server (NOT IMPLEMENTED)
        None => {
            server.broadcast_message(DefaultChannel::Unreliable, sync_message);
        }
    }
}

fn client_sync_system(mut commands: Commands, mut client: ResMut<RenetClient>, lobby: Res<Lobby>) {
    while let Some(message) = client.receive_message(DefaultChannel::Unreliable) {
        let players: HashMap<ClientId, Vec3> = bincode::deserialize(&message).unwrap();
        for (player_id, translation) in players.iter() {
            if let Some(player_entity) = lobby.players.get(player_id) {
                let transform = Transform {
                    translation: *translation,
                    ..Default::default()
                };
                commands.entity(*player_entity).insert(transform);
            }
        }
    }
}

fn add_server(app: &mut App, port: u16) {
    use bevy_renet::renet::transport::{NetcodeServerTransport, ServerAuthentication, ServerConfig};
    app.add_plugins(RenetServerPlugin);
    app.add_plugins(NetcodeServerPlugin);

    let server = RenetServer::new(ConnectionConfig::default());

    let public_addr = format!("0.0.0.0:{}", port).parse().unwrap();
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

fn add_client(app: &mut App, ip: IpAddr, port: u16) {
    app.add_plugins(RenetClientPlugin);
    app.add_plugins(bevy_renet::transport::NetcodeClientPlugin);
    app.add_plugins(PlayerPlugin);

    let client = RenetClient::new(ConnectionConfig::default());

    let server_addr = format!("{}:{}", ip, port).parse().unwrap();
    let local_bind_address = format!("{}:0", local_ip().unwrap());
    info!("connecting to {}:{}", ip, port);
    let socket = UdpSocket::bind(local_bind_address).unwrap();
    let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
    let client_id = current_time.as_millis() as u64;
    let authentication = ClientAuthentication::Unsecure {
        client_id,
        protocol_id: PROTOCOL_ID,
        server_addr,
        user_data: None,
    };

    let transport = NetcodeClientTransport::new(current_time, authentication, socket).unwrap();

    app.insert_resource(client);
    app.insert_resource(transport);
    app.insert_resource(CurrentClientId(ClientId::from_raw(client_id)));

    app.add_event::<PlayerCommand>();
    app.add_systems(Update, (client_sync_system, client_initalize).run_if(client_connected()));
}

fn main() {
    let mut app = App::new();
    app.add_plugins((DefaultPlugins, WorldInspectorPlugin::new()));
    app.add_systems(Startup, setup);
    app.insert_resource(Lobby::default());

    let cli = Cli::default();

    match cli {
        Cli::SinglePlayer => {
            info!("Singleplayer")
        }
        Cli::Server { port } => {
            add_server(&mut app, port);
            add_client(&mut app, local_ip().unwrap(), port)
        }
        Cli::Client { port, ip } => {
            add_client(&mut app, ip, port)
        }
    }

    // If any error is found we just panic
    fn panic_on_error_system(mut renet_error: EventReader<NetcodeTransportError>) {
        for e in renet_error.read() {
            panic!("{}", e);
        }
    }
    app.add_systems(Update, panic_on_error_system);
    
    app.run();
}
