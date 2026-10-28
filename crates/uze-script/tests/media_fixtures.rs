//! The media reference scripts (`tests/_fixtures/canonical/media`) do
//! what their extensions promise: a plugin check runs no script and keeps
//! no resident session, so `snake`'s game and `pets`' kennel are played
//! here, through the same session and engine the workspace runs them in.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use uze_script::{
    CheckoutPath, Entry, Grant, Grants, Reach, ReachError, Resident, ResidentScript, Script,
};

fn file(extension: &str, name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/_fixtures/canonical/media/extensions")
        .join(extension)
        .join(name)
}

/// An extension's store and nothing of a checkout.
struct Kept(Mutex<Value>);

impl Kept {
    fn holding(store: Value) -> Arc<Self> {
        Arc::new(Self(Mutex::new(store)))
    }
}

impl Reach for Kept {
    fn read_checkout(&self, _: &CheckoutPath) -> Result<String, ReachError> {
        Err(ReachError("no checkout".to_owned()))
    }

    fn list_checkout(&self, _: &CheckoutPath) -> Result<Vec<Entry>, ReachError> {
        Ok(Vec::new())
    }

    fn store(&self) -> Result<Value, ReachError> {
        Ok(self.0.lock().unwrap().clone())
    }
}

fn snake(store: Value) -> Resident {
    let component = std::fs::read(file("snake", "snake.uze")).unwrap();
    let script = ResidentScript::compile(&uze_script::script_of("snake.uze", &component).unwrap())
        .expect("snake.uze compiles as a resident script");
    let grants = Grants::from([Grant::Resident, Grant::Store]);
    let (game, started) = Resident::start(
        Arc::new(script),
        &grants,
        Kept::holding(store),
        &json!({ "slot": "view", "size": { "width": 80, "height": 20 } }),
    );
    assert_eq!(started.outcome.unwrap().data, json!(true), "start draws");
    game
}

fn key(game: &Resident, key: &str) -> uze_script::Envelope {
    game.input(&json!({ "key": key, "mods": [] }))
        .outcome
        .unwrap()
}

fn tick(game: &Resident) -> uze_script::Envelope {
    game.event(&json!({ "name": "timer" })).outcome.unwrap()
}

fn frame(game: &Resident) -> Value {
    game.render(80, 20).outcome.unwrap().data
}

/// The header's three texts: the score, the best, and what to do.
fn header(frame: &Value) -> Vec<String> {
    frame["children"][0]["children"]
        .as_array()
        .unwrap()
        .iter()
        .map(|text| text["args"][0].as_str().unwrap().to_owned())
        .collect()
}

/// The board's rows as text, two characters a square, inside its border.
fn board(frame: &Value) -> Vec<String> {
    let rows = frame["children"][1]["props"]["rows"].as_array().unwrap();
    rows.iter()
        .map(|row| {
            row.as_array()
                .unwrap()
                .iter()
                .map(|cell| cell.as_str().or(cell["ch"].as_str()).unwrap())
                .collect()
        })
        .collect()
}

/// The first frame is the one the snake fixture's tests draw: what the
/// view shows before an arrow is pressed.
#[test]
fn snake_waits_for_an_arrow_with_the_best_score_its_store_kept() {
    let game = snake(json!({ "best": 4 }));
    let first = frame(&game);
    assert_eq!(
        header(&first),
        ["score 0", "best 4", "an arrow starts · esc lets go"]
    );
    let rows = board(&first);
    assert_eq!(rows.len(), 12, "ten rows inside a border");
    assert_eq!(rows[3], format!("│{}●{}│", " ".repeat(10), " ".repeat(29)));
    assert_eq!(
        rows[6],
        format!("│{}{}{}│", " ".repeat(6), "█".repeat(6), " ".repeat(28))
    );
    let recorded: Value = serde_json::from_str(
        &std::fs::read_to_string(file("snake", "fixtures/start.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(first, recorded, "the fixture is the frame the script draws");

    assert!(tick(&game).timer.is_none(), "nothing moves before an arrow");
    assert!(
        !key(&game, "left").data.as_bool().unwrap(),
        "never backwards"
    );
    assert!(
        key(&game, "q").data == json!(false),
        "a key it does not use"
    );
}

#[test]
fn snake_scores_on_the_first_up_keeps_the_best_and_plays_again() {
    let game = snake(json!({}));
    let started = key(&game, "up");
    assert_eq!(started.data, json!(true));
    assert_eq!(started.timer, Some(150), "an arrow starts the clock");

    assert_eq!(tick(&game).data, json!(true));
    assert_eq!(tick(&game).data, json!(true));
    let ate = tick(&game);
    assert_eq!(ate.data, json!(true));
    assert_eq!(
        ate.store.as_deref(),
        Some(json!({ "best": 1 }).as_object().unwrap()),
        "a new best is kept at once"
    );
    assert_eq!(header(&frame(&game))[..2], ["score 1", "best 1"]);

    // Straight on into the top wall.
    let mut ticks = 0;
    loop {
        let moved = tick(&game);
        ticks += 1;
        if moved.timer.is_none() {
            break;
        }
        assert!(ticks < 10, "the wall is three squares up");
    }
    let over = frame(&game);
    assert_eq!(
        header(&over),
        ["score 1", "best 1", "game over · space plays again"]
    );
    assert_eq!(key(&game, "up").data, json!(false), "over until space");
    assert_eq!(key(&game, "space").data, json!(true));
    assert_eq!(header(&frame(&game))[..2], ["score 0", "best 1"]);
}

#[test]
fn snake_draws_within_its_bounds_at_every_tick_of_a_long_game() {
    let game = snake(json!({}));
    key(&game, "up");
    let turns = ["right", "down", "left", "up"];
    for at in 0..200 {
        if at % 5 == 0 {
            key(&game, turns[(at / 5) % 4]);
        }
        tick(&game);
        assert!(game.render(80, 20).outcome.is_ok(), "tick {at}");
    }
    assert!(game.stop().outcome.is_ok());
}

fn kennel_script(name: &str) -> Script {
    let component = std::fs::read(file("pets", name)).unwrap();
    Script::compile(&uze_script::script_of(name, &component).unwrap()).unwrap()
}

#[test]
fn pets_feeds_the_pet_and_the_kennel_says_it_is_content() {
    let grants = Grants::from([Grant::Store]);
    let kennel = kennel_script("kennel.uze");
    let feed = kennel_script("feed.uze");
    let ask = |script: &Script, store: Value| {
        script
            .answer(&json!({}), &json!({}), &grants, Kept::holding(store))
            .outcome
            .unwrap()
    };

    let hungry = ask(&kennel, json!({}));
    assert_eq!(
        hungry.data,
        json!({ "name": "Pixel", "fed": 0, "mood": "hungry", "said": "hungry" })
    );
    assert_eq!(
        hungry.store.as_deref(),
        Some(json!({ "name": "Pixel" }).as_object().unwrap()),
        "its name is kept"
    );

    let fed = ask(&feed, json!({ "name": "Pixel", "fed": 2 }));
    let patch = fed.store.expect("a meal is kept");
    assert_eq!(patch["fed"], json!(3));
    assert!(patch["last_fed"].as_i64().unwrap() > 0);
    assert_eq!(fed.refresh, ["kennel"]);

    let content = ask(
        &kennel,
        json!({ "name": "Biscuit", "fed": 3, "last_fed": patch["last_fed"] }),
    );
    assert_eq!(content.data["name"], "Biscuit");
    assert_eq!(content.data["mood"], "content");
    assert_eq!(content.data["said"], "content, fed 3 times");
}
