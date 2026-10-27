//! A sample of Rhai, the language a declared extension's scripts are in.
import "helpers" as helpers;

/// Counts the open tasks, skipping the ones marked done.
private fn open_tasks(tasks) {
    let count = 0;
    for task in tasks {
        if task?.done ?? false { continue; }
        count += 1;
    }
    count
}

fn answer(input, context) {
    const LIMIT = 1_000;
    let mask = 0xFF & 0o17 | 0b1010;
    let ratio = 2.5e-1;
    let label = switch input.kind {
        "board" => `Board: ${input.title} (${ open_tasks(input.tasks) } open)`,
        _ => "unknown \u0041 kind",
    };
    let raw = #"C:\no\escape"#;
    let add = |a, b| a + b;
    let doubled = [1, 2, 3].map(|x| x * 2);
    /* a block /* nested */ comment */
    if type_of(input) == "map" && is_def_fn("open_tasks", 1) {
        print(`${label} / ${raw}`);
    }
    #{
        data: #{ label: label, ratio: ratio, mask: mask, range: 0..LIMIT, sum: add.call(1, 2) },
        refresh: false,
        empty: (),
    }
}
