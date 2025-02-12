use psst_core::{
    audio_normalize::NormalizationLevel,
    audio_output::AudioOutput,
    audio_player::{PlaybackConfig, PlaybackItem, Player, PlayerCommand, PlayerEvent},
    cache::{Cache, CacheHandle},
    cdn::{Cdn, CdnHandle},
    connection::Credentials,
    error::Error,
    item_id::{ItemId, ItemIdType},
    session::{SessionConfig, SessionService},
};
use std::{env, io, io::BufRead, path::PathBuf, thread};

fn main() {
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    let track_id = args
        .get(1)
        .expect("Expected <track_id> in the first parameter");
    let login_creds = Credentials::from_username_and_password(
        env::var("SPOTIFY_USERNAME").unwrap(),
        env::var("SPOTIFY_PASSWORD").unwrap(),
    );
    let session = SessionService::with_config(SessionConfig {
        login_creds,
        proxy_url: None,
    });

    start(track_id, session).unwrap();
}

fn start(track_id: &str, session: SessionService) -> Result<(), Error> {
    let cdn = Cdn::new(session.clone(), None)?;
    let cache = Cache::new(PathBuf::from("cache"))?;
    let item_id = ItemId::from_base62(track_id, ItemIdType::Track).unwrap();
    play_item(
        session,
        cdn,
        cache,
        PlaybackItem {
            item_id,
            norm_level: NormalizationLevel::Track,
        },
    )
}

fn play_item(
    session: SessionService,
    cdn: CdnHandle,
    cache: CacheHandle,
    item: PlaybackItem,
) -> Result<(), Error> {
    let output = AudioOutput::open()?;
    let output_remote = output.remote();
    let config = PlaybackConfig::default();

    let mut player = Player::new(session, cdn, cache, config, output.remote());

    let output_thread = thread::spawn({
        let player_source = player.audio_source();
        move || {
            output
                .start_playback(player_source)
                .expect("Playback failed");
        }
    });

    let _ui_thread = thread::spawn({
        let player_sender = player.event_sender();

        player_sender
            .send(PlayerEvent::Command(PlayerCommand::LoadQueue {
                items: vec![item, item, item],
                position: 0,
            }))
            .unwrap();

        move || {
            for line in io::stdin().lock().lines() {
                match line.as_ref().map(|s| s.as_str()) {
                    Ok("p") => {
                        player_sender
                            .send(PlayerEvent::Command(PlayerCommand::Pause))
                            .unwrap();
                    }
                    Ok("r") => {
                        player_sender
                            .send(PlayerEvent::Command(PlayerCommand::Resume))
                            .unwrap();
                    }
                    Ok("s") => {
                        player_sender
                            .send(PlayerEvent::Command(PlayerCommand::Stop))
                            .unwrap();
                    }
                    Ok("<") => {
                        player_sender
                            .send(PlayerEvent::Command(PlayerCommand::Previous))
                            .unwrap();
                    }
                    Ok(">") => {
                        player_sender
                            .send(PlayerEvent::Command(PlayerCommand::Next))
                            .unwrap();
                    }
                    _ => log::warn!("unknown command"),
                }
            }
        }
    });

    for event in player.event_receiver() {
        player.handle(event);
    }
    output_remote.close();
    output_thread.join().unwrap();

    Ok(())
}
