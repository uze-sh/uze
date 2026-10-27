// The `gpu` source: one reading of `nvidia-smi`, and the readings before
// it, kept in the store.
fn field(parts, at) {
    let text = parts[at];
    text.trim();
    text
}

// A function sees nothing outside it, so what it asks and how much it
// keeps are written here.
fn answer(input, context) {
    let query = "--query-gpu=name,utilization.gpu,memory.used,memory.total,temperature.gpu";
    let kept_readings = 30;
    let ran = run_program("nvidia-smi", [query, "--format=csv,noheader,nounits"]);
    if ran.code != 0 {
        throw `nvidia-smi answered ${ran.code}: ${ran.stderr}`;
    }
    let line = lines(ran.stdout)[0];
    let parts = line.split(",");
    let load = parse_int(field(parts, 1));
    let used = parse_int(field(parts, 2));
    let total = parse_int(field(parts, 3));
    let temperature = parse_float(field(parts, 4));

    let kept = store();
    let history = if "history" in kept { kept.history } else { [] };
    history.push(load);
    while history.len() > kept_readings {
        history.remove(0);
    }

    let warn_at = input.preferences["warn-at"];
    #{
        data: #{
            warn: warn_at.to_float() / 100.0,
            name: field(parts, 0),
            load: load,
            load_ratio: load.to_float() / 100.0,
            memory: `${used} / ${total} MiB`,
            memory_ratio: used.to_float() / total.to_float(),
            temperature: `${temperature.round()}°C`,
            history: history,
        },
        store: #{ history: history },
    }
}
