use bevy::prelude::*;
use bevy_inspector_egui::quick::WorldInspectorPlugin;
use clap::Parser;
use bevy_renet::renet::transport::NetcodeTransportError;
use std::net::{IpAddr, Ipv4Addr};

mod client;
mod server;
use cavetown::*;
pub use client::*;
pub use server::*;

const DEFAULT_PORT:u16 = 5001;

#[derive(Parser, PartialEq, Resource)]
enum Cli {
    SinglePlayer,
    Server {
        #[arg(short, long, default_value_t = DEFAULT_PORT)]
        port: u16,
    },
    Client {
        #[arg(short, long, default_value_t = Ipv4Addr::LOCALHOST.into())]
        ip: IpAddr,

        #[arg(short, long, default_value_t = DEFAULT_PORT)]
        port: u16,
    },
}

impl Default for Cli {
    fn default() -> Self {
        Self::parse()
    }
}

fn main() {
    let mut app = App::new();
    app.insert_resource(Lobby::default());

    let cli = Cli::default();

    match cli {
        Cli::SinglePlayer => {
            app.add_plugins((DefaultPlugins, WorldInspectorPlugin::new()));
            add_server(&mut app, DEFAULT_PORT);
            add_client(&mut app, Ipv4Addr::LOCALHOST.into(), DEFAULT_PORT);
        }
        Cli::Server { port } => {
            app.add_plugins((MinimalPlugins,));
            add_server(&mut app, port);
        }
        Cli::Client { port, ip } => {
            app.add_plugins((DefaultPlugins, WorldInspectorPlugin::new()));
            add_client(&mut app, ip, port);
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
