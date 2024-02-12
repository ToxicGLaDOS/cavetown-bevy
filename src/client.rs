use bevy::prelude::*;
use bevy_renet::renet::*;
use bevy_renet::renet::transport::{ClientAuthentication, NetcodeClientTransport};
use bevy_renet::{
    client_connected,
    renet::{ClientId, RenetClient},
    RenetClientPlugin,
};
use std::{
    collections::HashMap,
    net::{IpAddr, UdpSocket, Ipv4Addr},
    time::SystemTime
};
use local_ip_address::local_ip;

use cavetown::*;

pub struct PlayerPlugin;

#[derive(Bundle)]
pub struct ClientPlayerBundle {
    sprite: SpriteSheetBundle,
    name: Name
}

enum AssetLocation {
    OverworldImage
}

impl AssetLocation {
    fn as_str(&self) -> &'static str {
        match self {
            AssetLocation::OverworldImage => "overworld.png"
        }
    }
}

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
            app.add_systems(Update, player_input.run_if(client_connected()));
    }
}

// System that sets up everything that isn't dependent on
// being connected to the server (happens before that).
fn setup(mut commands: Commands, asset_server: Res<AssetServer>) {
    let font = asset_server.load("fonts/DejaVuSans.ttf");
    let text_style = TextStyle {
        font: font.clone(),
        font_size: 60.0,
        color: Color::WHITE,
    };
    commands.spawn(
        NodeBundle {
            style: Style {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Start,
                ..default()
            },
            ..default() 
        }
    ).with_children(|parent| {
        parent.spawn(
            TextBundle::from_section(
                "overworld",
                text_style,
            ).with_style(
                Style {
                    top: Val::Px(5.0),
                    ..default()
                })
            );
    });
    commands.spawn(Camera2dBundle::default());
    commands.spawn(SpriteBundle {
        texture: asset_server.load(AssetLocation::OverworldImage.as_str()),
        ..Default::default()
    });
}

// Processes player input and sends messages to the server
// based on those messages.
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

// Gets messages from the server related to client initialization
// and acts accordingly
//
// For now this just spawns the player bundles for players when they connect.
// That includes spawning the player in a single player session when they connect
// to the server.
pub fn client_initalize(mut commands: Commands, mut client: ResMut<RenetClient>,
    mut lobby: ResMut<Lobby>,
    asset_server: Res<AssetServer>,
    mut texture_atlases: ResMut<Assets<TextureAtlas>>,) {
    while let Some(message) = client.receive_message(DefaultChannel::ReliableOrdered) {
        let command: InitCommand = bincode::deserialize(&message).unwrap();
        match command {
            InitCommand::PlayerConnected { client_id, position } => {
                let player_entity = spawn_player(&mut commands, &asset_server, &mut texture_atlases, client_id, position);

                info!("Inserting player {}", client_id);
                lobby.players.insert(client_id, player_entity);
            }
        }
    }
}

// Syncs the movement of all the clients connected to the server
// 
// We receive messages from the server and move the corresponding
// entity based on the message we recieve.
// This includes the player in a single player session.
pub fn client_sync_system(mut commands: Commands, mut client: ResMut<RenetClient>, lobby: Res<Lobby>) {
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

// Helper function to spawn a player.
pub fn spawn_player(commands: &mut Commands,
    asset_server: &Res<AssetServer>,
    texture_atlases: &mut ResMut<Assets<TextureAtlas>>,
    client_id: ClientId,
    position: Vec3
    ) -> Entity {
    let img_path ="character.png".to_string();

    let texture_handle = asset_server.load(&img_path);
    let texture_atlas = TextureAtlas::from_grid(
        texture_handle,
        Vec2::new(64.0, 64.0), 1, 4, Some(Vec2::new(0.0, 0.0)), Some(Vec2::new(0.0, 0.0)));

    let texture_atlas_handle = texture_atlases.add(texture_atlas);

    let transform = Transform::from_translation(position);
    
    let player_entity = commands.spawn(
        ClientPlayerBundle {
            sprite: SpriteSheetBundle {
                transform,
                sprite: TextureAtlasSprite{
                    index : 0,
                      ..default()
                },
                texture_atlas: texture_atlas_handle.clone(),
                ..default()
            },
            name: Name::new(format!("Player {}", client_id))
        }
    ).id();

    player_entity
}


// Adds the client systems/plugins/resources/etc. to the app
pub fn add_client(app: &mut App, ip: IpAddr, port: u16) {
    app.add_plugins(RenetClientPlugin);
    app.add_plugins(bevy_renet::transport::NetcodeClientPlugin);
    app.add_systems(Startup, setup);
    app.add_plugins(PlayerPlugin);

    let client = RenetClient::new(ConnectionConfig::default());

    let server_addr = format!("{}:{}", ip, port).parse().unwrap();
    let local_bind_address;

    if ip.is_loopback() {
        local_bind_address = format!("{}:0", Ipv4Addr::LOCALHOST);
    } else {
        local_bind_address = format!("{}:0", local_ip().unwrap());
    }
    info!("binding to {}", local_bind_address);
    info!("connecting to {}:{}", ip, port);
    let socket = UdpSocket::bind(local_bind_address).unwrap();
    let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
    let client_id = current_time.as_millis() as u64;
    info!("Client id: {}", client_id);
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
