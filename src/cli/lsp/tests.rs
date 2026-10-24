use std::time::Duration;

use lsp_server::{Connection, Message, Notification, Request, RequestId};
use serde_json::{Value, json};

use super::*;

const MANIFEST: &str = r#"protocol 1
id "board"
name "Board"
description "A board"
surface "board" slot="view" view="board.kdl" source="board"
source "board" { fixture "fixtures/board.json"; }
"#;

/// An extension on disk, and a server on the other end of `client`.
fn served(root: &Path) -> (Connection, std::thread::JoinHandle<()>, PathBuf) {
    let extension = root.join("greet/extensions/board");
    std::fs::create_dir_all(extension.join("fixtures")).unwrap();
    std::fs::write(extension.join("extension.kdl"), MANIFEST).unwrap();
    std::fs::write(
        extension.join("board.kdl"),
        "Stack {\n    Text \"{title}\"\n}\n",
    )
    .unwrap();
    std::fs::write(
        extension.join("fixtures/board.json"),
        r#"{"title": "Week", "current": {"name": "x"}}"#,
    )
    .unwrap();
    let (server, client) = Connection::memory();
    let home = uze_application::UzeHome::at(root.join("home"));
    let thread = std::thread::spawn(move || {
        let app = UzeApplication::new(home, Vec::new());
        run(&app, &server).expect("the server runs");
    });
    (client, thread, extension)
}

fn request(client: &Connection, id: i32, method: &str, params: Value) -> Value {
    client
        .sender
        .send(Message::Request(Request::new(
            RequestId::from(id),
            method.to_owned(),
            params,
        )))
        .unwrap();
    loop {
        match client
            .receiver
            .recv_timeout(Duration::from_secs(30))
            .expect("an answer")
        {
            Message::Response(response) if response.id == RequestId::from(id) => {
                return response.response_result.unwrap_or(Value::Null);
            }
            _ => {}
        }
    }
}

fn notify(client: &Connection, method: &str, params: Value) {
    client
        .sender
        .send(Message::Notification(Notification::new(
            method.to_owned(),
            params,
        )))
        .unwrap();
}

fn diagnostics_for(client: &Connection, uri: &str) -> Vec<Value> {
    loop {
        match client
            .receiver
            .recv_timeout(Duration::from_secs(60))
            .expect("diagnostics")
        {
            Message::Notification(notification)
                if notification.method == "textDocument/publishDiagnostics"
                    && notification.params["uri"] == uri =>
            {
                return notification.params["diagnostics"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
            }
            _ => {}
        }
    }
}

#[test]
fn an_editor_gets_the_checks_diagnostics_completion_and_hover() {
    let root = uze_testkit::temp::scratch("lsp-session");
    let (client, thread, extension) = served(&root);
    let capabilities = request(&client, 1, "initialize", json!({ "capabilities": {} }));
    assert_eq!(capabilities["capabilities"]["hoverProvider"], true);
    notify(&client, "initialized", json!({}));

    let view = extension.join("board.kdl");
    let uri = uri_of(&view);
    notify(
        &client,
        "textDocument/didOpen",
        json!({ "textDocument": { "uri": uri, "languageId": "kdl", "version": 1, "text": "Tabel {\n}\n" } }),
    );
    let said = diagnostics_for(&client, &uri);
    assert!(
        said.iter().any(|diagnostic| diagnostic["severity"] == 1
            && diagnostic["message"]
                .as_str()
                .unwrap()
                .contains("did you mean `Table`?")
            && diagnostic["range"]["start"]["line"] == 0),
        "{said:#?}"
    );

    notify(
        &client,
        "textDocument/didChange",
        json!({ "textDocument": { "uri": uri, "version": 2 }, "contentChanges": [{ "text": "Stack {\n    Progress ratio=0.5\n    Text \"{title}\"\n}\n" }] }),
    );
    let fixed = diagnostics_for(&client, &uri);
    assert!(fixed.iter().all(|d| d["severity"] != 1), "{fixed:#?}");
    let offered = request(
        &client,
        2,
        "textDocument/completion",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 1, "character": 18 } }),
    );
    let labels: Vec<&str> = offered
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["label"].as_str().unwrap())
        .collect();
    assert!(
        labels.contains(&"ratio") && !labels.contains(&"truncate"),
        "{labels:?}"
    );
    let fields = request(
        &client,
        3,
        "textDocument/completion",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 2, "character": 11 } }),
    );
    let labels: Vec<&str> = fields
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["label"].as_str().unwrap())
        .collect();
    assert!(
        labels.contains(&"current") && labels.contains(&"title"),
        "{labels:?}"
    );

    let hovered = request(
        &client,
        4,
        "textDocument/hover",
        json!({ "textDocument": { "uri": uri }, "position": { "line": 1, "character": 7 } }),
    );
    assert!(
        hovered["contents"]["value"]
            .as_str()
            .unwrap()
            .contains("Progress"),
        "{hovered}"
    );

    request(&client, 5, "shutdown", Value::Null);
    notify(&client, "exit", Value::Null);
    thread.join().unwrap();
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_uri_and_a_path_go_back_and_forth() {
    let path = PathBuf::from("/tmp/a b/extensions/x/board.kdl");
    let uri = uri_of(&path);
    assert_eq!(uri, "file:///tmp/a%20b/extensions/x/board.kdl");
    assert_eq!(path_of(&uri), Some(path));
    assert_eq!(
        path_of("file:///C:/x/y.kdl"),
        Some(PathBuf::from("C:/x/y.kdl"))
    );
}
