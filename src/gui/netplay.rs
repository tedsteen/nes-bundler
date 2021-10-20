use std::{str::FromStr};
use libp2p::PeerId;

use crate::{GameRunnerState, MyGameState, NetPlayState, PlayState, network::{p2p::{GameState, P2P, P2PGame}}};

enum State {
    CreateGame(CreateGameState),
    StartGame(StartGameState)
}

struct StartGameState {
    game: OneShotReceiver<P2PGame>,
    current_state: tokio::sync::watch::Receiver<GameState>
}

impl StartGameState {
    fn create(p2p: P2P, name: &str, slot_count: u8) -> Self {
        let (sender, current_state) = tokio::sync::watch::channel(GameState::Initializing);
        let (game_sender, game_receiver) = tokio::sync::oneshot::channel();
        let name = name.to_owned();
        tokio::spawn(async move {
            let game = p2p.create_game(&name, slot_count).await;
            let p2p = p2p.clone();
            let _ = game_sender.send(game.clone());
            loop {
                if let Err(_) = sender.send(game.current_state(p2p.clone()).await) {
                    break; //No one is listening any longer.
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            println!("Broke out");
        });

        Self {
            game: OneShotReceiver::new(game_receiver),
            current_state
        }
    }

    fn join(p2p: P2P, owner_id: PeerId) -> Self {
        let (sender, current_state) = tokio::sync::watch::channel(GameState::Initializing);
        let (game_sender, game_receiver) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            let game = p2p.join_game(owner_id);
            let _ = game_sender.send(game.clone());
            loop {
                if let Err(_) = sender.send(game.current_state(p2p.clone()).await) {
                    break; //No one is listening any longer.
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            println!("Broke out");
        });

        Self {
            game: OneShotReceiver::new(game_receiver),
            current_state
        }
    }

    fn claim(&mut self, idx: usize) {
        if let Some(game) = self.game.get() {
            game.claim_slot(idx);
        }
    }
}

struct CreateGameState {
    //Create
    name: String,
    slot_count: u8,

    //Join
    search_string: String,
    join_id: String,
    search_result: Option<OneShotReceiver<Vec<(String, PeerId)>>>
}

impl CreateGameState {
    fn new() -> Self {
        Self {
            name: "test".to_owned(),
            slot_count: 2,
            search_string: "".to_owned(),
            join_id: "".to_owned(),
            search_result: None
        }
    }
    fn create(&self, p2p: P2P) -> State {
        State::StartGame(StartGameState::create(p2p, &self.name, self.slot_count))
    }

    fn join(&self, p2p: P2P) -> State {
        State::StartGame(StartGameState::join(p2p, PeerId::from_str(&self.join_id).unwrap()))
    }

    fn search(&mut self, p2p: P2P) {

        let (sender, receiver) = tokio::sync::oneshot::channel();
        let search_string = self.search_string.clone();
        tokio::spawn(async move {
            let r = p2p.find_games(&search_string).await;
            let _ = sender.send(r);
        });
        self.search_result = Some(OneShotReceiver::new(receiver));
    }
}

pub(crate) struct NetplayGui {
    state: State,
    p2p: P2P
}

impl NetplayGui {
    pub(crate) fn new(p2p: P2P) -> Self {        
        Self {
            state: State::CreateGame(CreateGameState::new()),
            p2p
        }
    }

    pub(crate) fn handle_event(&mut self, _: &winit::event::Event<'_, ()>) {
    }
    
    pub(crate) fn ui(&mut self, ctx: &egui::CtxRef, game_runner_state: &mut GameRunnerState) {
        egui::Window::new("Netplay!").collapsible(false).show(ctx, |ui| {
            match &mut self.state {
                State::CreateGame(create_state) => {
                    ui.add(egui::TextEdit::singleline(&mut create_state.search_string).hint_text("Search for room"));
                    if ui.button("Search").clicked() {
                        create_state.search(self.p2p.clone());
                    }
                    if let Some(result) = &mut create_state.search_result {
                        if let Some(r) = result.get() {
                            for (a, b) in r {
                                ui.label(format!("Result: {:?} = '{:?}'", a, b));
                                if ui.button("Copy id").clicked() {
                                    create_state.join_id = b.to_string();
                                }
                            }
                        }
                    }
                    ui.add(egui::TextEdit::singleline(&mut create_state.join_id).hint_text("Id of room to join"));
                    let join_btn = ui.button("Join");

                    ui.add(egui::TextEdit::singleline(&mut create_state.name).hint_text("Name your new room"));
                    ui.add(egui::DragValue::new(&mut create_state.slot_count).speed(1.0).clamp_range(2..=4).suffix(" players"));
                    let enabled = create_state.name.trim().len() > 0;
                    
                    let create_btn = ui.add(egui::Button::new("Create").enabled(enabled)).on_disabled_hover_text("Give your room a name");
                    
                    if create_btn.clicked() {
                        //self.create_room(&create_state.name, create_state.slot_count);
                        self.state = create_state.create(self.p2p.clone());
                    } else if join_btn.clicked() {
                        //TODO: Check that the id is a PeerId
                        self.state = create_state.join(self.p2p.clone());
                    }

                },
                State::StartGame(start_game_state) => {
                    let state = start_game_state.current_state.borrow().clone();
                    match state {
                        GameState::Initializing => {
                            ui.label(format!("State: Lobby is initializing!"));
                        },
                        GameState::New(slots) => {
                            for (idx, slot) in slots.iter().enumerate() {
                                match slot {
                                    crate::network::p2p::Slot::Vacant() => {
                                        ui.label(format!("Slot - Free"));
                                        if ui.button("Claim").clicked() {
                                            start_game_state.claim(idx);
                                        }
                                    },
                                    crate::network::p2p::Slot::Occupied(participant) => {
                                        let name = match participant {
                                            crate::network::p2p::Participant::Local(id) => {
                                                format!("You! ({})", id)
                                            },
                                            crate::network::p2p::Participant::Remote(id, _) => {
                                                format!("{}", id)
                                            },
                                        };
                                        ui.label(format!("Slot - {:?}", name));
                                    },
                                }
                                
                            }
                        },
                        GameState::Ready(ready_state) => {
                            ui.label(format!("State: Lobby is ready! - {:?}", ready_state));
                            if ui.button("Start game!").clicked() {
                                let (mut session, local_handle) = self.p2p.start_session(ready_state);
                                session.set_fps(60).unwrap();
                                session.set_frame_delay(4, local_handle).unwrap();
                                session.start_session().expect("Could not start P2P session");
                        
                                *game_runner_state = GameRunnerState::Playing(MyGameState::new(), PlayState::NetPlay(NetPlayState { session, local_handle, frames_to_skip: 0, frame: 0 }));
                            }
                        },
                    }
                }
            }
        });
    }
}

struct OneShotReceiver<T> {
    receiver: tokio::sync::oneshot::Receiver<T>,
    state: Option<T>
}

impl<T> OneShotReceiver<T> {
    fn new(receiver: tokio::sync::oneshot::Receiver<T>) -> Self {
        Self { receiver, state: None }
    }

    fn get(&mut self) -> &Option<T> {
        if let (None, Ok(v)) = (&self.state, self.receiver.try_recv()) {
            self.state = Some(v);
        }
        &self.state
    }
}
