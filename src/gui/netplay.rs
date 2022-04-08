use egui::{Button, Context, DragValue, ScrollArea, TextEdit, Window};
use libp2p::PeerId;

use crate::{
    network::p2p::{GameState, P2PGame, P2P},
    GameRunnerState, MyGameState, NetPlayState, PlayState,
};

enum State {
    CreateGame(CreateGameState),
    StartGame(StartGameState),
}

struct StartGameState {
    game: P2PGame,
    current_state: tokio::sync::watch::Receiver<GameState>,
}

impl StartGameState {
    fn ted(p2p: &P2P, game: &P2PGame) -> tokio::sync::watch::Receiver<GameState> {
        let (sender, current_state) = tokio::sync::watch::channel(GameState::Initializing);
        let mut p2p = p2p.clone();
        let game = game.clone();

        tokio::spawn(async move {
            loop {
                if sender.send(game.current_state(&mut p2p).await).is_err() {
                    break; //No one is listening any longer.
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            println!("Broke out");
        });
        current_state
    }

    fn create(p2p: &P2P, name: &str, slot_count: u8) -> Self {
        let game = p2p.create_game(name, slot_count);

        Self {
            current_state: StartGameState::ted(p2p, &game),
            game,
        }
    }

    fn join(p2p: &P2P, owner_id: &PeerId) -> Self {
        let game = p2p.join_game(owner_id);

        Self {
            current_state: StartGameState::ted(p2p, &game),
            game,
        }
    }

    fn claim(&mut self, idx: usize) {
        self.game.claim_slot(idx);
    }
}

struct CreateGameState {
    //Create
    name: String,
    slot_count: u8,

    //Join
    search_string: String,
    search_result: Option<OneShotReceiver<Vec<(String, PeerId)>>>,
}

impl CreateGameState {
    fn new() -> Self {
        Self {
            name: "test".to_owned(),
            slot_count: 2,
            search_string: "".to_owned(),
            search_result: None,
        }
    }
    fn create(&self, p2p: &P2P) -> State {
        State::StartGame(StartGameState::create(p2p, &self.name, self.slot_count))
    }

    fn join(&self, p2p: &P2P, owner_id: PeerId) -> State {
        State::StartGame(StartGameState::join(p2p, &owner_id))
    }

    fn search(&mut self, p2p: &P2P) {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let search_string = self.search_string.clone();
        let p2p = p2p.clone();
        tokio::spawn(async move {
            let _ = sender.send(p2p.find_games(&search_string).await);
        });
        self.search_result = Some(OneShotReceiver::new(receiver));
    }
}

pub(crate) struct NetplayGui {
    state: State,
    p2p: P2P,
}

impl NetplayGui {
    pub(crate) fn new(p2p: P2P) -> Self {
        Self {
            state: State::CreateGame(CreateGameState::new()),
            p2p,
        }
    }

    pub(crate) fn handle_event(&mut self, _: &winit::event::WindowEvent) {}

    pub(crate) fn ui(&mut self, ctx: &Context, game_runner_state: &mut GameRunnerState) {
        Window::new("Netplay!").collapsible(false).show(ctx, |ui| {
            let state = &mut self.state;
            match state {
                State::CreateGame(create_state) => {
                    let mut new_state = None;
                    ui.add(
                        TextEdit::singleline(&mut create_state.search_string)
                            .hint_text("Search for room"),
                    );
                    if ui.button("Search").clicked() {
                        create_state.search(&self.p2p);
                    }

                    if let Some(result) = &mut create_state.search_result {
                        if let Some(results) = result.get().clone() {
                            let scroll_area = ScrollArea::vertical().max_height(100.0);

                            scroll_area.show(ui, |ui| {
                                ui.vertical(|ui| {
                                    for (name, peer_id) in results {
                                        ui.label(format!("{:?}'", name));
                                        if ui.button("Join").clicked() {
                                            new_state = Some(create_state.join(&self.p2p, peer_id));
                                        }
                                    }
                                });
                            });
                        }
                    }

                    ui.add(
                        TextEdit::singleline(&mut create_state.name)
                            .hint_text("Name your new room"),
                    );
                    ui.add(
                        DragValue::new(&mut create_state.slot_count)
                            .speed(1.0)
                            .clamp_range(2..=4)
                            .suffix(" players"),
                    );
                    let enabled = !create_state.name.trim().is_empty();

                    if ui
                        .add_enabled(enabled, Button::new("Create"))
                        .on_disabled_hover_text("Give your room a name")
                        .clicked()
                    {
                        new_state = Some(create_state.create(&self.p2p));
                    }

                    if let Some(new_state) = new_state {
                        self.state = new_state;
                    }
                }
                State::StartGame(start_game_state) => {
                    let state = start_game_state.current_state.borrow().clone();
                    match state {
                        GameState::Initializing => {
                            ui.label("State: Lobby is initializing!".to_string());
                        }
                        GameState::New(slots) => {
                            for (idx, slot) in slots.iter().enumerate() {
                                match slot {
                                    crate::network::p2p::Slot::Vacant() => {
                                        ui.label("Slot - Free".to_string());
                                        if ui.button("Claim").clicked() {
                                            start_game_state.claim(idx);
                                        }
                                    }
                                    crate::network::p2p::Slot::Occupied(participant) => {
                                        let name = match participant {
                                            crate::network::p2p::Participant::Local(id) => {
                                                format!("You! ({})", id)
                                            }
                                            crate::network::p2p::Participant::Remote(peer, _) => {
                                                format!("{}", peer.id)
                                            }
                                        };
                                        ui.label(format!("Slot - {:?}", name));
                                    }
                                }
                            }
                        }
                        GameState::Ready(ready_state) => {
                            ui.label(format!("State: Lobby is ready! - {:?}", ready_state));
                            if ui.button("Start game!").clicked() {
                                let (mut session, local_handle) =
                                    self.p2p.create_session(&ready_state); // create a GGRS session
                                *game_runner_state = GameRunnerState::Playing(
                                    MyGameState::new(),
                                    PlayState::NetPlay(NetPlayState {
                                        session,
                                        player_count: ready_state.players.len(),
                                        local_handle,
                                        frame: 0,
                                    }),
                                );
                            }
                        }
                    }
                }
            }
        });
    }
}

struct OneShotReceiver<T> {
    receiver: tokio::sync::oneshot::Receiver<T>,
    state: Option<T>,
}

impl<T> OneShotReceiver<T> {
    fn new(receiver: tokio::sync::oneshot::Receiver<T>) -> Self {
        Self {
            receiver,
            state: None,
        }
    }

    fn get(&mut self) -> &Option<T> {
        if let (None, Ok(v)) = (&self.state, self.receiver.try_recv()) {
            self.state = Some(v);
        }
        &self.state
    }
}
